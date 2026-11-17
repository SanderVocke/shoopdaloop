use crate::{
    fixture::*,
    reference,
    runner::{self, Control, Driver, Measurement, Run},
};
use anyhow::{anyhow, ensure, Result};
use rt_fpga_compiler::StateProgramTarget;
use rt_fpga_testbench::{
    bits::Bits,
    compiler::{CompiledProgram, RtlTarget},
    drivers::{pipeline::Pipeline, store::Geometry},
    loop_image,
    memory::{Backend, Mode},
    model::Probe,
    sim::{Agent, Sim},
    suites,
    timing::dram::Dram,
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

const CAPACITIES: [u64; 7] = [1024, 256, 1024, 32, 512, 2, 2];
const MEMORY_SEED: u64 = 20261117;
const STATE_RAM: &str = "core.pipeline.state_engine.loop_memory.memory";
const OUTPUT_RAM: &str = "core.pipeline.sample_engine.local_io.outputs_memory.memory";

#[derive(Default, Clone, Copy)]
struct Activity {
    state: u64,
    jobs: u64,
    queue_peak: u64,
}
impl Agent for Activity {
    fn sample(
        &mut self,
        model: &rt_fpga_testbench::model::Model<'_>,
        edges: rt_fpga_testbench::clock::Edges,
    ) {
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

pub fn run(fixture: Fixture, memory_model: &str, output: &Path) -> Result<Run> {
    let mode = match memory_model {
        "dramsim3" => Mode::Dramsim3,
        "fixed" => Mode::Fixed,
        _ => anyhow::bail!("unsupported memory model"),
    };
    let geometry = Geometry::new(
        [u64::from(LOOPS * CHANNELS), MEMORY_BYTES as u64 / 4, 1],
        128,
    );
    let logical = reference::graph()?;
    let views: Vec<_> = (0..u64::from(LOOPS * CHANNELS)).collect();
    let compiled = RtlTarget::new(CAPACITIES, LOOPS, &views)
        .build(&logical)
        .map_err(|e| anyhow!("RTL lowering: {e:?}"))?;
    let directory = output.with_extension("rtl");
    external(rt_fpga_testbench::report::configure(&directory, false))?;
    let mut design = suites::pipeline::design(CAPACITIES, &geometry, 64, mode, 1);
    for slot in 0..LOOPS {
        for word in [0, 1, 3, 4, 5, 6, 8, 11, 14] {
            design.probes.push(Probe::observe(&format!(
                "{STATE_RAM}[{}]",
                slot * 16 + word
            )));
        }
    }
    for index in 0..2 * QUANTUM {
        design
            .probes
            .push(Probe::observe(&format!("{OUTPUT_RAM}[{index}]")));
    }
    for name in ["slot_count", "free_count"] {
        design
            .probes
            .push(Probe::observe(&format!("core.media.{name}")));
    }
    let model = external(design.build(&rt_fpga_testbench::root().join("work/rt-benchmark-model")))?;
    let mut sim = Sim::new(external(model.benchmark_session())?, &[("clk", 6666, 0)]);
    sim.max_ps = u64::MAX;
    let activity = Rc::new(RefCell::new(Activity::default()));
    sim.agent(activity.clone());
    let mut pipeline = external(Pipeline::new(
        &mut sim,
        CAPACITIES,
        geometry,
        mode,
        MEMORY_SEED,
    ))?;
    if mode == Mode::Dramsim3 {
        let timing = external(Dram::default(&directory.join("dram-150mhz"), FPGA_CLOCK_HZ))?;
        let mut memory = pipeline.store.memory.borrow_mut();
        if let Backend::Axi(axi) = &mut memory.backend {
            axi.timing = Box::new(timing);
        }
    }
    pipeline.audio.load_compiled(&mut sim, &compiled);
    pipeline.store.load(&mut sim, &serde_json::json!({"chunk_frames":1,"block_chunks":128,"chunk_limit":MEMORY_BYTES/4,
        "samples":[],"free_chunks":[],"views":(0..LOOPS*CHANNELS).map(|_| serde_json::json!({"chunks":[],"frames":0})).collect::<Vec<_>>()}));
    pipeline.store.memory.borrow_mut().control.stalls = false;
    let (control, execution) =
        rt::channel(256, 3, snapshot::capacity(1, LOOPS as usize, 1).unwrap())
            .map_err(|e| anyhow!("channel: {e:?}"))?;
    let endpoint = rt::Endpoint::new(
        control,
        rt::Settings {
            graph: 1,
            topology: 1,
            limits: reference::limits(),
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
    let driver = Rtl {
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
    fn word(&self, slot: u32, word: u32) -> u64 {
        self.sim
            .model
            .get(&format!("{STATE_RAM}[{}]", slot * 16 + word))
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
        match command {
            Command::LoopGain(_, gain) => {
                if !gain.is_finite() {
                    return Code::InvalidValue;
                }
                let address = compiled
                    .bindings
                    .iter()
                    .find(|b| b.id.slot == id)
                    .and_then(|b| b.gain)
                    .unwrap();
                let value = rt_fpga_reference::fixed::Fixed::from_f32(gain).0;
                let bits = Bits::from(rt_fpga_testbench::component::value_words(
                    rt_fpga_testbench::component::Value::Ramp(rt_fpga_testbench::component::Ramp {
                        start: value,
                        target: value,
                        duration: 0,
                        elapsed: 0,
                    }),
                ))
                .resized(129);
                pipeline
                    .audio
                    .program
                    .configure(sim, true, address.into(), Some(&bits), false);
                Code::Applied
            }
            Command::Transition(_, plan) => {
                if plan.now.is_some() && (plan.first.is_some() || plan.second.is_some()) {
                    return Code::Unsupported;
                }
                let mut row = Bits::zero(1024);
                row.set(0, 64, sim.model.get(&format!("{STATE_RAM}[{}]", id * 16)));
                let mut flags = sim.model.get(&format!("{STATE_RAM}[{}]", id * 16 + 1));
                flags &= !(0x3ffu64 << 32);
                row.set(64, 64, flags);
                let mut install = |index: usize, mode: TransitionMode, pulse: bool, skip: u32| {
                    let flag = 96 + index * 5;
                    row.set(flag, 1, 1)
                        .set(flag + 1, 1, u64::from(pulse))
                        .set(flag + 2, 2, mode as u64)
                        .set(flag + 4, 1, u64::from(mode == TransitionMode::Playing));
                    let base = (10 + index * 3) * 64;
                    row.set(base, 64, frame)
                        .set(base + 64, 64, skip as u64)
                        .set(base + 128, 64, 0);
                };
                if let Some(mode) = plan.now {
                    install(0, mode, false, 0);
                }
                for (index, step) in [plan.first, plan.second].into_iter().enumerate() {
                    if let Some(step) = step {
                        install(index, step.mode, true, step.skip);
                    }
                }
                row.set(56, 1, u64::from(plan.second.is_some()));
                loop_image::write_plans(&mut pipeline.audio.program, sim, id, &row);
                if plan.first.is_some() || plan.second.is_some() {
                    Code::Scheduled
                } else {
                    Code::Applied
                }
            }
            Command::ClearPlannedTransitions(_, mask) => {
                let mut flags =
                    Bits::from_u64(sim.model.get(&format!("{STATE_RAM}[{}]", id * 16 + 1)))
                        .resized(129);
                for index in 0..2 {
                    if mask as u8 & (1 << index) != 0 {
                        flags.set(32 + index * 5, 1, 0);
                    }
                }
                loop_image::access(
                    &mut pipeline.audio.program,
                    sim,
                    id as u64 * 16 + 1,
                    Some(&flags),
                );
                Code::Applied
            }
            _ => unreachable!(),
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
            let step = |index: usize| -> Result<Option<protocol::immediate::PlannedTransition>> {
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
    }
    fn process(&mut self) -> Result<Measurement> {
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
        for (channel, samples) in self.inputs.iter().enumerate() {
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
            ("run_inputs", 2),
            ("run_outputs", 2),
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
            .until(2_000_000, "RTL quantum completion", |m| m.get("done") != 0);
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
        let fabric_cycles = self.sim.cycles - cycles;
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let activity = *self.activity.borrow();
        let stalls = self.stalls();
        for channel in 0..2 {
            for offset in 0..QUANTUM {
                let raw = self
                    .sim
                    .model
                    .get(&format!("{OUTPUT_RAM}[{}]", channel * QUANTUM + offset))
                    as u32 as i32;
                self.outputs[channel][offset] = raw as f32 / (1u32 << 24) as f32;
            }
        }
        self.publish()?;
        self.frame += QUANTUM as u64;
        let memory = self.pipeline.store.memory.borrow();
        Ok(Measurement {
            elapsed_ns,
            fabric_cycles: Some(fabric_cycles),
            reads: memory.reads() - reads,
            writes: memory.writes() - writes,
            storage_bytes: (self.sim.model.get("core.media.slot_count")
                - self.sim.model.get("core.media.free_count"))
                * 128
                * 4,
            reclaimed: 0,
            state_cycles: activity.state - activity_start.state,
            job_cycles: activity.jobs - activity_start.jobs,
            queue_peak: activity.queue_peak,
            axi_address_stall_cycles: stalls[0] - stalls_start[0],
            axi_write_stall_cycles: stalls[1] - stalls_start[1],
            axi_response_stall_cycles: stalls[2] - stalls_start[2],
            dram_queue_stall_cycles: stalls[3] - stalls_start[3],
        })
    }
    fn output(&self, channel: usize) -> &[f32] {
        &self.outputs[channel]
    }
    fn finish(&mut self) -> Result<serde_json::Value> {
        Ok(self.pipeline.store.memory.borrow_mut().report(true))
    }
    fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"driver":"stepped-dummy","numeric":"Q8.24","timing":"rtl-fabric-cycles","fabric_hz":FPGA_CLOCK_HZ,"memory_model":format!("{:?}",self.mode),"memory_seed":MEMORY_SEED,
            "window":"command configuration, input upload, SP/JP/media execution, mandatory completion and queue drain; passive output/state probes do not advance clocks",
            "arm_service_time":"not modeled","waveform_clock_period_ps":6666,"simulation_optimization":3,"simulation_tracing":false,"provenance":rt_fpga_testbench::report::provenance(),"memory":self.pipeline.store.memory.borrow_mut().report(false)})
    }
}
