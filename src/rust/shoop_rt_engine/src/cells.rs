//! Latest-value cells that the render side writes without blocking and the
//! control side reads when it assembles telemetry. Loop snapshots use one
//! nonblocking publication check; peaks remain independent latest-value cells.

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

#[derive(Default)]
struct MonitoringPolicy {
    generation: AtomicU32,
    enabled: AtomicBool,
}

#[derive(Default)]
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
    policies: [MonitoringPolicy; 2],
    pub automatic_demand: AtomicBool,
    pub automatic_monitoring: AtomicBool,
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
            policies: Default::default(),
            automatic_demand: AtomicBool::new(false),
            automatic_monitoring: AtomicBool::new(false),
        }
    }
    pub(crate) fn configure_policy(&self, generation: u32, active_generation: u32, enabled: bool) {
        if generation == active_generation {
            return;
        }
        let bank =
            usize::from(self.policies[0].generation.load(Ordering::Acquire) == active_generation);
        let policy = &self.policies[bank];
        policy.generation.store(0, Ordering::Release);
        policy.enabled.store(enabled, Ordering::Relaxed);
        policy.generation.store(generation, Ordering::Release);
    }
    pub(crate) fn set_policy(&self, generation: u32, enabled: bool) -> bool {
        if let Some(policy) = self
            .policies
            .iter()
            .find(|policy| policy.generation.load(Ordering::Acquire) == generation)
        {
            policy.enabled.store(enabled, Ordering::Release);
            true
        } else {
            false
        }
    }
    pub(crate) fn policy_enabled(&self, generation: u32) -> bool {
        generation != 0
            && self.policies.iter().any(|policy| {
                policy.generation.load(Ordering::Acquire) == generation
                    && policy.enabled.load(Ordering::Acquire)
            })
    }
}

#[derive(Default)]
struct MonitoringOwner {
    generation: AtomicU32,
    strip: AtomicU32,
}

#[derive(Default)]
pub(crate) struct LoopCells {
    pub recorded: crate::recorded::TakeCells,
    owners: [MonitoringOwner; 2],
    pub publication: AtomicU64,
    epoch: AtomicU64,
    frame: AtomicU64,
    iteration: AtomicU64,
    passes: AtomicU64,
    waiting_repeat: AtomicBool,
    pub timing_revision: AtomicU64,
    pub generation: AtomicU32,
    pub mode: AtomicU32,
    pub length: AtomicU64,
    pub content_revision: AtomicU64,
    capture_incomplete: AtomicBool,
    pub position: AtomicU64,
    pending_mode: AtomicU32,
    pending_delay: AtomicU32,
    /// Clock edges this loop emitted, counted at settled boundaries.
    pub pulses: AtomicU64,
    pub peaks: Peak,
}

#[derive(Clone, Copy)]
pub(crate) struct LoopSnapshot {
    pub epoch: u64,
    pub frame: u64,
    pub generation: u32,
    pub mode: Mode,
    pub length: u64,
    pub revision: u64,
    pub capture_incomplete: bool,
    pub position: u64,
    pub pending: Option<(u32, Mode)>,
    pub iteration: u64,
    pub passes: u64,
    pub waiting_repeat: bool,
    pub timing_revision: u64,
}
impl LoopCells {
    pub(crate) fn configure_owner(&self, generation: u32, strip: u32, active_generation: u32) {
        if generation == active_generation {
            return;
        }
        let bank =
            usize::from(self.owners[0].generation.load(Ordering::Acquire) == active_generation);
        let owner = &self.owners[bank];
        owner.generation.store(0, Ordering::Release);
        owner.strip.store(strip, Ordering::Relaxed);
        owner.generation.store(generation, Ordering::Release);
    }
    fn owner(&self, generation: u32) -> Option<usize> {
        self.owners.iter().find_map(|owner| {
            (owner.generation.load(Ordering::Acquire) == generation)
                .then(|| owner.strip.load(Ordering::Relaxed) as usize)
        })
    }
    pub(crate) fn snapshot(&self) -> Option<LoopSnapshot> {
        let before = self.publication.load(Ordering::SeqCst);
        if before & 1 != 0 {
            return None;
        }
        let snapshot = LoopSnapshot {
            timing_revision: self.timing_revision.load(Ordering::SeqCst),
            epoch: self.epoch.load(Ordering::SeqCst),
            frame: self.frame.load(Ordering::SeqCst),
            generation: self.generation.load(Ordering::SeqCst),
            mode: mode_from_code(self.mode.load(Ordering::SeqCst)),
            length: self.length.load(Ordering::SeqCst),
            revision: self.content_revision.load(Ordering::SeqCst),
            capture_incomplete: self.capture_incomplete.load(Ordering::SeqCst),
            position: self.position.load(Ordering::SeqCst),
            pending: self
                .pending_mode
                .load(Ordering::SeqCst)
                .checked_sub(1)
                .map(|mode| {
                    (
                        self.pending_delay.load(Ordering::SeqCst),
                        mode_from_code(mode),
                    )
                }),
            iteration: self.iteration.load(Ordering::SeqCst),
            passes: self.passes.load(Ordering::SeqCst),
            waiting_repeat: self.waiting_repeat.load(Ordering::SeqCst),
        };
        core::sync::atomic::fence(Ordering::SeqCst);
        (self.publication.load(Ordering::SeqCst) == before).then_some(snapshot)
    }
}

