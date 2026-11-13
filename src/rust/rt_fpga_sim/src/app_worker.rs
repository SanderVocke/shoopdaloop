//! One owner for the Verilator context, model and application queue consumer.
use crate::{SimResult, Simulator, app, runtime};
use rt_fpga_adapter::engine::Adapter;
use shoop_rt_backend::EngineClock;
use shoop_rt_engine::{
    PrepareError,
    external::{Consumer, Prepared},
    prepare_with_consumer,
};
use shoop_rt_protocol::DriverStatus;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMode {
    Continuous,
    Stepped,
}
enum Request {
    Run(usize, mpsc::SyncSender<Result<(), String>>),
    Signal(bool),
    Stop,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct SimulationMetrics {
    pub model_build_load: Duration,
    pub preparation: Duration,
    pub execution: Duration,
    pub cycles: u64,
}
#[derive(Default)]
struct Status {
    frame: AtomicU64,
    metrics: Mutex<rt_fpga_adapter::engine::Metrics>,
    store: Mutex<crate::StoreUsage>,
    simulation: Mutex<SimulationMetrics>,
}
#[derive(Clone)]
pub struct Handle {
    send: mpsc::Sender<Request>,
    status: Arc<Status>,
}
impl Handle {
    pub fn run(&self, frames: usize) -> SimResult<()> {
        let (send, receive) = mpsc::sync_channel(1);
        self.send.send(Request::Run(frames, send))?;
        receive.recv()?.map_err(Into::into)
    }
    pub fn frame(&self) -> u64 {
        self.status.frame.load(Ordering::Relaxed)
    }
    pub fn store_usage(&self) -> crate::StoreUsage {
        *self.status.store.lock().unwrap()
    }
    pub fn simulation_metrics(&self) -> SimulationMetrics {
        *self.status.simulation.lock().unwrap()
    }
    pub fn metrics(&self) -> rt_fpga_adapter::engine::Metrics {
        *self.status.metrics.lock().unwrap()
    }
    pub fn signal(&self, enabled: bool) -> SimResult<()> {
        self.send.send(Request::Signal(enabled)).map_err(Into::into)
    }
}
pub struct Worker {
    send: mpsc::Sender<Request>,
    join: Option<thread::JoinHandle<()>>,
    mode: RunMode,
    status: Arc<Status>,
    remainder: u128,
    pub output: Arc<Mutex<Vec<[f32; 2]>>>,
    pub failure: Arc<Mutex<Option<String>>>,
}
impl Worker {
    pub fn prepare(artifacts: PathBuf, mode: RunMode) -> SimResult<Prepared<Self>> {
        Self::prepare_with_trace(artifacts, mode, None)
    }
    pub fn prepare_with_trace(
        artifacts: PathBuf,
        mode: RunMode,
        wave: Option<PathBuf>,
    ) -> SimResult<Prepared<Self>> {
        let (send, receive) = mpsc::channel();
        let (ready_send, ready_receive) =
            mpsc::sync_channel::<Result<shoop_rt_engine::EngineControl, PrepareError>>(1);
        let output = Arc::new(Mutex::new(Vec::new()));
        let failure = Arc::new(Mutex::new(None));
        let status = Arc::new(Status::default());
        let worker_status = status.clone();
        let worker_output = output.clone();
        let worker_failure = failure.clone();
        let diagnostic = failure.clone();
        let prepared = prepare_with_consumer(
            &app::description(),
            // This worker clocks the RTL model unpaced; no device driver exists.
            DriverStatus {
                kind: shoop_rt_protocol::DriverKind::Simulated,
                state: shoop_rt_protocol::DriverRunState::Running,
                sample_rate: 48_000,
                buffer_frames: 512,
            },
            move |parts| {
                let join = thread::Builder::new()
                    .name("fpga-marlin-app".into())
                    .spawn(move || {
                        let result = (|| -> SimResult<()> {
                            let model_started = Instant::now();
                            let runtime = runtime(&artifacts)?;
                            let mut sim = Simulator::new(&runtime, wave.as_deref(), false)?;
                            sim.arena_capacity(32768)?;
                            worker_status.simulation.lock().unwrap().model_build_load =
                                model_started.elapsed();
                            let prepare_started = Instant::now();
                            let Consumer {
                                control,
                                mut execution,
                                ..
                            } = Adapter::prepare(parts, sim)?;
                            worker_status.simulation.lock().unwrap().preparation =
                                prepare_started.elapsed();
                            ready_send
                                .send(Ok(control))
                                .map_err(|_| "startup receiver closed")?;
                            let mut signal = false;
                            loop {
                                let request = if mode == RunMode::Continuous {
                                    match receive.try_recv() {
                                        Ok(request) => Some(request),
                                        Err(mpsc::TryRecvError::Empty) => None,
                                        Err(mpsc::TryRecvError::Disconnected) => break,
                                    }
                                } else {
                                    match receive.recv() {
                                        Ok(request) => Some(request),
                                        Err(_) => break,
                                    }
                                };
                                match request {
                                    Some(Request::Stop) => break,
                                    Some(Request::Signal(enabled)) => {
                                        signal = enabled;
                                        continue;
                                    }
                                    Some(Request::Run(frames, reply)) => {
                                        let result = run(
                                            &mut execution,
                                            frames,
                                            signal,
                                            &worker_output,
                                            true,
                                            &worker_status,
                                        )
                                        .map_err(|e| e.to_string());
                                        let failure = result.as_ref().err().cloned();
                                        let _ = reply.send(result);
                                        if let Some(error) = failure {
                                            return Err(error.into());
                                        }
                                    }
                                    None => run(
                                        &mut execution,
                                        32,
                                        signal,
                                        &worker_output,
                                        false,
                                        &worker_status,
                                    )?,
                                }
                            }
                            Ok(())
                        })();
                        if let Err(error) = result {
                            worker_failure
                                .lock()
                                .unwrap()
                                .get_or_insert_with(|| error.to_string());
                        }
                    })
                    .map_err(|_| PrepareError::Runtime(rt_runtime::Error::Disconnected))?;
                let control = ready_receive
                    .recv()
                    .map_err(|_| PrepareError::Runtime(rt_runtime::Error::Disconnected))??;
                Ok(Consumer {
                    control,
                    execution: Worker {
                        send,
                        join: Some(join),
                        mode,
                        status,
                        remainder: 0,
                        output,
                        failure,
                    },
                    latency_frames: 32,
                    capabilities: shoop_rt_protocol::Capabilities(0),
                })
            },
        )
        .map_err(|e| {
            format!(
                "hardware preparation: {e:?}; worker: {:?}",
                diagnostic.lock().unwrap()
            )
        })?;
        Ok(prepared)
    }
    pub fn handle(&self) -> Handle {
        Handle {
            send: self.send.clone(),
            status: self.status.clone(),
        }
    }
    pub fn signal(&self, enabled: bool) -> SimResult<()> {
        self.send.send(Request::Signal(enabled)).map_err(Into::into)
    }
    pub fn run(&self, frames: usize) -> SimResult<()> {
        let (send, receive) = mpsc::sync_channel(1);
        self.send.send(Request::Run(frames, send))?;
        receive.recv()?.map_err(Into::into)
    }
}
fn run(
    execution: &mut Adapter<Simulator<'_>>,
    mut frames: usize,
    signal: bool,
    output: &Mutex<Vec<[f32; 2]>>,
    capture: bool,
    status: &Status,
) -> SimResult<()> {
    while frames > 0 {
        let count = frames.min(512);
        let start = execution.frame().0;
        let inputs: Vec<Vec<f32>> = (0..2)
            .map(|ch| {
                (0..count)
                    .map(|offset| {
                        if signal {
                            app::input(ch, start + offset as u64)
                        } else {
                            0.
                        }
                    })
                    .collect()
            })
            .collect();
        let mut left = vec![0.; count];
        let mut right = vec![0.; count];
        let started = Instant::now();
        execution
            .process(&[&inputs[0], &inputs[1]], &mut [&mut left, &mut right])
            .map_err(|error| format!("{error}; device: {:?}", execution.device_mut().last_error))?;
        {
            let mut metrics = status.simulation.lock().unwrap();
            metrics.execution += started.elapsed();
            metrics.cycles = execution.device_mut().cycles;
        }
        if capture {
            output
                .lock()
                .unwrap()
                .extend(left.into_iter().zip(right).map(|(l, r)| [l, r]));
        }
        status.frame.store(execution.frame().0, Ordering::Relaxed);
        *status.metrics.lock().unwrap() = *execution.metrics();
        frames -= count;
    }
    *status.store.lock().unwrap() = execution.device_mut().store_usage()?;
    Ok(())
}
impl EngineClock for Worker {
    fn advance(&mut self, elapsed: Duration) {
        if self.mode == RunMode::Stepped {
            self.remainder += elapsed.as_nanos() * 48_000;
            let frames = (self.remainder / 1_000_000_000) as usize;
            self.remainder %= 1_000_000_000;
            if let Err(error) = self.run(frames) {
                self.failure
                    .lock()
                    .unwrap()
                    .get_or_insert_with(|| error.to_string());
            }
        }
    }
    fn wait(&mut self) -> bool {
        if self.mode == RunMode::Stepped {
            self.run(32).is_ok()
        } else {
            thread::yield_now();
            self.failure.lock().unwrap().is_none()
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.send.send(Request::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
