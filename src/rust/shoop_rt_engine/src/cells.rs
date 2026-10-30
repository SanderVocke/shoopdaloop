//! Latest-value cells that the render side writes without blocking and the
//! control side reads when it assembles telemetry. Fields are independent
//! atomics: a record may combine values from neighbouring callbacks.

use rt_runtime::Frame;
use shoop_loops::runtime::BoundaryObserver;
use shoop_loops::{Decision, Event, Mode};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus, Peaks, MAX_STRIP_CHANNELS};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

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
        for (value, cell) in values.iter_mut().zip(&self.0).take(channels) {
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

#[derive(Default)]
pub(crate) struct StripCells {
    pub input: Peak,
    pub output: Peak,
    pub midi_in: AtomicBool,
    pub midi_out: AtomicBool,
}

#[derive(Default)]
pub(crate) struct LoopCells {
    pub generation: AtomicU32,
    pub mode: AtomicU32,
    pub length: AtomicU64,
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
    }
}
pub(crate) fn mode_from_code(code: u32) -> Mode {
    match code {
        1 => Mode::Playing,
        2 => Mode::Recording,
        _ => Mode::Stopped,
    }
}

pub(crate) struct Cells {
    pub next_frame: AtomicU64,
    pub endpoint_alive: AtomicBool,
    pub strips: Box<[StripCells]>,
    pub loops: Box<[LoopCells]>,
    pub master: Peak,
    driver: [AtomicU32; 4],
    xruns: AtomicU32,
    load: AtomicU32,
}

impl Cells {
    pub(crate) fn new(strips: usize, loops: usize, driver: DriverStatus) -> Arc<Self> {
        let cells = Self {
            next_frame: AtomicU64::new(0),
            endpoint_alive: AtomicBool::new(true),
            strips: (0..strips).map(|_| StripCells::default()).collect(),
            loops: (0..loops).map(|_| LoopCells::default()).collect(),
            master: Peak::default(),
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
            std::array::from_fn(|index| self.driver[index].load(Ordering::Relaxed));
        DriverStatus {
            kind: match kind {
                1 => DriverKind::Cpal,
                2 => DriverKind::WebAudio,
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
    pub fn set_driver(&self, driver: DriverStatus) {
        self.0.set_driver(driver);
    }
    pub fn add_xruns(&self, count: u32) {
        self.0.xruns.fetch_add(count, Ordering::Relaxed);
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
            cells.position.store(state.position, Ordering::Relaxed);
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
