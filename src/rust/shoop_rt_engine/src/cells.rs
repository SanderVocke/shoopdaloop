//! Latest-value cells that the render side writes without blocking and the
//! control side reads when it assembles telemetry. Fields are independent
//! atomics: a record may combine values from neighbouring callbacks.

use alloc::boxed::Box;

use crate::stream::ByteRing;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use rt_runtime::Frame;
use shoop_loops::runtime::BoundaryObserver;
use shoop_loops::{Decision, Event, Mode};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus, Peaks, MAX_STRIP_CHANNELS};

/// Per-channel absolute peaks since the last read.
#[derive(Default)]
pub(crate) struct Peak([AtomicU32; MAX_STRIP_CHANNELS]);

impl Peak {
    pub(crate) fn raise(&self, channel: usize, value: f32) {
        if value.is_finite() && value > 0. {
            if let Some(cell) = self.0.get(channel) {
                cell.fetch_max(value.to_bits(), Ordering::Relaxed);
            }
        }
    }
    pub(crate) fn take(&self, channels: usize) -> Peaks {
        let mut values = [0.; MAX_STRIP_CHANNELS];
        let channels = channels.min(MAX_STRIP_CHANNELS);
        for (value, cell) in values.iter_mut().zip(&self.0) {
            *value = f32::from_bits(cell.swap(0, Ordering::Relaxed));
        }
        Peaks::new(&values[..channels]).expect("bounded channels")
    }
    pub(crate) fn clear(&self) {
        for cell in &self.0 {
            cell.store(0, Ordering::Relaxed);
        }
    }
}

pub(crate) struct StripCells {
    pub input: Peak,
    pub output: Peak,
    pub midi_in: AtomicBool,
    pub midi_out: AtomicBool,
    pub latest_input_midi: AtomicU64,
    pub owner_generation: AtomicU32,
    pub processor_demand: AtomicBool,
    pub routing_state: AtomicU32,
    pub forced_monitor_off: AtomicU32,
    pub wet_gate: Box<[AtomicU32]>,
}
impl StripCells {
    pub(crate) fn observe_midi(&self, packet: &[u8]) {
        if packet.is_empty() || packet.len() > 4 {
            return;
        }
        let mut value = (packet.len() as u64) << 32;
        for (index, byte) in packet.iter().enumerate() {
            value |= u64::from(*byte) << (8 * index);
        }
        self.latest_input_midi.store(value, Ordering::Relaxed);
    }
    pub(crate) fn latest_midi(&self) -> Option<shoop_rt_protocol::MidiMessage> {
        let value = self.latest_input_midi.load(Ordering::Relaxed);
        let len = (value >> 32) as usize;
        if len == 0 || len > 4 {
            return None;
        }
        shoop_rt_protocol::MidiMessage::new(&value.to_le_bytes()[..len])
    }
    fn new(frames: usize) -> Self {
        Self {
            input: Peak::default(),
            output: Peak::default(),
            midi_in: AtomicBool::new(false),
            midi_out: AtomicBool::new(false),
            latest_input_midi: AtomicU64::new(0),
            owner_generation: AtomicU32::new(0),
            processor_demand: AtomicBool::new(false),
            routing_state: AtomicU32::new(0),
            forced_monitor_off: AtomicU32::new(0),
            wet_gate: (0..frames).map(|_| AtomicU32::new(0)).collect(),
        }
    }
}

#[derive(Default)]
pub(crate) struct LoopCells {
    pub recorded: crate::recorded::TakeCells,
    pub generation: AtomicU32,
    pub mode: AtomicU32,
    pub length: AtomicU64,
    pub content_revision: AtomicU64,
    pub position: AtomicU64,
    /// Clock edges this loop emitted, counted at settled boundaries.
    pub pulses: AtomicU64,
    pub peaks: Peak,
}

pub(crate) fn mode_code(mode: Mode) -> u32 {
    match mode {
        Mode::Stopped => 0,
        Mode::Playing => 1,
        Mode::Recording => 2,
        Mode::PlayingDryThroughWet => 3,
        Mode::RecordingDryIntoWet => 4,
    }
}
pub(crate) fn mode_from_code(code: u32) -> Mode {
    match code {
        1 => Mode::Playing,
        2 => Mode::Recording,
        3 => Mode::PlayingDryThroughWet,
        4 => Mode::RecordingDryIntoWet,
        _ => Mode::Stopped,
    }
}

pub(crate) struct Cells {
    pub profiling: Option<rt_runtime::profiling::Control>,
    #[cfg(test)]
    pub fail_reader_tick: AtomicBool,
    pub next_frame: AtomicU64,
    pub preview_finished: AtomicU64,
    pub preview_playing: AtomicU64,
    pub endpoint_alive: AtomicBool,
    /// Rendering faulted; the engine no longer executes control work.
    pub faulted: AtomicBool,
    pub strips: Box<[StripCells]>,
    pub loops: Box<[LoopCells]>,
    pub buses: Box<[Peak]>,
    /// Callbacks rendered, and the peak of every host input (0) and output (1).
    pub callbacks: AtomicU64,
    pub host_peaks: Peak,
    pub bulk: ByteRing,
    driver: [AtomicU32; 4],
    xruns: AtomicU32,
    load: AtomicU32,
}

