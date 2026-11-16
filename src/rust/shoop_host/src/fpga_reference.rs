//! Temporary single-install reference host, with preparation outside the processing thread.
use anyhow::{ensure, Result};
use rt_fpga_compiler::{
    graph::{GraphDescription, LoopDescription},
    Identity, StateProgramTarget,
};
use rt_fpga_reference::{
    compiler::RustTarget,
    fixed::Fixed,
    jobs::JobProcessor,
    pipeline::Pipeline,
    rt_interface::{Configuration, LoopBinding, Processor},
    storage::MediaStore,
};
use shoop_rt_protocol::{
    immediate::{Batch, Code, Command, CommandRecord, Response, PREPARED_GRAPH_LOOPS},
    Capabilities, ClientMessage, DriverKind, DriverRunState, DriverStatus, EngineMessage, Limits,
};
use shoop_rt_transport::{
    carrier::{CarrierError, Endpoint as CarrierEndpoint, InProcess, Outbox},
    client::ProtocolClient,
    rt,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Prepared {
    processor: Processor,
    jobs: JobProcessor,
    media: MediaStore,
}
pub struct Driver {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
pub struct Endpoint {
    control: rt::Endpoint,
    execution: Option<rt::Execution>,
    install: mpsc::SyncSender<Prepared>,
    activated: mpsc::Receiver<bool>,
    pending: Option<Batch>,
    installed: bool,
    quantum: u32,
}
pub struct Started {
    pub client: ProtocolClient<InProcess<Endpoint>>,
    pub driver: Driver,
}

pub fn start(quantum: u32, sample_rate: u32) -> Result<Started> {
    ensure!(
        matches!(quantum, 1 | 16 | 32 | 64 | 128) && sample_rate > 0,
        "invalid reference driver geometry"
    );
    let (control, execution) = rt::channel(
        256,
        4,
        shoop_rt_protocol::snapshot::capacity(2, PREPARED_GRAPH_LOOPS as usize, 1).unwrap(),
    )?;
    let limits = Limits {
        composite_entries: 0,
        composite_segments: 0,
        timing_relations: 0,
        content_chunk_bytes: 0,
        strips: 2,
        loops: PREPARED_GRAPH_LOOPS,
        strip_audio_channels: 1,
        master_channels: 2,
        buses: 1,
        bus_channels: 2,
        host_audio_inputs: 1,
        host_audio_outputs: 2,
        host_midi_inputs: 0,
        host_midi_outputs: 0,
        max_frames: quantum,
        quantum,
        processing_latency_frames: 0,
        smoothing_limit_frames: 0,
        reliable_credits: 0,
        bulk_credits: 0,
    };
    let control = rt::Endpoint::new(
        control,
        rt::Settings {
            graph: 1,
            topology: 1,
            limits,
            driver: DriverStatus {
                kind: DriverKind::Simulated,
                state: DriverRunState::Running,
                sample_rate,
                buffer_frames: quantum,
            },
        },
    )?
    .with_capabilities(Capabilities::PREPARED_GRAPH);
    let (tx, rx) = mpsc::sync_channel::<Prepared>(1);
    let (ack, activated) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let worker = thread::Builder::new()
        .name("fpga-reference".into())
        .spawn(move || {
            let period = Duration::from_secs_f64(f64::from(quantum) / f64::from(sample_rate));
            let mut next = Instant::now();
            let mut prepared: Option<Prepared> = None;
            let input = vec![Fixed::from_f32(0.25); quantum as usize];
            let mut left = vec![Fixed::ZERO; quantum as usize];
            let mut right = left.clone();
            while !stopping.load(Ordering::Acquire) {
                let fresh = if prepared.is_none() {
                    match rx.try_recv() {
                        Ok(p) => {
                            prepared = Some(p);
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                if let Some(p) = &mut prepared {
                    let result = p.processor.process(
                        &mut p.jobs,
                        &mut p.media,
                        &[&input],
                        &mut [&mut left, &mut right],
                    );
                    if fresh {
                        let _ = ack.send(result.is_ok());
                    }
                    if let Err(error) = &result {
                        tracing::error!(?error, "reference processing failed");
                        break;
                    }
                }
                next += period;
                if let Some(wait) = next.checked_duration_since(Instant::now()) {
                    thread::sleep(wait);
                } else {
                    next = Instant::now();
                }
            }
        })?;
    let endpoint = Endpoint {
        control,
        execution: Some(execution),
        install: tx,
        activated,
        pending: None,
        installed: false,
        quantum,
    };
    Ok(Started {
        client: ProtocolClient::new(InProcess::new(endpoint, Outbox::new(64, 8, 0))),
        driver: Driver {
            stop,
            thread: Some(worker),
        },
    })
}
impl Endpoint {
    fn prepare(&mut self, batch: Batch) -> Result<Prepared> {
        ensure!(self.control.is_connected(), "session is not connected");
        ensure!(
            !self.installed && self.pending.is_none(),
            "graph already installed"
        );
        ensure!(
            batch.graph == 0 && batch.topology == 0 && batch.deadline.is_none(),
            "initial graph requires configuration zero"
        );
        let commands = batch.commands();
        let Some(Command::BeginGraph { quantum, loops }) = commands.first().copied() else {
            anyhow::bail!("missing graph header");
        };
        ensure!(
            quantum == self.quantum
                && loops > 0
                && loops <= PREPARED_GRAPH_LOOPS
                && commands.len() == loops as usize + 2,
            "invalid graph size"
        );
        ensure!(
            commands.last() == Some(&Command::ActivateGraph),
            "missing activation"
        );
        let mut description = GraphDescription {
            quantum,
            loops: Vec::new(),
        };
        for command in &commands[1..commands.len() - 1] {
            let Command::ConfigureLoop(l) = *command else {
                anyhow::bail!("invalid graph record");
            };
            ensure!(command.valid(), "invalid loop description");
            description.loops.push(LoopDescription {
                id: Identity {
                    slot: l.slot,
                    generation: l.generation,
                },
                source: l.source,
                input: l.input.is_some(),
                output: l.output,
                gain: l.gain,
                repeat_sync: l.repeat_sync,
            });
        }
        let logical = description.compile()?;
        let mut media = MediaStore::new(1).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let views = (0..loops)
            .map(|_| media.create_recording().map(|v| v.0 as u64))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let compiled = RustTarget::new(&views).build(&logical)?;
        let bindings = compiled
            .bindings
            .iter()
            .map(|b| LoopBinding {
                gain: b.gain,
                meter: b.meter,
                initial_gain: description
                    .loops
                    .iter()
                    .find(|l| l.id.slot == b.id.slot)
                    .unwrap()
                    .gain,
                ..LoopBinding::new(b.id)
            })
            .collect();
        let resources = compiled.resources;
        let pipeline = Pipeline::prepare(
            compiled.plan,
            compiled.program,
            compiled.memory,
            resources.queue_capacity as usize,
        )
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let processor = Processor::prepare(
            pipeline,
            self.execution
                .take()
                .ok_or_else(|| anyhow::anyhow!("graph already prepared"))?,
            Configuration {
                graph: 1,
                topology: 1,
                frame: 0,
            },
            bindings,
        )
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        Ok(Prepared {
            processor,
            media,
            jobs: JobProcessor::with_history_slots(
                quantum as usize,
                resources.buffers as usize,
                resources.history_slots as usize,
            ),
        })
    }
    fn response(batch: Batch, code: Code) -> EngineMessage {
        let records = batch
            .commands()
            .iter()
            .map(|&c| CommandRecord::new(c).complete(code))
            .collect::<Vec<_>>();
        EngineMessage::ImmediateResponse(
            Response::new(batch.header().complete(0, code), &records).unwrap(),
        )
    }
}
impl CarrierEndpoint for Endpoint {
    fn try_deliver(
        &mut self,
        message: ClientMessage,
        out: &mut Outbox,
    ) -> std::result::Result<(), CarrierError> {
        if let ClientMessage::Immediate(batch) = message {
            if batch.commands().iter().any(|c| {
                matches!(
                    c,
                    Command::BeginGraph { .. } | Command::ConfigureLoop(_) | Command::ActivateGraph
                )
            }) {
                if out.available(shoop_rt_protocol::Lane::Reliable) == 0 {
                    return Err(CarrierError::Full);
                }
                match self.prepare(batch) {
                    Ok(prepared) => {
                        self.install
                            .try_send(prepared)
                            .map_err(|_| CarrierError::Full)?;
                        self.pending = Some(batch);
                        self.installed = true;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "reference graph preparation refused");
                        out.push(Self::response(batch, Code::CannotApply))?;
                    }
                }
                return Ok(());
            }
            if !self.installed || self.pending.is_some() {
                return out.push(Self::response(batch, Code::CannotApply));
            }
        }
        self.control.try_deliver(message, out)
    }
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        let _ = self.try_deliver(message, out);
    }
    fn poll(&mut self, out: &mut Outbox) {
        if self.pending.is_some() && out.available(shoop_rt_protocol::Lane::Reliable) > 0 {
            if let Ok(success) = self.activated.try_recv() {
                let batch = self.pending.take().unwrap();
                out.push(Self::response(
                    batch,
                    if success {
                        Code::Applied
                    } else {
                        Code::CannotApply
                    },
                ))
                .unwrap();
            }
        }
        self.control.poll(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_rt_protocol::{immediate::GraphLoop, snapshot::Header};
    use shoop_wasm_test_support::shoop_test;

    fn response(client: &mut ProtocolClient<InProcess<Endpoint>>) -> Response {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(EngineMessage::ImmediateResponse(response)) = client.poll() {
                return response;
            }
            assert!(Instant::now() < deadline, "activation response timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[shoop_test]
    fn installation_refuses_invalid_and_repeated_graphs_without_control_pacing() {
        let Started {
            mut client,
            driver: _driver,
        } = start(16, 8000).unwrap();
        let graph = |source| {
            Batch::new(
                0,
                0,
                &[
                    Command::BeginGraph {
                        quantum: 16,
                        loops: 1,
                    },
                    Command::ConfigureLoop(GraphLoop {
                        slot: 0,
                        generation: 3,
                        source,
                        input: Some(0),
                        gain: 1.0,
                        output: true,
                        repeat_sync: false,
                    }),
                    Command::ActivateGraph,
                ],
            )
            .unwrap()
        };
        assert!(client
            .carrier_mut()
            .endpoint_mut()
            .prepare(graph(None))
            .is_err());
        client.connect().unwrap();
        assert!(matches!(client.poll(), Some(EngineMessage::Welcome { .. })));
        client
            .immediate(Batch::new(1, 1, &[Command::LoopGain(0, 0.5)]).unwrap())
            .unwrap();
        assert_eq!(response(&mut client).records()[0].code(), Code::CannotApply);
        client.immediate(graph(Some(1))).unwrap();
        assert_eq!(response(&mut client).header.code(), Code::CannotApply);
        client.immediate(graph(None)).unwrap();
        assert_eq!(response(&mut client).header.code(), Code::Applied);
        client.immediate(graph(None)).unwrap();
        assert_eq!(response(&mut client).header.code(), Code::CannotApply);
        let frame = |client: &mut ProtocolClient<InProcess<Endpoint>>| {
            let mut bytes = vec![0; client.carrier_mut().endpoint().control.snapshot_capacity()];
            let (_, len) = client
                .carrier_mut()
                .endpoint()
                .control
                .read_snapshot(&mut bytes)
                .unwrap();
            Header::read(&bytes[..len]).unwrap().frame
        };
        let before = frame(&mut client);
        client
            .carrier_mut()
            .endpoint_mut()
            .try_deliver(ClientMessage::Goodbye, &mut Outbox::new(1, 0, 0))
            .unwrap();
        thread::sleep(Duration::from_millis(20));
        assert!(frame(&mut client) > before);
    }
}
