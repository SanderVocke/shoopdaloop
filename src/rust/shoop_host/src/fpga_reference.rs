//! ARM-side graph binding and an independently paced persistent reference engine.
use anyhow::{ensure, Result};
use rt_fpga_compiler::{
    graph::{GraphDescription, LoopDescription},
    loading::Allocator,
    Identity, ProgramLimits,
};
use rt_fpga_reference::{
    compiler::RustTarget,
    fixed::Fixed,
    jobs::{BufferId, JobProcessor},
    pipeline::Pipeline,
    rt_interface::{program_memory, Configuration, PreparedProgram, Processor, ProgramControl},
    state::{Instruction, Program},
    storage::{MediaStore, ViewId},
    transitions::{PlanDescription, TransitionLimits, TransitionPlan},
};
use shoop_rt_protocol::{
    immediate::{
        Batch, Code, Command, CommandRecord, Header, Response, SwitchProgram, PREPARED_GRAPH_LOOPS,
    },
    Capabilities, ClientMessage, DriverKind, DriverRunState, DriverStatus, EngineMessage, Lane,
    Limits,
};
use shoop_rt_transport::{
    carrier::{CarrierError, Endpoint as CarrierEndpoint, InProcess, Outbox},
    client::ProtocolClient,
    rt,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub fn program_limits() -> ProgramLimits {
    ProgramLimits {
        instructions: 128,
        state_values: 64,
        loop_slots: 2 * PREPARED_GRAPH_LOOPS,
        jobs: 128,
        buffers: 64,
        history_slots: 64,
        input_ports: 1,
        output_ports: 2,
        quantum: 128,
    }
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
struct Pending {
    batch: Batch,
    descriptor: SwitchProgram,
    commands: Vec<Command>,
    ready: Option<bool>,
    published: bool,
}
pub struct Endpoint {
    control: rt::Endpoint,
    programs: ProgramControl,
    allocator: Allocator,
    pending: Option<Pending>,
    replies: Outbox,
    held: Option<EngineMessage>,
    quantum: u32,
}
pub struct Started {
    pub client: ProtocolClient<InProcess<Endpoint>>,
    pub driver: Driver,
}
pub fn start(quantum: u32, sample_rate: u32) -> Result<Started> {
    start_with_memory(quantum, sample_rate, 16)
}
pub fn start_with_memory(quantum: u32, sample_rate: u32, memory_mb: u32) -> Result<Started> {
    ensure!(
        matches!(quantum, 1 | 16 | 32 | 64 | 128) && sample_rate > 0,
        "invalid reference driver geometry"
    );
    let pool = program_limits();
    let (control, execution) = rt::channel(
        256,
        4,
        shoop_rt_protocol::snapshot::capacity(2, pool.loop_slots as usize, 1).unwrap(),
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
            graph: 0,
            topology: 0,
            limits,
            driver: DriverStatus {
                kind: DriverKind::Simulated,
                state: DriverRunState::Running,
                sample_rate,
                buffer_frames: quantum,
            },
        },
    )?
    .with_capabilities(Capabilities::PREPARED_GRAPH.union(Capabilities::LIVE_PROGRAMS));
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum,
            loops: vec![],
        },
        TransitionLimits {
            loop_slots: pool.loop_slots,
            events: 4 * pool.loop_slots,
        },
    )
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let program = Program {
        quantum,
        chunk_frames: 1,
        jobs_per_quantum: 3,
        sync_instructions: vec![],
        instructions: vec![
            Instruction::ProcessSilence {
                destination: BufferId(0),
            },
            Instruction::ProcessOutput {
                source: BufferId(0),
                port: 0,
            },
            Instruction::ProcessOutput {
                source: BufferId(0),
                port: 1,
            },
        ],
    };
    let pipeline = Pipeline::prepare(plan, program, vec![], pool.jobs as usize)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let (programs, program_execution) = program_memory();
    let mut processor = Processor::prepare(
        pipeline,
        execution,
        Configuration {
            graph: 0,
            topology: 0,
            frame: 0,
        },
        vec![],
    )
    .and_then(|p| p.with_program_memory(program_execution, pool))
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let chunks = (memory_mb as usize)
        .checked_mul(1024 * 1024 / 4)
        .ok_or_else(|| anyhow::anyhow!("reference media capacity overflow"))?;
    let mut media =
        MediaStore::with_block_chunk_limit(1, 128, chunks).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    media
        .create_recordings(pool.loop_slots as usize)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    media
        .reserve_chunk_capacity(chunks)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    for view in 0..pool.loop_slots {
        media
            .reserve_view_capacity(ViewId(view as usize), chunks)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    let mut jobs = JobProcessor::with_history_slots(
        quantum as usize,
        pool.buffers as usize,
        pool.history_slots as usize,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let worker = thread::Builder::new()
        .name("fpga-reference".into())
        .spawn(move || {
            let mut reclaim = 0;
            let period = Duration::from_secs_f64(f64::from(quantum) / f64::from(sample_rate));
            let mut next = Instant::now();
            let input = vec![Fixed::from_f32(0.25); quantum as usize];
            let mut left = vec![Fixed::ZERO; quantum as usize];
            let mut right = left.clone();
            while !stopping.load(Ordering::Acquire) {
                if let Err(error) = processor.process(
                    &mut jobs,
                    &mut media,
                    &[&input],
                    &mut [&mut left, &mut right],
                ) {
                    tracing::error!(?error, "reference processing failed");
                    break;
                }
                if !processor.handoff_pending()
                    && !processor.pipeline().state().owns_loop_slot(reclaim)
                {
                    if let Err(error) = rt_fpga_reference::jobs::Memory::prepare_unused_view(
                        &mut media,
                        ViewId(reclaim),
                    ) {
                        tracing::error!(?error, "reference reclamation failed");
                        break;
                    }
                }
                reclaim = (reclaim + 1) % pool.loop_slots as usize;
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
        programs,
        allocator: Allocator::new(pool),
        pending: None,
        replies: Outbox::new(8, 8, 0),
        held: None,
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
    fn prepare(&mut self, batch: Batch) -> Result<()> {
        ensure!(self.control.is_connected(), "session is not connected");
        ensure!(self.pending.is_none(), "a program is already pending");
        let revision = self.allocator.revision();
        ensure!(
            batch.graph == revision && batch.topology == revision && batch.deadline.is_none(),
            "stale graph preparation"
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
        let layout = self.allocator.stage(&logical)?;
        let staged = (|| -> Result<SwitchProgram> {
            let views = layout
                .loops
                .iter()
                .map(|l| u64::from(l.physical.slot))
                .collect::<Vec<_>>();
            let unused = layout
                .new_loops()
                .map(|l| ViewId(l.slot as usize))
                .collect();
            let mut target = RustTarget::new(&views);
            target.limits = program_limits();
            let image = PreparedProgram::new(target.build_image(&logical, layout.clone())?, unused)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            let descriptor = image.descriptor();
            self.programs
                .stage(image)
                .map_err(|_| anyhow::anyhow!("program memory handoff is full"))?;
            Ok(descriptor)
        })();
        match staged {
            Ok(descriptor) => {
                let mut commands = layout
                    .changed_sources()
                    .iter()
                    .map(|l| {
                        Command::ClearPlannedTransitions(
                            l.slot,
                            shoop_rt_protocol::immediate::ClearPlanned::Both,
                        )
                    })
                    .collect::<Vec<_>>();
                commands.push(Command::SwitchProgram(descriptor));
                self.pending = Some(Pending {
                    batch,
                    descriptor,
                    commands,
                    ready: None,
                    published: false,
                });
                Ok(())
            }
            Err(error) => {
                self.allocator.cancel(layout.revision)?;
                Err(error)
            }
        }
    }
    fn response(batch: Batch, frame: u64, code: Code, committed: Option<u64>) -> EngineMessage {
        let records = batch
            .commands()
            .iter()
            .map(|&c| CommandRecord::new(c).complete(code))
            .collect::<Vec<_>>();
        let header = committed.map_or_else(|| batch.header(), |r| Header::new(r, r, None));
        EngineMessage::ImmediateResponse(
            Response::new(header.complete(frame, code), &records).unwrap(),
        )
    }
    fn translate(&mut self, message: EngineMessage) -> EngineMessage {
        if let EngineMessage::ImmediateResponse(response) = &message {
            if response
                .records()
                .iter()
                .any(|r| matches!(r.command(), Command::SwitchProgram(_)))
                && self.pending.as_ref().is_some_and(|p| p.published)
            {
                let pending = self.pending.take().unwrap();
                let success = response.records().iter().any(|r| {
                    matches!(r.command(), Command::SwitchProgram(_)) && r.code() == Code::Applied
                });
                if success {
                    self.allocator
                        .complete(pending.descriptor.revision)
                        .expect("fenced switch completion");
                } else {
                    self.allocator
                        .refused(pending.descriptor.revision)
                        .expect("refused pending image");
                    let old = self.allocator.revision();
                    self.control.set_configuration(old, old);
                }
                return Self::response(
                    pending.batch,
                    response.header.frame(),
                    if success {
                        Code::Applied
                    } else {
                        Code::CannotApply
                    },
                    success.then_some(pending.descriptor.revision),
                );
            }
        }
        message
    }
    fn flush(&mut self, out: &mut Outbox) -> bool {
        loop {
            let Some(message) = self.held.take().or_else(|| self.replies.pop()) else {
                return true;
            };
            let message = self.translate(message);
            if out.available(message.lane()) == 0 {
                self.held = Some(message);
                return false;
            }
            out.push(message).unwrap();
        }
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
                if out.available(Lane::Reliable) == 0 {
                    return Err(CarrierError::Full);
                }
                if let Err(error) = self.prepare(batch) {
                    tracing::warn!(%error,"reference graph preparation refused");
                    out.push(Self::response(batch, 0, Code::CannotApply, None))?;
                }
                return Ok(());
            }
            if self.allocator.revision() == 0 {
                return out.push(Self::response(batch, 0, Code::CannotApply, None));
            }
        }
        self.control.try_deliver(message, out)
    }
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        let _ = self.try_deliver(message, out);
    }
    fn poll(&mut self, out: &mut Outbox) {
        while self.programs.reclaim().is_some() {}
        if !self.flush(out) {
            return;
        }
        while let Some(ready) = self.programs.ready() {
            if let Some(p) = &mut self.pending {
                if p.descriptor.revision == ready.revision {
                    p.ready = Some(ready.result.is_ok());
                }
            }
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.ready == Some(false))
        {
            let pending = self.pending.take().unwrap();
            self.allocator.cancel(pending.descriptor.revision).unwrap();
            self.held = Some(Self::response(pending.batch, 0, Code::CannotApply, None));
            if !self.flush(out) {
                return;
            }
        }
        if let Some(pending) = &mut self.pending {
            if pending.ready == Some(true) && !pending.published {
                let batch = Batch::new(
                    self.allocator.revision(),
                    self.allocator.revision(),
                    &pending.commands,
                )
                .unwrap();
                if self
                    .control
                    .try_deliver(ClientMessage::Immediate(batch), &mut self.replies)
                    .is_ok()
                {
                    self.allocator.publish(pending.descriptor.revision).unwrap();
                    pending.published = true;
                    self.control.set_configuration(
                        pending.descriptor.revision,
                        pending.descriptor.revision,
                    );
                }
            }
        }
        self.control.poll(&mut self.replies);
        self.flush(out);
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
            if let Some(EngineMessage::ImmediateResponse(r)) = client.poll() {
                return r;
            }
            assert!(Instant::now() < deadline, "activation response timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn graph(revision: u64, source: Option<u32>) -> Batch {
        Batch::new(
            revision,
            revision,
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
    }
    #[shoop_test]
    fn installation_updates_and_client_lifetime_do_not_drive_processing() {
        let Started {
            mut client,
            driver: _driver,
        } = start(16, 8000).unwrap();
        assert!(client
            .carrier_mut()
            .endpoint_mut()
            .prepare(graph(0, None))
            .is_err());
        client.connect().unwrap();
        assert!(matches!(client.poll(), Some(EngineMessage::Welcome { .. })));
        client
            .immediate(Batch::new(0, 0, &[Command::LoopGain(0, 0.5)]).unwrap())
            .unwrap();
        assert_eq!(response(&mut client).records()[0].code(), Code::CannotApply);
        client.immediate(graph(0, Some(1))).unwrap();
        assert_eq!(response(&mut client).header.code(), Code::CannotApply);
        client.immediate(graph(0, None)).unwrap();
        let first = response(&mut client);
        assert_eq!(first.header.code(), Code::Applied);
        assert_eq!(first.header.graph(), 1);
        client.immediate(graph(0, None)).unwrap();
        assert_eq!(response(&mut client).header.code(), Code::CannotApply);
        client.immediate(graph(1, None)).unwrap();
        let next = response(&mut client);
        assert_eq!(next.header.code(), Code::Applied);
        assert_eq!(next.header.graph(), 2);
        let frame = |c: &mut ProtocolClient<InProcess<Endpoint>>| {
            let mut bytes = vec![0; c.carrier_mut().endpoint().control.snapshot_capacity()];
            let (_, len) = c
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