pub(crate) fn mode_code(mode: Mode) -> u32 {
    match mode {
        Mode::Stopped => 0,
        Mode::Playing => 1,
        Mode::Recording => 2,
        Mode::Replacing => 5,
        Mode::PlayingDryThroughWet => 3,
        Mode::RecordingDryIntoWet => 4,
    }
}
pub(crate) fn mode_from_code(code: u32) -> Mode {
    match code {
        1 => Mode::Playing,
        2 => Mode::Recording,
        5 => Mode::Replacing,
        3 => Mode::PlayingDryThroughWet,
        4 => Mode::RecordingDryIntoWet,
        _ => Mode::Stopped,
    }
}

pub(crate) struct Cells {
    pub memory: crate::memory::Memory,
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
            memory: Default::default(),
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
    /// The largest load since the previous call.
    pub(crate) fn take_load(&self) -> f32 {
        f32::from_bits(self.load.swap(0, Ordering::Relaxed))
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
    /// Callback time as a share of the callback period. Telemetry reports the largest
    /// since its previous record.
    pub fn set_load(&self, load: f32) {
        // Non-negative floats order like their bit patterns.
        self.0
            .load
            .fetch_max(load.max(0.).to_bits(), Ordering::Relaxed);
    }
}

/// Publishes settled loop state at every boundary.
pub(crate) struct Observer(
    pub Arc<Cells>,
    pub rt_runtime::Epoch,
    pub(crate) super::monitoring::Demand,
    Box<[Option<(u32, u32, Mode)>]>,
);
impl Observer {
    pub(crate) fn new(cells: Arc<Cells>) -> Self {
        let demand = super::monitoring::Demand::new(cells.loops.len());
        let pending = alloc::vec![None; cells.loops.len()].into_boxed_slice();
        Self(cells, rt_runtime::Epoch(0), demand, pending)
    }
}

impl BoundaryObserver for Observer {
    fn settled(&mut self, resolver: &shoop_loops::Resolver) {
        self.2.update(resolver);
        self.3.fill(None);
        for pending in resolver.pending_transitions() {
            use shoop_loops::Transition;
            let mode = match pending.request.transition {
                Transition::Stop => Mode::Stopped,
                Transition::Play { .. } => Mode::Playing,
                Transition::PlayDry { .. } => Mode::PlayingDryThroughWet,
                Transition::Record { .. } | Transition::RecordPrefix { .. } => Mode::Recording,
                Transition::ReplaceInput { .. } => Mode::Replacing,
                Transition::Rerecord => Mode::RecordingDryIntoWet,
                Transition::Adopt { mode, .. } => mode,
                _ => continue,
            };
            let delay = pending.remaining_ticks.unwrap_or_else(|| {
                pending
                    .effective
                    .0
                    .saturating_sub(resolver.frame().0)
                    .div_ceil(resolver.quantum().frames() as u64)
            });
            let Ok(delay) = u32::try_from(delay) else {
                continue;
            };
            let Some(slot) = self.3.get_mut(pending.request.target.slot() as usize) else {
                continue;
            };
            if slot.is_none_or(|(_, previous, _)| delay < previous) {
                *slot = Some((pending.request.target.generation(), delay, mode));
            }
        }
        for strip in &self.0.strips {
            strip.automatic_demand.store(false, Ordering::Relaxed);
        }
        for node in resolver.topology().nodes() {
            if self.2.recording(node.id.slot() as usize) {
                if let Some(strip) = self.0.loops[node.id.slot() as usize]
                    .owner(node.id.generation())
                    .and_then(|strip| self.0.strips.get(strip))
                {
                    strip.automatic_demand.store(true, Ordering::Relaxed);
                }
            }
        }
    }
    fn topology_epoch(&mut self, epoch: rt_runtime::Epoch) {
        self.1 = epoch;
    }
    fn boundary(&mut self, frame: Frame, decisions: &[Decision], _events: &[Event], _dropped: u64) {
        for decision in decisions {
            let state = decision.state;
            let Some(cells) = self.0.loops.get(state.id.slot() as usize) else {
                continue;
            };
            let publication = cells.publication.fetch_add(1, Ordering::SeqCst);
            let generation = state.id.generation();
            if cells.generation.load(Ordering::SeqCst) != generation {
                cells.pulses.store(0, Ordering::SeqCst);
                cells.generation.store(generation, Ordering::SeqCst);
            }
            if state.last_pulse == Some(frame) {
                cells.pulses.fetch_add(1, Ordering::SeqCst);
            }
            let pending =
                self.3[state.id.slot() as usize].filter(|(owner, _, _)| *owner == generation);
            cells.pending_mode.store(
                pending.map_or(0, |(_, _, mode)| mode_code(mode) + 1),
                Ordering::SeqCst,
            );
            cells
                .pending_delay
                .store(pending.map_or(0, |(_, delay, _)| delay), Ordering::SeqCst);
            cells.mode.store(mode_code(state.mode), Ordering::SeqCst);
            cells
                .capture_incomplete
                .store(state.capture_incomplete, Ordering::SeqCst);
            let length = if state.mode == Mode::Recording {
                state.capture_frames
            } else {
                state.content_length
            };
            cells.length.store(length, Ordering::SeqCst);
            cells
                .content_revision
                .store(state.content_revision.0, Ordering::SeqCst);
            cells
                .timing_revision
                .store(state.timing_revision, Ordering::SeqCst);
            cells.position.store(state.position, Ordering::SeqCst);
            cells.epoch.store(self.1 .0, Ordering::SeqCst);
            cells.frame.store(frame.0, Ordering::SeqCst);
            let cursor = state.composite.unwrap_or_default();
            cells.iteration.store(cursor.iteration, Ordering::SeqCst);
            cells.passes.store(cursor.passes, Ordering::SeqCst);
            cells
                .waiting_repeat
                .store(cursor.waiting_repeat, Ordering::SeqCst);
            cells
                .publication
                .store(publication.wrapping_add(2), Ordering::SeqCst);
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

#[cfg(test)]
mod monitoring_tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn monitoring_owner_banks_follow_the_confirmed_model_even_when_observation_lags_or_a_candidate_reuses_its_id(
    ) {
        let cells = LoopCells::default();
        assert_no_heap(|| {
            cells.configure_owner(1, 0, 0);
            cells.configure_owner(2, 1, 1);
            assert_eq!(cells.owner(1), Some(0));
            assert_eq!(cells.owner(2), Some(1));
            cells.configure_owner(2, 2, 1);
            assert_eq!(cells.owner(1), Some(0));
            assert_eq!(cells.owner(2), Some(2));
            cells.configure_owner(3, 3, 2);
            assert_eq!(cells.owner(1), None);
            assert_eq!(cells.owner(2), Some(2));
            assert_eq!(cells.owner(3), Some(3));
        });
    }
}
