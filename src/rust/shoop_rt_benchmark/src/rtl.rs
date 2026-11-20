use crate::{
    fixture::*,
    reference,
    runner::{self, Control, Driver, Measurement, Run},
};
use anyhow::{anyhow, ensure, Result};
use rt_fpga_compiler::StateProgramTarget;
use rt_fpga_rtl::{
    bits::Bits,
    model::Probe,
    sim::{Agent, Sim},
    target::{CompiledProgram, RtlTarget},
};
use rt_fpga_testbench::{
    drivers::{pipeline::Pipeline, store::Geometry},
    loop_image,
    memory::{axi::Axi, Backend, Control as MemoryControl, Memory, Mode, Shared},
    timing::{dram::Dram, Fixed as FixedTiming, Timing},
};
use shoop_rt_protocol as protocol;
use shoop_rt_protocol::{
    codec,
    immediate::{Code, Command, CommandRecord, TransitionMode},
    snapshot, DriverKind, DriverRunState, DriverStatus, LoopMode, LoopSlotId, Peaks, SlotId,
    Telemetry,
};
use shoop_rt_transport::{carrier::Outbox, rt};
use std::{cell::RefCell, path::Path, rc::Rc, time::Instant};

const CAPACITIES: [u64; 7] = [2048, 512, 2048, 32, 512, 2, 2];
const MEMORY_SEED: u64 = 20261117;
const STATE_RAM: &str = "core.audio.core.pipeline.state_engine.loop_memory.memory";
const OUTPUT_RAM: &str = "core.audio.core.pipeline.sample_engine.local_io.outputs_memory.memory";

#[derive(Default, Clone, Copy)]
struct Activity {
    state: u64,
    jobs: u64,
    queue_peak: u64,
}
impl Agent for Activity {
    fn sample(&mut self, model: &rt_fpga_rtl::model::Model<'_>, edges: rt_fpga_rtl::clock::Edges) {
        if edges.rises(0) {
            self.state += model.get("state_busy");
            self.jobs += model.get("jobs_busy");
            self.queue_peak = self.queue_peak.max(model.get("queued_jobs"));
        }
    }
}

fn external<T>(result: rt_fpga_testbench::Result<T>) -> Result<T> {
    result.map_err(|e| anyhow!("{e}"))
}

