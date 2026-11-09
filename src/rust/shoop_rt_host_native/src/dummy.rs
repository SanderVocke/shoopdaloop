//! A driver without a device: a thread renders at the buffer period, or only the
//! frames a caller requests.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use shoop_rt_engine::{EngineDescription, HostPorts};
use shoop_rt_protocol::{DriverKind, Name};

use crate::cycle::Cycle;
use crate::status::Shared;
use crate::{Prepared, StartError, Started};

const IDLE: Duration = Duration::from_micros(200);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DummyMode {
    /// Render a buffer every buffer period, like a device.
    Automatic,
    /// Render only frames requested through [`DummyControl`], as fast as possible.
    Controlled,
}

#[derive(Clone, Debug)]
pub struct DummyConfig {
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub audio_inputs: u16,
    pub audio_outputs: u16,
    pub midi_inputs: u16,
    pub midi_outputs: u16,
    pub mode: DummyMode,
    /// Input samples by host port and absolute frame; silence when unset.
    pub input: Option<fn(usize, u64) -> f32>,
    /// Wraps each cycle of the driver thread, for instrumentation such as
    /// allocation checks.
    pub wrap_cycle: Option<fn(&mut dyn FnMut())>,
}

impl Default for DummyConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            buffer_frames: 256,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
            mode: DummyMode::Automatic,
            input: None,
            wrap_cycle: None,
        }
    }
}

struct Requests {
    pending: AtomicU64,
    sample_rate: u32,
    /// Time not yet requested, in nanoseconds times the sample rate.
    remainder: Mutex<u128>,
}

/// Requests frames from a dummy driver in [`DummyMode::Controlled`].
#[derive(Clone)]
pub struct DummyControl(Arc<Requests>);

impl DummyControl {
    pub fn request_frames(&self, frames: u64) {
        self.0.pending.fetch_add(frames, Ordering::AcqRel);
    }

    /// Request the whole frames that `elapsed` covers; the rest carries over.
    pub fn advance(&self, elapsed: Duration) {
        const NANOS: u128 = 1_000_000_000;
        let mut remainder = self.0.remainder.lock().unwrap_or_else(|e| e.into_inner());
        let total = *remainder + elapsed.as_nanos() * u128::from(self.0.sample_rate);
        *remainder = total % NANOS;
        self.request_frames((total / NANOS) as u64);
    }

    /// Requested frames not rendered yet.
    pub fn pending_frames(&self) -> u64 {
        self.0.pending.load(Ordering::Acquire)
    }

    /// Wait until every requested frame is rendered. Returns false on timeout.
    pub fn wait_rendered(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.pending_frames() > 0 {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(IDLE);
        }
        true
    }
}

pub(crate) struct DummyThread {
    exit: Arc<AtomicBool>,
    thread: JoinHandle<Cycle>,
}

impl DummyThread {
    pub fn stop(self) -> Option<Cycle> {
        self.exit.store(true, Ordering::Release);
        self.thread.join().ok()
    }
}

struct Loop {
    cycle: Cycle,
    shared: Arc<Shared>,
    requests: Arc<Requests>,
    exit: Arc<AtomicBool>,
    mode: DummyMode,
    buffer: usize,
    input: Option<fn(usize, u64) -> f32>,
    wrap: Option<fn(&mut dyn FnMut())>,
    late_ns: u64,
}

fn numbered(prefix: &str, count: u16) -> Vec<Name> {
    (1..=count)
        .map(|index| crate::port_name(prefix, &format!("_{index}")))
        .collect()
}

fn period(frames: usize, sample_rate: u32) -> Duration {
    Duration::from_nanos((frames as u64 * 1_000_000_000) / u64::from(sample_rate))
}

impl Loop {
    fn step(&mut self, frames: usize) {
        let input = self.input;
        let late_ns = self.late_ns;
        let Self { cycle, shared, .. } = self;
        let mut body = || {
            let started = Instant::now();
            Shared::count(&shared.counters.callbacks, 1);
            if frames > cycle.max_frames() {
                Shared::count(&shared.counters.split_callbacks, 1);
            }
            let first = cycle.next_frame();
            shared.begin_profiling(first, frames);
            for base in (0..frames).step_by(cycle.max_frames()) {
                let count = (frames - base).min(cycle.max_frames());
                for (port, samples) in cycle.inputs_mut().iter_mut().enumerate() {
                    let samples = &mut samples[..count];
                    rt_runtime::audio_samples::process(|| match input {
                        Some(input) => {
                            for (offset, sample) in samples.iter_mut().enumerate() {
                                *sample = input(port, first + base as u64 + offset as u64);
                            }
                        }
                        None => samples.fill(0.),
                    });
                }
                cycle.render(count, shared.nanos(started), true);
            }
            let busy = started.elapsed();
            shared
                .set_load(busy.as_secs_f32() / period(frames, shared.sample_rate()).as_secs_f32());
            shared.record_callback(
                first,
                frames,
                busy.as_nanos().min(u64::MAX as u128) as u64,
                late_ns,
            );
        };
        match self.wrap {
            Some(wrap) => wrap(&mut body),
            None => body(),
        }
    }

