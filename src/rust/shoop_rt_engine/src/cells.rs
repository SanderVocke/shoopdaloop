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
    policies: [MonitoringPolicy; 2],
    pub automatic_demand: AtomicBool,
    pub automatic_monitoring: AtomicBool,
}

impl StripCells {
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
    owners: [MonitoringOwner; 2],
    publication: AtomicU64,
    epoch: AtomicU64,
    frame: AtomicU64,
    iteration: AtomicU64,
    passes: AtomicU64,
    waiting_repeat: AtomicBool,
    pub generation: AtomicU32,
    pub mode: AtomicU32,
    pub length: AtomicU64,
    pub content_revision: AtomicU64,
    pub position: AtomicU64,
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
    pub position: u64,
    pub iteration: u64,
    pub passes: u64,
    pub waiting_repeat: bool,
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
        let before = self.publication.load(Ordering::Acquire);
        if before & 1 != 0 {
            return None;
        }
        let snapshot = LoopSnapshot {
            epoch: self.epoch.load(Ordering::Relaxed),
            frame: self.frame.load(Ordering::Relaxed),
            generation: self.generation.load(Ordering::Relaxed),
            mode: mode_from_code(self.mode.load(Ordering::Relaxed)),
            length: self.length.load(Ordering::Relaxed),
            revision: self.content_revision.load(Ordering::Relaxed),
            position: self.position.load(Ordering::Relaxed),
            iteration: self.iteration.load(Ordering::Relaxed),
            passes: self.passes.load(Ordering::Relaxed),
            waiting_repeat: self.waiting_repeat.load(Ordering::Relaxed),
        };
        core::sync::atomic::fence(Ordering::Acquire);
        (self.publication.load(Ordering::Relaxed) == before).then_some(snapshot)
    }
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
            strips: (0..strips).map(|_| StripCells::default()).collect(),
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
pub(crate) struct Observer(
    pub Arc<Cells>,
    pub rt_runtime::Epoch,
    pub(crate) super::monitoring::Demand,
);
impl Observer {
    pub(crate) fn new(cells: Arc<Cells>) -> Self {
        let demand = super::monitoring::Demand::new(cells.loops.len());
        Self(cells, rt_runtime::Epoch(0), demand)
    }
}

impl BoundaryObserver for Observer {
    fn settled(&mut self, resolver: &shoop_loops::Resolver) {
        self.2.update(resolver);
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
            let publication = cells.publication.fetch_add(1, Ordering::AcqRel);
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
            cells.epoch.store(self.1 .0, Ordering::Relaxed);
            cells.frame.store(frame.0, Ordering::Relaxed);
            let cursor = state.composite.unwrap_or_default();
            cells.iteration.store(cursor.iteration, Ordering::Relaxed);
            cells.passes.store(cursor.passes, Ordering::Relaxed);
            cells
                .waiting_repeat
                .store(cursor.waiting_repeat, Ordering::Relaxed);
            cells
                .publication
                .store(publication.wrapping_add(2), Ordering::Release);
        }
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
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