pub fn run(
    fixture: Fixture,
    memory_model: &str,
    dram_config: Option<&Path>,
    output: &Path,
) -> Result<Run> {
    let media = fixture.media;
    let mode = match memory_model {
        "dramsim3" => Mode::Dramsim3,
        "fixed" => Mode::Fixed,
        _ => anyhow::bail!("unsupported memory model"),
    };
    let geometry = Geometry::new(
        [
            u64::from(LOOPS * CHANNELS),
            (MEMORY_BYTES / (FPGA_CHUNK_FRAMES * 4)) as u64,
            FPGA_CHUNK_FRAMES as u64,
        ],
        FPGA_BLOCK_CHUNKS as u64,
    );
    let channels = media.audio_channels() as u32;
    let logical = reference::graph(media)?;
    let views: Vec<_> = (0..u64::from(LOOPS * channels)).collect();
    let compiled = RtlTarget::new(CAPACITIES, LOOPS, &views)
        .build(&logical)
        .map_err(|e| anyhow!("RTL lowering: {e:?}"))?;
    let directory = output.with_extension("rtl");
    external(rt_fpga_rtl::report::configure(&directory, false))?;
    let mut design = rt_fpga_rtl::pipeline::design(
        &geometry,
        rt_fpga_compiler::ProgramLimits {
            instructions: CAPACITIES[0] as u32,
            state_values: CAPACITIES[1] as u32,
            jobs: CAPACITIES[2] as u32,
            quantum: CAPACITIES[3] as u32,
            buffers: CAPACITIES[4] as u32,
            input_ports: CAPACITIES[5] as u32,
            output_ports: CAPACITIES[6] as u32,
            loop_slots: LOOPS,
            history_slots: 128,
        },
    );
    design.memory = None;
    design.parameters.insert("LIVE_LOADING".into(), "0".into());
    design
        .parameters
        .insert("PROGRAM_REGIONS".into(), "1".into());
    design.top = "midi_audio_pipeline_axi".into();
    design.probes.clear();
    design
        .includes
        .push(rt_fpga_rtl::root().join("fpga/midi/rtl"));
    for name in [
        "midi_audio_pipeline_axi",
        "midi_audio_pipeline",
        "midi_data_jobs",
        "midi_store",
    ] {
        design
            .sources
            .push(rt_fpga_rtl::root().join(format!("fpga/midi/rtl/{name}.sv")));
    }
    let midi_limits = reference::midi_limits();
    for (name, value) in [
        ("MIDI_LOOPS", LOOPS as u64),
        ("MIDI_INPUTS", 1),
        ("MIDI_DESTINATIONS", 1),
        ("MIDI_BUFFERS", 128),
        ("MIDI_CURSORS", LOOPS as u64),
        ("MIDI_SCRATCH_EVENTS", MIDI_SCRATCH_EVENTS as u64),
        ("MIDI_BLOCK_EVENTS", MIDI_BLOCK_EVENTS as u64),
        ("MIDI_BLOCKS", midi_limits.blocks as u64),
        ("MIDI_MAX_EVENTS", midi_limits.max_events as u64),
        ("MIDI_VIEWS", midi_limits.views as u64),
        ("MIDI_RETIREMENTS", midi_limits.retirements as u64),
        ("MIDI_MAINTENANCE", 64),
        ("MIDI_AXI_DATA_WIDTH", 64),
        ("MIDI_PHYSICAL_BASE", 0x03000000),
        ("READ_HINT_DEPTH", 256),
    ] {
        design.parameters.insert(name.into(), value.to_string());
    }
    design
        .probes
        .push(Probe::observe("core.midi.content.free_count"));
    design.probes.push(Probe::array(STATE_RAM));
    design.probes.push(Probe::array(OUTPUT_RAM));
    for name in ["slot_count", "free_count"] {
        design
            .probes
            .push(Probe::observe(&format!("core.audio.core.media.{name}")));
    }
    let model = external(design.build(&rt_fpga_rtl::root().join("work/rt-benchmark-model")))?;
    let mut sim = Sim::new(external(model.benchmark_session())?, &[("clk", 6666, 0)]);
    sim.max_ps = u64::MAX;
    let activity = Rc::new(RefCell::new(Activity::default()));
    sim.agent(activity.clone());
    let mut pipeline = external(Pipeline::new(
        &mut sim,
        CAPACITIES,
        geometry,
        Mode::Fixed,
        MEMORY_SEED,
    ))?;
    let timing: Box<dyn Timing> = if mode == Mode::Dramsim3 {
        let root = rt_fpga_rtl::root();
        let config = dram_config
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("SHOOP_DRAMSIM3_CONFIG").map(std::path::PathBuf::from))
            .unwrap_or_else(|| root.join("fpga/axi_memory/configs/zturn-v2-ddr3-1066-x32.ini"));
        Box::new(external(Dram::new(
            &root.join("work/dramsim3/build/libshoop_dram.so"),
            &config,
            &directory.join("shared-dram-150mhz"),
            FPGA_CLOCK_HZ,
        ))?)
    } else {
        Box::new(FixedTiming::new(7))
    };
    let controller = Rc::new(RefCell::new(timing));
    if let Backend::Axi(axi) = &mut pipeline.store.memory.borrow_mut().backend {
        axi.timing = Box::new(crate::rtl_memory::Port {
            controller: controller.clone(),
            advances_clock: true,
        });
    }
    let event_address = (sim.model.get("midi_arena_bytes") as usize + 15) & !15;
    let midi_words = (event_address + MIDI_INPUT_CAPACITY * 16 + 256) / 4;
    let midi_memory = Rc::new(RefCell::new(Memory {
        control: MemoryControl::default(),
        backend: Backend::Axi(Box::new(
            Axi::new(
                midi_words,
                0x03000000,
                8,
                Box::new(crate::rtl_memory::Port {
                    controller,
                    advances_clock: false,
                }),
                MEMORY_SEED + 1,
            )
            .with_prefix("midi_"),
        )),
        words: midi_words,
        base: 0x03000000,
        width: 8,
        sample_words: None,
        sample_transactions: 0,
    }));
    sim.agent(midi_memory.clone());
    let emitted = Rc::new(RefCell::new(MidiOutput(Vec::with_capacity(
        MIDI_OUTPUT_CAPACITY,
    ))));
    sim.agent(emitted.clone());
    pipeline.audio.load_compiled(&mut sim, &compiled);
    pipeline.store.load(&mut sim, &serde_json::json!({"chunk_frames":FPGA_CHUNK_FRAMES,"block_chunks":FPGA_BLOCK_CHUNKS,"chunk_limit":MEMORY_BYTES/(FPGA_CHUNK_FRAMES*4),
        "samples":[],"free_chunks":[],"views":(0..LOOPS*channels).map(|_| serde_json::json!({"chunks":[],"frames":0})).collect::<Vec<_>>()}));
    pipeline.store.memory.borrow_mut().control.stalls = false;
    let (control, execution) =
        rt::channel(256, 3, snapshot::capacity(1, LOOPS as usize, 1).unwrap())
            .map_err(|e| anyhow!("channel: {e:?}"))?;
    let endpoint = rt::Endpoint::new(
        control,
        rt::Settings {
            graph: 1,
            topology: 1,
            limits: reference::limits(media),
            driver: DriverStatus {
                kind: DriverKind::Dummy,
                state: DriverRunState::Running,
                sample_rate: SAMPLE_RATE,
                buffer_frames: QUANTUM as u32,
            },
        },
    )
    .map_err(|e| anyhow!("endpoint: {e:?}"))?;
    let mut control = Control::new(endpoint, Outbox::new(256, 256, 0))?;
    control.configuration = (1, 1);
    sim.model.set_all(&[
        ("midi_arena_base", 0),
        ("midi_memory_arena_bytes", (midi_words * 4) as u64),
        ("midi_event_address", event_address as u64),
        ("midi_event_count", 0),
        ("midi_command_op", 0),
        ("midi_command_valid", 1),
        ("midi_response_ready", 0),
    ]);
    sim.until(5_000_000, "MIDI format admission", |m| {
        m.get("midi_command_ready") != 0
    });
    sim.idle(1);
    sim.model.set("midi_command_valid", 0);
    sim.until(5_000_000, "MIDI format completion", |m| {
        m.get("midi_response_valid") != 0
    });
    ensure!(
        sim.model.get("midi_response_error") == 0,
        "MIDI format failed"
    );
    sim.model.set("midi_response_ready", 1);
    sim.idle(1);
    sim.model.set("midi_response_ready", 0);
    let driver = Rtl {
        media,
        midi_memory,
        midi_packets: Vec::with_capacity(MIDI_INPUT_CAPACITY),
        midi_result: Vec::with_capacity(MIDI_OUTPUT_CAPACITY),
        emitted,
        event_address,
        sim,
        pipeline,
        compiled,
        execution,
        frame: 0,
        inputs: std::array::from_fn(|_| vec![0; QUANTUM]),
        outputs: std::array::from_fn(|_| vec![0.; QUANTUM]),
        snapshot: vec![0; snapshot::capacity(1, LOOPS as usize, 1).unwrap()],
        mode,
        activity,
    };
    Ok(runner::run("fpga-rtl", fixture, control, driver))
}

