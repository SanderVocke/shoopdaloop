//! In-process Carla ownership off the audio callback.

use crate::{
    buffered::{RenderEndpoint, Worker},
    native::{CarlaMainThreadUiDispatcher, CarlaNativeHost, CarlaNativeUiHandle},
};
use anyhow::{anyhow, bail, Context, Result};
use rt_buffered::{BufferLimits, PreparedTiming};
use shoop_plugin_protocol::{CarlaChainType, MAX_STATE_BYTES};
use std::{
    sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const COMMAND_SLOTS: usize = 8;
const START_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const IDLE_POLL: Duration = Duration::from_micros(200);

enum Operation {
    Active(bool),
    Save,
    Restore(String),
}

enum Reply {
    Done,
    State(String),
}

struct Command {
    operation: Operation,
    reply: SyncSender<Result<Reply>>,
    deadline: Instant,
}

/// The host keeps this handle for the lifetime of the installed render endpoint.
pub struct InProcess {
    commands: SyncSender<Command>,
    thread: JoinHandle<()>,
    editor: Option<CarlaNativeUiHandle>,
}

impl InProcess {
    pub fn start(
        chain: CarlaChainType,
        sample_rate: u32,
        timing: PreparedTiming,
        limits: BufferLimits,
        ui: Option<CarlaMainThreadUiDispatcher>,
    ) -> Result<(RenderEndpoint, Self)> {
        Worker::transport_bytes(chain, timing, limits)
            .map_err(|error| anyhow!("could not prepare Carla worker storage: {error:?}"))?;
        let frames = timing.batch_frames() as u32;
        let (commands, receiver) = sync_channel(COMMAND_SLOTS);
        let (ready, startup) = sync_channel(1);
        let thread = thread::Builder::new()
            .name("carla-native-dsp".into())
            .spawn(move || {
                let prepared: Result<_> = (|| {
                    let processor = match ui {
                        Some(ui) => CarlaNativeHost::instantiate_with_ui_dispatcher(
                            chain,
                            sample_rate,
                            frames,
                            ui,
                        ),
                        None => CarlaNativeHost::instantiate(chain, sample_rate, frames),
                    }?;
                    let editor = processor.external_ui();
                    let (endpoint, worker) = Worker::prepare(processor, timing, limits)
                        .map_err(|error| anyhow!("could not prepare Carla worker: {error:?}"))?;
                    Ok((endpoint, worker, editor))
                })();
                match prepared {
                    Ok((endpoint, worker, editor)) => {
                        if ready.send(Ok((endpoint, editor))).is_ok() {
                            run(worker, receiver);
                        }
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })
            .context("spawning Carla DSP worker")?;
        let (endpoint, editor) = startup
            .recv_timeout(START_TIMEOUT)
            .context("waiting for Carla DSP startup")??;
        Ok((
            endpoint,
            Self {
                commands,
                thread,
                editor,
            },
        ))
    }

    fn request(&self, operation: Operation) -> Result<Reply> {
        let (reply, response) = sync_channel(1);
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        self.commands
            .try_send(Command {
                operation,
                reply,
                deadline,
            })
            .map_err(|error| anyhow!("Carla control submission failed: {error}"))?;
        self.thread.thread().unpark();
        response
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .context("waiting for Carla control")?
    }

    pub fn set_active(&self, active: bool) -> Result<()> {
        self.request(Operation::Active(active)).map(|_| ())
    }

    pub fn save_state(&self) -> Result<String> {
        match self.request(Operation::Save)? {
            Reply::State(state) => Ok(state),
            Reply::Done => bail!("Carla state response was incomplete"),
        }
    }

    pub fn restore_state(&self, state: String) -> Result<()> {
        if state.len() > MAX_STATE_BYTES {
            bail!("Carla state exceeds the worker transfer limit");
        }
        self.request(Operation::Restore(state)).map(|_| ())
    }

    pub fn editor(&self) -> Option<&CarlaNativeUiHandle> {
        self.editor.as_ref()
    }

    pub fn stop(self, timeout: Duration) -> Result<()> {
        drop(self.commands);
        self.thread.thread().unpark();
        let start = Instant::now();
        while !self.thread.is_finished() {
            if start.elapsed() >= timeout {
                bail!("Carla DSP worker did not stop before the deadline");
            }
            thread::sleep(IDLE_POLL);
        }
        self.thread
            .join()
            .map_err(|_| anyhow!("Carla DSP worker panicked"))
    }
}

fn run(mut worker: Worker, commands: Receiver<Command>) {
    loop {
        match commands.try_recv() {
            Ok(command) => {
                let reply = if Instant::now() >= command.deadline {
                    Err(anyhow!("Carla control expired before execution"))
                } else {
                    match command.operation {
                        Operation::Active(active) => {
                            worker.set_active(active);
                            Ok(Reply::Done)
                        }
                        Operation::Save => worker.processor().save_state().and_then(|state| {
                            if state.len() > MAX_STATE_BYTES {
                                bail!("Carla state exceeds the worker transfer limit");
                            }
                            Ok(Reply::State(state))
                        }),
                        Operation::Restore(state) => worker
                            .processor()
                            .restore_state(&state)
                            .map(|()| Reply::Done),
                    }
                };
                let _ = command.reply.send(reply);
            }
            Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        match worker.process_next() {
            Ok(true) => {}
            Ok(false) => thread::park_timeout(IDLE_POLL),
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_buffered::{AsyncBatch, Backend, BatchTag, MidiLimits, Timing};
    use rt_runtime::{buffer::MidiBuffer, Frame, Quantum};
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn in_process_service_owns_native_dsp_state_and_teardown_off_render() {
        let quantum = Quantum::new(16).unwrap();
        let timing = Timing::default().prepare(quantum).unwrap();
        let midi = MidiLimits {
            events_per_quantum: 16,
            bytes_per_quantum: 64,
        };
        let limits = BufferLimits {
            midi_input: midi,
            midi_output: midi,
            parameters_per_quantum: 0,
        };
        let (mut endpoint, service) =
            match InProcess::start(CarlaChainType::Rack, 48_000, timing, limits, None) {
                Ok(service) => service,
                Err(error)
                    if std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none()
                        && crate::native::carla_runtime_availability().is_err() =>
                {
                    eprintln!("skipping in-process Carla service test: {error:#}");
                    return;
                }
                Err(error) => panic!("Carla service could not start: {error:#}"),
            };
        let state = service.save_state().unwrap();
        service.restore_state(state).unwrap();
        let transport = endpoint.async_transport().unwrap();
        let input = [0.25; 64];
        let midi_input = [MidiBuffer::new(4, 16).unwrap()];
        let batch = |sequence| AsyncBatch {
            tag: BatchTag {
                generation: 1,
                sequence,
                source: Frame(sequence * timing.batch_frames() as u64),
            },
            frames: timing.batch_frames(),
            audio: &input,
            midi: &midi_input,
            parameters: &[],
        };
        for (sequence, active) in [false, true].into_iter().enumerate() {
            service.set_active(active).unwrap();
            let submission = batch(sequence as u64);
            let tag = submission.tag;
            assert_no_heap(|| transport.try_submit(submission)).unwrap();
            let start = Instant::now();
            loop {
                if let Some(completion) = transport.try_completion() {
                    assert_eq!(completion.tag, tag);
                    if active {
                        assert_eq!(completion.output.unwrap().audio, input);
                    } else {
                        assert_eq!(
                            completion.output.err(),
                            Some(rt_runtime::Error::InvalidState)
                        );
                    }
                    break;
                }
                assert!(
                    start.elapsed() < CONTROL_TIMEOUT,
                    "Carla completion timed out"
                );
                thread::sleep(IDLE_POLL);
            }
        }
        service.stop(CONTROL_TIMEOUT).unwrap();
        assert_eq!(
            transport.try_submit(batch(2)),
            Err(rt_runtime::Error::Disconnected)
        );
    }
}
