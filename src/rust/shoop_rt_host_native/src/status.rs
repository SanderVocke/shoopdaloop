//! Counters and driver state shared by the driver threads and the host handle.
//! Every update is a relaxed atomic store, so callbacks may make them.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::time::Instant;

use shoop_rt_engine::{HostStatus, Phase};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};

/// The render owner's phase as last reported by a callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderPhase {
    Running,
    /// The control endpoint is gone; output is silent after a MIDI reset.
    Disconnected,
    /// Rendering failed; output is silent. Stop the host.
    Faulted,
}

/// Why a stream reported an error other than an underrun.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamFailure {
    DeviceNotAvailable,
    StreamInvalidated,
    Backend,
}

/// Host counters since start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Driver callbacks, not counting the parts of split callbacks.
    pub callbacks: u64,
    /// Callbacks above the prepared maximum, rendered as several engine callbacks.
    pub split_callbacks: u64,
    /// Underruns the audio backend reported.
    pub stream_underruns: u64,
    /// Captured audio dropped because the duplex bridge was full.
    pub input_overruns: u64,
    /// Output callbacks that found too little captured audio.
    pub input_underruns: u64,
    /// Dummy driver cycles that started more than a period late.
    pub late_cycles: u64,
    pub midi_in_dropped: u64,
    /// MIDI input messages that were empty, too long or not valid MIDI.
    pub midi_in_refused: u64,
    pub midi_out_dropped: u64,
    pub midi_out_failed: u64,
    /// The last callback's processing time as a share of its period.
    pub load: f32,
}

impl Stats {
    /// Everything reported to the engine as an xrun.
    pub fn xruns(&self) -> u64 {
        self.stream_underruns + self.input_overruns + self.input_underruns + self.late_cycles
    }
}

#[derive(Default)]
pub(crate) struct Counters {
    pub callbacks: AtomicU64,
    pub split_callbacks: AtomicU64,
    pub stream_underruns: AtomicU64,
    pub input_overruns: AtomicU64,
    pub input_underruns: AtomicU64,
    pub late_cycles: AtomicU64,
    pub midi_in_dropped: AtomicU64,
    pub midi_in_refused: AtomicU64,
    pub midi_out_dropped: AtomicU64,
    pub midi_out_failed: AtomicU64,
}

pub(crate) struct Shared {
    pub host: HostStatus,
    pub counters: Counters,
    epoch: Instant,
    kind: DriverKind,
    sample_rate: u32,
    state: AtomicU8,
    buffer_frames: AtomicU32,
    failure: AtomicU8,
    phase: AtomicU8,
    next_frame: AtomicU64,
    load: AtomicU32,
    /// The host is stopping: drivers keep rendering so a MIDI reset can go out.
    pub stopping: AtomicBool,
}

fn state_code(state: DriverRunState) -> u8 {
    match state {
        DriverRunState::Starting => 0,
        DriverRunState::Running => 1,
        DriverRunState::Stopped => 2,
        DriverRunState::Failed => 3,
    }
}

impl Shared {
    pub fn new(host: HostStatus, driver: DriverStatus) -> Self {
        Self {
            host,
            counters: Counters::default(),
            epoch: Instant::now(),
            kind: driver.kind,
            sample_rate: driver.sample_rate,
            state: AtomicU8::new(state_code(driver.state)),
            buffer_frames: AtomicU32::new(driver.buffer_frames),
            failure: AtomicU8::new(0),
            phase: AtomicU8::new(0),
            next_frame: AtomicU64::new(0),
            load: AtomicU32::new(0),
            stopping: AtomicBool::new(false),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Host clock time in nanoseconds since the host started.
    pub fn nanos(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.epoch).as_nanos() as u64
    }

    pub fn driver(&self) -> DriverStatus {
        DriverStatus {
            kind: self.kind,
            state: match self.state.load(Ordering::Relaxed) {
                0 => DriverRunState::Starting,
                1 => DriverRunState::Running,
                2 => DriverRunState::Stopped,
                _ => DriverRunState::Failed,
            },
            sample_rate: self.sample_rate,
            buffer_frames: self.buffer_frames.load(Ordering::Relaxed),
        }
    }

    fn publish(&self) {
        self.host.set_driver(self.driver());
    }

    /// A failed driver stays failed.
    pub fn set_state(&self, state: DriverRunState) {
        let failed = state_code(DriverRunState::Failed);
        if self.state.load(Ordering::Relaxed) != failed || state == DriverRunState::Stopped {
            self.state.store(state_code(state), Ordering::Relaxed);
            self.publish();
        }
    }

    pub fn set_buffer_frames(&self, frames: u32) {
        if self.buffer_frames.swap(frames, Ordering::Relaxed) != frames {
            self.publish();
        }
    }

    pub fn fail(&self, failure: StreamFailure) {
        self.failure.store(
            match failure {
                StreamFailure::DeviceNotAvailable => 1,
                StreamFailure::StreamInvalidated => 2,
                StreamFailure::Backend => 3,
            },
            Ordering::Relaxed,
        );
        self.set_state(DriverRunState::Failed);
    }

    pub fn failure(&self) -> Option<StreamFailure> {
        match self.failure.load(Ordering::Relaxed) {
            1 => Some(StreamFailure::DeviceNotAvailable),
            2 => Some(StreamFailure::StreamInvalidated),
            3 => Some(StreamFailure::Backend),
            _ => None,
        }
    }

    pub fn xrun(&self, counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
        self.host.add_xruns(1);
    }

    pub fn count(counter: &AtomicU64, amount: u64) {
        counter.fetch_add(amount, Ordering::Relaxed);
    }

    pub fn set_load(&self, load: f32) {
        self.load.store(load.to_bits(), Ordering::Relaxed);
        self.host.set_load(load);
    }

    pub fn set_render(&self, phase: Phase, next_frame: u64) {
        let code = match phase {
            Phase::Running => 0,
            Phase::Disconnected => 1,
            Phase::Faulted(_) => 2,
        };
        self.phase.store(code, Ordering::Relaxed);
        self.next_frame.store(next_frame, Ordering::Relaxed);
    }

    pub fn phase(&self) -> RenderPhase {
        match self.phase.load(Ordering::Relaxed) {
            0 => RenderPhase::Running,
            1 => RenderPhase::Disconnected,
            _ => RenderPhase::Faulted,
        }
    }

    pub fn next_frame(&self) -> u64 {
        self.next_frame.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> Stats {
        let c = &self.counters;
        let get = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        Stats {
            callbacks: get(&c.callbacks),
            split_callbacks: get(&c.split_callbacks),
            stream_underruns: get(&c.stream_underruns),
            input_overruns: get(&c.input_overruns),
            input_underruns: get(&c.input_underruns),
            late_cycles: get(&c.late_cycles),
            midi_in_dropped: get(&c.midi_in_dropped),
            midi_in_refused: get(&c.midi_in_refused),
            midi_out_dropped: get(&c.midi_out_dropped),
            midi_out_failed: get(&c.midi_out_failed),
            load: f32::from_bits(self.load.load(Ordering::Relaxed)),
        }
    }
}