impl Cells {
    pub(crate) fn new(
        strips: usize,
        loops: usize,
        buses: usize,
        bulk_bytes: usize,
        driver: DriverStatus,
        frames: usize,
    ) -> Arc<Self> {
        let cells = Self {
            profiling: None,
            #[cfg(test)]
            fail_reader_tick: AtomicBool::new(false),
            next_frame: AtomicU64::new(0),
            preview_finished: AtomicU64::new(0),
            preview_playing: AtomicU64::new(0),
            endpoint_alive: AtomicBool::new(true),
            faulted: AtomicBool::new(false),
            strips: (0..strips).map(|_| StripCells::new(frames)).collect(),
            loops: (0..loops).map(|_| LoopCells::default()).collect(),
            buses: (0..buses).map(|_| Peak::default()).collect(),
            callbacks: AtomicU64::new(0),
            host_peaks: Peak::default(),
            bulk: ByteRing::new(bulk_bytes),
            driver: Default::default(),
            xruns: AtomicU32::new(0),
            load: AtomicU32::new(0),
        };
        cells.set_driver(driver);
        Arc::new(cells)
    }
    pub(crate) fn set_driver(&self, driver: DriverStatus) {
        let kind = match driver.kind {
            DriverKind::Dummy => 0,
            DriverKind::Cpal => 1,
            DriverKind::WebAudio => 2,
            DriverKind::Jack => 3,
        };
        let state = match driver.state {
            DriverRunState::Starting => 0,
            DriverRunState::Running => 1,
            DriverRunState::Stopped => 2,
            DriverRunState::Failed => 3,
        };
        for (cell, value) in
            self.driver
                .iter()
                .zip([kind, state, driver.sample_rate, driver.buffer_frames])
        {
            cell.store(value, Ordering::Relaxed);
        }
    }
    pub(crate) fn driver(&self) -> DriverStatus {
        let [kind, state, sample_rate, buffer_frames] =
            core::array::from_fn(|index| self.driver[index].load(Ordering::Relaxed));
        DriverStatus {
            kind: match kind {
                1 => DriverKind::Cpal,
                2 => DriverKind::WebAudio,
                3 => DriverKind::Jack,
                _ => DriverKind::Dummy,
            },
            state: match state {
                0 => DriverRunState::Starting,
                1 => DriverRunState::Running,
                2 => DriverRunState::Stopped,
                _ => DriverRunState::Failed,
            },
            sample_rate,
            buffer_frames,
        }
    }
    pub(crate) fn xruns(&self) -> u32 {
        self.xruns.load(Ordering::Relaxed)
    }
    pub(crate) fn load(&self) -> f32 {
        f32::from_bits(self.load.load(Ordering::Relaxed))
    }
}

/// The host's handle for engine-owned driver status inputs. It may be used from
/// any thread; updates appear in the next engine telemetry.
#[derive(Clone)]
pub struct HostStatus(pub(crate) Arc<Cells>);

impl HostStatus {
    pub fn profiling(&self) -> Option<&rt_runtime::profiling::Control> {
        self.0.profiling.as_ref()
    }
    pub fn set_driver(&self, driver: DriverStatus) {
        self.0.set_driver(driver);
    }
    pub fn add_xruns(&self, count: u32) {
        self.0.xruns.fetch_add(count, Ordering::Relaxed);
        if count != 0 {
            if let Some(control) = &self.0.profiling {
                control.trigger(rt_runtime::profiling::HOST_XRUN);
            }
        }
    }
    /// Callback time as a share of the callback period.
    pub fn set_load(&self, load: f32) {
        self.0.load.store(load.to_bits(), Ordering::Relaxed);
    }
}

/// Publishes settled loop state at every boundary.
pub(crate) struct Observer(pub Arc<Cells>);

impl BoundaryObserver for Observer {
    fn boundary(&mut self, frame: Frame, decisions: &[Decision], _events: &[Event], _dropped: u64) {
        for decision in decisions {
            let state = decision.state;
            let Some(cells) = self.0.loops.get(state.id.slot() as usize) else {
                continue;
            };
            let generation = state.id.generation();
            if cells.generation.load(Ordering::Relaxed) != generation {
                cells.pulses.store(0, Ordering::Relaxed);
                cells.generation.store(generation, Ordering::Relaxed);
            }
            if state.last_pulse == Some(frame) {
                cells.pulses.fetch_add(1, Ordering::Relaxed);
            }
            cells.mode.store(mode_code(state.mode), Ordering::Relaxed);
            let length = if state.mode == Mode::Recording {
                state.capture_frames
            } else {
                state.content_length
            };
            cells.length.store(length, Ordering::Relaxed);
            cells
                .content_revision
                .store(state.content_revision.0, Ordering::Relaxed);
            cells.position.store(state.position, Ordering::Relaxed);
        }
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn shrinking_meter_width_discards_unreported_channels() {
        let peak = Peak::default();
        peak.raise(0, 0.25);
        peak.raise(1, 0.75);
        assert_eq!(peak.take(1).as_slice(), &[0.25]);
        assert_eq!(peak.take(2).as_slice(), &[0., 0.]);
        peak.raise(1, 0.5);
        assert!(peak.take(0).as_slice().is_empty());
        assert_eq!(peak.take(2).as_slice(), &[0., 0.]);
    }
}