    fn run(mut self) -> Cycle {
        let period = period(self.buffer, self.shared.sample_rate());
        let mut deadline = Instant::now();
        while !self.exit.load(Ordering::Acquire) {
            self.late_ns = if self.mode == DummyMode::Automatic {
                Instant::now()
                    .saturating_duration_since(deadline)
                    .as_nanos()
                    .min(u64::MAX as u128) as u64
            } else {
                0
            };
            let pending = self.requests.pending.load(Ordering::Acquire);
            match self.mode {
                DummyMode::Automatic => self.step(self.buffer),
                DummyMode::Controlled if pending > 0 => {
                    let frames = pending.min(self.buffer as u64);
                    self.step(frames as usize);
                    self.requests.pending.fetch_sub(frames, Ordering::AcqRel);
                    continue;
                }
                DummyMode::Controlled if self.shared.stopping.load(Ordering::Acquire) => {
                    self.step(self.buffer)
                }
                DummyMode::Controlled => {
                    std::thread::sleep(IDLE);
                    deadline = Instant::now();
                    continue;
                }
            }
            deadline += period;
            let now = Instant::now();
            if now > deadline + period {
                self.shared.xrun(&self.shared.counters.late_cycles);
                deadline = now;
            } else if deadline > now {
                std::thread::sleep(deadline - now);
            }
        }
        if self.shared.stopping.load(Ordering::Acquire)
            && self.shared.phase() == crate::RenderPhase::Running
        {
            self.step(self.buffer);
        }
        self.cycle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test(no_wasm = "native driver shutdown")]
    fn stopping_joins_the_final_reset_callback_before_reporting_confirmation() {
        let template = EngineDescription {
            segments: vec![shoop_rt_engine::Segment {
                strips: 1,
                loops: 1,
            }],
            resource_pages: 16,
            page_bytes: 256,
            ..Default::default()
        };
        let Prepared {
            description,
            endpoint,
            outbox,
            render,
            shared,
            layout,
        } = Prepared::new(
            &template,
            HostPorts::numbered(0, 0, 1),
            DriverKind::Dummy,
            48000,
            Some(128),
        )
        .unwrap();
        drop(endpoint);
        drop(outbox);
        let cycle =
            Cycle::new(render, shared.clone(), &description, Vec::new(), Vec::new()).unwrap();
        let exit = Arc::new(AtomicBool::new(false));
        let worker = Loop {
            cycle,
            shared: shared.clone(),
            requests: Arc::new(Requests {
                pending: AtomicU64::new(0),
                sample_rate: 48000,
                remainder: Mutex::new(0),
            }),
            exit: exit.clone(),
            mode: DummyMode::Controlled,
            buffer: 128,
            input: None,
            wrap: None,
            late_ns: 0,
        };
        let stopped = exit.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            worker.run()
        });
        let report = crate::Running {
            configuration: crate::DriverConfig::Dummy(DummyConfig::default()),
            shared,
            driver: crate::Driver::Dummy(DummyThread { exit, thread }),
            pump: None,
            connections: Vec::new(),
            layout,
            warnings: Vec::new(),
            reset_wait: Duration::ZERO,
            dummy: None,
            instance_name: "Shutdown test".into(),
        }
        .stop();
        assert!(report.midi_reset);
        assert_eq!(report.phase, crate::RenderPhase::Disconnected);
        assert!(report.stopped.is_ok());
    }
}

pub(crate) fn start(
    config: &DummyConfig,
    template: &EngineDescription,
) -> Result<Started, StartError> {
    if config.sample_rate == 0 || config.buffer_frames == 0 {
        return Err(StartError::InvalidConfig(
            "the dummy driver needs a sample rate and a buffer size",
        ));
    }
    let ports = HostPorts {
        audio_inputs: numbered("audio_in", config.audio_inputs),
        audio_outputs: numbered("audio_out", config.audio_outputs),
        midi_inputs: numbered("midi_in", config.midi_inputs),
        midi_outputs: numbered("midi_out", config.midi_outputs),
    };
    let Prepared {
        description,
        endpoint,
        outbox,
        render,
        shared,
        layout,
    } = Prepared::new(
        template,
        ports,
        DriverKind::Dummy,
        config.sample_rate,
        Some(config.buffer_frames),
    )?;
    let cycle = Cycle::new(render, shared.clone(), &description, Vec::new(), Vec::new())
        .map_err(|error| StartError::Prepare(shoop_rt_engine::PrepareError::Runtime(error)))?;
    let requests = Arc::new(Requests {
        pending: AtomicU64::new(0),
        sample_rate: config.sample_rate,
        remainder: Mutex::new(0),
    });
    let exit = Arc::new(AtomicBool::new(false));
    let input = match std::env::var("SHOOP_AUDIO_BENCH_INPUT").as_deref() {
        Ok("signal") => Some(benchmark_input as fn(usize, u64) -> f32),
        Ok("silence") => None,
        Ok(_) => return Err(StartError::InvalidConfig("invalid SHOOP_AUDIO_BENCH_INPUT")),
        Err(_) => config.input,
    };
    let worker = Loop {
        cycle,
        shared: shared.clone(),
        requests: requests.clone(),
        exit: exit.clone(),
        mode: config.mode,
        buffer: config.buffer_frames as usize,
        input,
        wrap: config.wrap_cycle,
        late_ns: 0,
    };
    shared.set_state(shoop_rt_protocol::DriverRunState::Running);
    let thread = std::thread::Builder::new()
        .name("shoop-rt-dummy".into())
        .spawn(move || worker.run())
        .map_err(|error| StartError::Thread(error.to_string()))?;
    Ok(crate::assemble(
        endpoint,
        outbox,
        crate::Running {
            configuration: crate::DriverConfig::Dummy(config.clone()),
            instance_name: "Dummy".to_owned(),
            shared,
            driver: crate::Driver::Dummy(DummyThread { exit, thread }),
            pump: None,
            connections: Vec::new(),
            layout,
            warnings: Vec::new(),
            reset_wait: crate::reset_wait(config.buffer_frames, config.sample_rate),
            dummy: Some(DummyControl(requests)),
        },
    ))
}

fn benchmark_input(port: usize, frame: u64) -> f32 {
    let phase = (frame.wrapping_add(port as u64 * 313) % 997) as f32 / 997.;
    (phase - 0.5) * 0.4 + 0.1
}