struct Rtl<'a> {
    sim: Sim<'a>,
    pipeline: Pipeline,
    compiled: CompiledProgram,
    execution: rt::Execution,
    frame: u64,
    inputs: [Vec<i32>; 2],
    outputs: [Vec<f32>; 2],
    snapshot: Vec<u8>,
    mode: Mode,
    media: Media,
    midi_memory: Shared,
    midi_packets: Vec<MidiPacket>,
    midi_result: Vec<MidiPacket>,
    emitted: Rc<RefCell<MidiOutput>>,
    event_address: usize,
    activity: Rc<RefCell<Activity>>,
}
impl Rtl<'_> {
    fn stalls(&self) -> [u64; 4] {
        match &self.pipeline.store.memory.borrow().backend {
            Backend::Axi(axi) => [
                axi.address_stalls,
                axi.write_stalls,
                axi.response_stalls,
                axi.submission_stalls,
            ],
            _ => [0; 4],
        }
    }
    fn word(&mut self, slot: u32, word: u32) -> u64 {
        self.sim
            .model
            .read_indexed(STATE_RAM, u64::from(slot * 16 + word))
    }
    fn command(
        pipeline: &mut Pipeline,
        sim: &mut Sim<'_>,
        compiled: &CompiledProgram,
        frame: u64,
        command: Command,
    ) -> Code {
        let id = match command {
            Command::Transition(id, _)
            | Command::LoopGain(id, _)
            | Command::ClearPlannedTransitions(id, _) => id,
            _ => return Code::Unsupported,
        };
        if id >= LOOPS {
            return Code::TargetUnavailable;
        }
        let mut words: [u64; 16] = std::array::from_fn(|i| {
            sim.model
                .read_indexed(STATE_RAM, u64::from(id * 16 + i as u32))
        });
        use rt_fpga_rtl::host::control::{apply_plan, clear_plan, gain_value, loop_state};
        let key = rt_fpga_compiler::Identity {
            slot: id,
            generation: 1,
        };
        let result = match command {
            Command::LoopGain(_, gain) => {
                if !gain.is_finite() {
                    return Code::InvalidValue;
                }
                let Some(address) = compiled
                    .bindings
                    .iter()
                    .find(|b| b.id.slot == id)
                    .and_then(|b| b.gain)
                else {
                    return Code::Unsupported;
                };
                pipeline.audio.program.configure(
                    sim,
                    true,
                    address.into(),
                    Some(&gain_value(gain)),
                    false,
                );
                return Code::Applied;
            }
            Command::Transition(_, plan) => {
                let state = loop_state(key, &words, None);
                let source = (id != 0).then_some(rt_fpga_reference::transitions::LoopId {
                    slot: 0,
                    generation: 1,
                });
                apply_plan(&mut words, state, source, frame, plan)
            }
            Command::ClearPlannedTransitions(_, mask) => {
                clear_plan(&mut words, mask);
                Ok(Code::Applied)
            }
            _ => unreachable!(),
        };
        match result {
            Ok(code) => {
                let mut row = Bits::zero(1024);
                for (index, word) in words.into_iter().enumerate() {
                    row.set(index * 64, 64, word);
                }
                loop_image::write_words(&mut pipeline.audio.program, sim, id, &row, 0, 16);
                code
            }
            Err(code) => code,
        }
    }
    fn publish(&mut self) -> Result<()> {
        let mut pos = snapshot::HEADER;
        for id in 0..LOOPS {
            let flags = self.word(id, 0);
            let mode = (flags >> 33) & 3;
            let record = snapshot::Record::Telemetry(Telemetry::Loop {
                loop_id: LoopSlotId(SlotId::new(id, 1)),
                mode: match mode {
                    0 => LoopMode::Stopped,
                    1 => LoopMode::Playing,
                    2 => LoopMode::Recording,
                    _ => anyhow::bail!("invalid RTL mode"),
                },
                next_mode: None,
                next_delay: None,
                content_revision: self.word(id, 6),
                recording_armed: false,
                capture_incomplete: false,
                timing_revision: 0,
                length: if mode == 2 {
                    self.word(id, 4).saturating_sub(QUANTUM as u64)
                } else {
                    self.word(id, 3)
                },
                position: self.word(id, 8),
                peaks: Peaks::default(),
            });
            pos += codec::encode_snapshot(&record, &mut self.snapshot[pos..])
                .map_err(|e| anyhow!("snapshot: {e:?}"))?;
            let flags = self.word(id, 1);
            let mut step =
                |index: usize| -> Result<Option<protocol::immediate::PlannedTransition>> {
                    let base = 32 + index * 5;
                    if flags & (1 << base) == 0 {
                        return Ok(None);
                    }
                    Ok(Some(protocol::immediate::PlannedTransition {
                        mode: TransitionMode::from_raw(((flags >> (base + 2)) & 3) as u32)
                            .ok_or_else(|| anyhow!("invalid RTL plan mode"))?,
                        skip: self.word(id, if index == 0 { 11 } else { 14 }).try_into()?,
                    }))
                };
            let record = snapshot::Record::LoopPlan {
                id: LoopSlotId(SlotId::new(id, 1)),
                plan: protocol::immediate::TransitionPlan {
                    now: None,
                    first: step(0)?,
                    second: step(1)?,
                },
                failures: 0,
            };
            pos += codec::encode_snapshot(&record, &mut self.snapshot[pos..])
                .map_err(|e| anyhow!("snapshot: {e:?}"))?;
        }
        snapshot::Header {
            frame: self.frame,
            graph: 1,
            topology: 1,
            records: 2 * LOOPS,
            loops: LOOPS,
            strips: 1,
            buses: 1,
        }
        .write(&mut self.snapshot);
        self.execution
            .snapshots
            .publish(&self.snapshot[..pos])
            .map_err(|e| anyhow!("snapshot publish: {e:?}"))?;
        Ok(())
    }
}
impl Driver for Rtl<'_> {
    fn next_frame(&self) -> u64 {
        self.frame
    }
    fn fill_input(&mut self) {
        for (channel, samples) in self.inputs.iter_mut().enumerate() {
            for (offset, sample) in samples.iter_mut().enumerate() {
                *sample = rt_fpga_reference::fixed::Fixed::from_f32(input(
                    channel,
                    self.frame + offset as u64,
                ))
                .0;
            }
        }
        self.midi_packets.clear();
        if self.media.midi() {
            midi_input(self.frame, &mut self.midi_packets);
            let mut memory = self.midi_memory.borrow_mut();
            if let Backend::Axi(bus) = &mut memory.backend {
                for (i, packet) in self.midi_packets.iter().enumerate() {
                    let packed =
                        u32::from_le_bytes([packet.bytes[0], packet.bytes[1], packet.bytes[2], 3]);
                    let address = self.event_address / 4 + i * 4;
                    bus.words[address..address + 4].copy_from_slice(&[0, packet.offset, packed, 0]);
                }
            }
        }
    }
    fn process(&mut self) -> Result<Measurement> {
        self.midi_result.clear();
        self.emitted.borrow_mut().0.clear();
        let midi_stalls_start = memory_stalls(&self.midi_memory);
        let midi_reads = self.midi_memory.borrow().reads();
        let midi_writes = self.midi_memory.borrow().writes();
        let start = Instant::now();
        self.activity.borrow_mut().queue_peak = 0;
        let activity_start = *self.activity.borrow();
        let stalls_start = self.stalls();
        let cycles = self.sim.cycles;
        let reads = self.pipeline.store.memory.borrow().reads();
        let writes = self.pipeline.store.memory.borrow().writes();
        self.execution.commands.begin_callback();
        while let Some(group) = self.execution.commands.pop() {
            let header = group.header;
            let code = if (header.graph(), header.topology()) != (1, 1) {
                Code::StaleConfiguration
            } else if header.deadline().is_some_and(|limit| self.frame > limit) {
                Code::DeadlineMissed
            } else {
                Code::Applied
            };
            group.respond(
                header.complete(self.frame, code),
                |record: CommandRecord| {
                    let result = if code == Code::Applied {
                        Self::command(
                            &mut self.pipeline,
                            &mut self.sim,
                            &self.compiled,
                            self.frame,
                            record.command(),
                        )
                    } else {
                        code
                    };
                    record.complete(result)
                },
            );
        }
        ensure!(
            self.sim.model.get("sync_frame") == self.frame,
            "RTL frame counter mismatch"
        );
        for (channel, samples) in self
            .inputs
            .iter()
            .enumerate()
            .take(self.media.audio_channels())
        {
            for (offset, &sample) in samples.iter().enumerate() {
                self.pipeline.audio.sample_config(
                    &mut self.sim,
                    0,
                    (channel * QUANTUM + offset) as u64,
                    Some(sample),
                    false,
                );
            }
        }
        self.pipeline.audio.clear_jobs();
        self.sim.model.set_all(&[
            ("run_buffers", self.pipeline.audio.buffers),
            ("run_inputs", self.media.audio_channels() as u64),
            ("run_outputs", self.media.audio_channels() as u64),
            ("midi_event_count", self.midi_packets.len() as u64),
            ("job_enable", 1),
            ("drain_ready", 0),
        ]);
        self.pipeline.audio.program.run_inputs(&mut self.sim, &[]);
        let mut admitted = false;
        for _ in 0..100 {
            if self.sim.tick(&["run_ready"]).0.get("run_ready") != 0 {
                admitted = true;
                break;
            }
        }
        ensure!(admitted, "RTL quantum admission failed");
        self.sim.model.set("run_valid", 0);
        self.sim
            .until(20_000_000, "RTL quantum completion", |m| m.get("done") != 0);
        ensure!(
            self.sim.model.get("error_code") == 0,
            "RTL processing fault at stage {}: {}",
            self.sim.model.get("error_stage"),
            self.sim.model.get("error_code")
        );
        ensure!(
            self.sim.model.get("queued_jobs") == 0 && self.sim.model.get("busy") == 0,
            "RTL completion before processing retired"
        );
        self.sim.idle(1);
        ensure!(
            self.sim.model.get("midi_error_code") == 0,
            "RTL MIDI processing fault: {}",
            self.sim.model.get("midi_error_code")
        );
        let fabric_cycles = self.sim.cycles - cycles;
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let activity = *self.activity.borrow();
        let stalls = self.stalls();
        self.midi_result.extend_from_slice(&self.emitted.borrow().0);
        for channel in 0..self.media.audio_channels() {
            for offset in 0..QUANTUM {
                let raw = self
                    .sim
                    .model
                    .read_indexed(OUTPUT_RAM, (channel * QUANTUM + offset) as u64)
                    as u32 as i32;
                self.outputs[channel][offset] = raw as f32 / (1u32 << 24) as f32;
            }
        }
        self.publish()?;
        self.frame += QUANTUM as u64;
        let memory = self.pipeline.store.memory.borrow();
        let midi_stalls = memory_stalls(&self.midi_memory);
        Ok(Measurement {
            elapsed_ns,
            fabric_cycles: Some(fabric_cycles),
            reads: memory.reads() - reads,
            writes: memory.writes() - writes,
            storage_bytes: (self.sim.model.get("core.audio.core.media.slot_count")
                - self.sim.model.get("core.audio.core.media.free_count"))
                * FPGA_BLOCK_FRAMES as u64
                * 4
                + (u64::from(reference::midi_limits().blocks)
                    - self.sim.model.get("core.midi.content.free_count"))
                    * u64::from(MIDI_BLOCK_EVENTS)
                    * 16,
            reclaimed: 0,
            state_cycles: activity.state - activity_start.state,
            job_cycles: activity.jobs - activity_start.jobs,
            queue_peak: activity.queue_peak,
            axi_address_stall_cycles: stalls[0] - stalls_start[0],
            axi_write_stall_cycles: stalls[1] - stalls_start[1],
            axi_response_stall_cycles: stalls[2] - stalls_start[2],
            dram_queue_stall_cycles: stalls[3] - stalls_start[3],
            midi_axi_address_stall_cycles: midi_stalls[0] - midi_stalls_start[0],
            midi_axi_write_stall_cycles: midi_stalls[1] - midi_stalls_start[1],
            midi_axi_response_stall_cycles: midi_stalls[2] - midi_stalls_start[2],
            midi_dram_queue_stall_cycles: midi_stalls[3] - midi_stalls_start[3],
            midi_storage_blocks: self.media.midi().then_some(
                u64::from(reference::midi_limits().blocks)
                    - self.sim.model.get("core.midi.content.free_count"),
            ),
            midi_reads: self.midi_memory.borrow().reads() - midi_reads,
            midi_writes: self.midi_memory.borrow().writes() - midi_writes,
            midi_input_events: self.midi_packets.len() as u64,
            midi_output_events: self.midi_result.len() as u64,
            ..Default::default()
        })
    }
    fn midi_output(&self) -> &[MidiPacket] {
        &self.midi_result
    }
    fn output(&self, channel: usize) -> &[f32] {
        &self.outputs[channel]
    }
    fn finish(&mut self) -> Result<serde_json::Value> {
        let mut value = self.pipeline.store.memory.borrow_mut().report(true);
        value["midi_port"] = self.midi_memory.borrow_mut().report(false);
        value["native_scope"] = serde_json::json!(
            "one controller shared by audio and MIDI; do not add duplicate native counters"
        );
        Ok(value)
    }
    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"driver":"stepped-dummy","numeric":"Q8.24","timing":"rtl-fabric-cycles","fabric_hz":FPGA_CLOCK_HZ,"memory_model":format!("{:?}",self.mode),"memory_seed":MEMORY_SEED,"shared_memory_controller":true,"memory_topology":"audio and MIDI AXI ports share one timing controller; one advance per fabric cycle",
            "chunk_frames":FPGA_CHUNK_FRAMES,"block_chunks":FPGA_BLOCK_CHUNKS,"media_capacity_bytes":MEMORY_BYTES,
            "window":"command configuration, input upload, SP/JP/media execution, mandatory completion and queue drain; passive output/state probes do not advance clocks",
            "arm_service_time":"not modeled","waveform_clock_period_ps":6666,"simulation_optimization":3,"simulation_tracing":false,"provenance":rt_fpga_rtl::report::provenance(),"midi_memory":self.midi_memory.borrow_mut().report(false),"memory":self.pipeline.store.memory.borrow_mut().report(false)})
    }
}

struct MidiOutput(Vec<MidiPacket>);
impl Agent for MidiOutput {
    fn drive(
        &mut self,
        model: &mut rt_fpga_rtl::model::Model<'_>,
        edges: rt_fpga_rtl::clock::Edges,
    ) {
        if edges.rises(0) {
            model.set_all(&[("midi_packet_ready", 1), ("midi_packet_refused", 0)]);
        }
    }
    fn sample(&mut self, model: &rt_fpga_rtl::model::Model<'_>, edges: rt_fpga_rtl::clock::Edges) {
        if edges.rises(0)
            && model.get("reset") == 0
            && model.get("midi_packet_valid") != 0
            && model.get("midi_packet_ready") != 0
        {
            assert_eq!(model.get("midi_packet_length"), 3);
            assert_eq!(model.get("midi_packet_destination"), 0);
            assert!(
                self.0.len() < self.0.capacity(),
                "MIDI output exceeds declared fixture capacity"
            );
            let bytes = (model.get("midi_packet_data") as u32).to_le_bytes();
            self.0.push(MidiPacket {
                offset: model.get("midi_packet_offset") as u32,
                bytes: bytes[..3].try_into().unwrap(),
            });
        }
    }
}

fn memory_stalls(memory: &Shared) -> [u64; 4] {
    match &memory.borrow().backend {
        Backend::Axi(axi) => [
            axi.address_stalls,
            axi.write_stalls,
            axi.response_stalls,
            axi.submission_stalls,
        ],
        _ => [0; 4],
    }
}
