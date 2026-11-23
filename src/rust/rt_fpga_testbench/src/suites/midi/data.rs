use super::Memory;
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    model::{Compiled, Design, Model},
    sim::{Agent, Sim},
};
use rt_fpga_compiler::{MidiOperation as M, StateProgramTarget};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{Job, JobProcessor},
    midi::{BufferLimits, InputEvent, Message, MidiMemory, OutputEvent, StoreLimits},
    pipeline::Pipeline,
    state::PlannedTransition,
    storage::MediaStore,
    target::RustTarget,
    transitions::{LoopId, Operation},
};
use std::{cell::RefCell, rc::Rc};
pub mod axi;
pub mod benchmark;
pub mod pipeline;
#[path = "../../../../rt_fpga_compiler/tests/support/programs.rs"]
mod programs;

fn assert_scheduled_jobs(mut actual: Vec<serde_json::Value>, expected: &[Job]) {
    fn expand_merge(
        jobs: &[Job],
        source: rt_fpga_reference::midi::BufferId,
        before: usize,
        leaves: &mut Vec<u16>,
        removed: &mut [bool],
    ) {
        let uses: usize =
            jobs.iter()
                .map(|job| match *job {
                    Job::MidiMerge { left, right, .. } => {
                        usize::from(left == source) + usize::from(right == source)
                    }
                    Job::MidiAppend { source: input, .. }
                    | Job::MidiOutput { source: input, .. } => usize::from(input == source),
                    _ => 0,
                })
                .sum();
        if uses != 1 {
            leaves.push(source.0);
            return;
        }
        if let Some((index, Job::MidiMerge { left, right, .. })) =
            jobs[..before].iter().enumerate().rev().find(|(_, job)|
                matches!(job, Job::MidiMerge { destination, .. } if *destination == source))
        {
            removed[index] = true;
            expand_merge(jobs, *left, index, leaves, removed);
            expand_merge(jobs, *right, index, leaves, removed);
        } else {
            leaves.push(source.0);
        }
    }
    let mut removed = vec![false; expected.len()];
    let mut outputs = expected
        .iter()
        .enumerate()
        .filter_map(|(index, job)| match *job {
            Job::MidiOutput { source, port } => Some((index, source, port)),
            _ => None,
        });
    let mut fused = std::collections::BTreeMap::new();
    for job in actual.iter().filter(|job| {
        matches!(
            job["kind"].as_str(),
            Some("midi_output" | "midi_merge_output")
        )
    }) {
        let (index, source, port) = outputs.next().expect("extra MIDI output job");
        if job["kind"] == "midi_merge_output" {
            let mut leaves = Vec::new();
            expand_merge(expected, source, index, &mut leaves, &mut removed);
            let output =
                serde_json::json!({"kind":"midi_merge_output","sources":leaves,"port":port});
            fused.insert(index, output);
        }
    }
    let mut expected: Vec<_> = expected
        .iter()
        .enumerate()
        .filter(|(index, _)| !removed[*index])
        .map(|(index, job)| {
            fused
                .remove(&index)
                .unwrap_or_else(|| serde_json::to_value(job).unwrap())
        })
        .collect();
    // Compare every descriptor while keeping the observable MIDI packet jobs in sequence.
    let key = |job: &serde_json::Value| match job["kind"].as_str() {
        Some("midi_restore_state" | "midi_cleanup_state" | "midi_output" | "midi_merge_output") => {
            None
        }
        _ => Some(job.to_string()),
    };
    actual.sort_by_cached_key(key);
    expected.sort_by_cached_key(key);
    assert_eq!(actual, expected);
}

pub fn logical(q: u32, mixed: bool) -> rt_fpga_compiler::LogicalStateProgram {
    let mut p = programs::mono_loops(q, 2);
    if !mixed {
        p.work_pass.clear();
        p.jobs_per_quantum = 0;
    }
    p.chunk_frames = q;
    p.resources.buffers = 8;
    p.resources.input_ports = 2;
    let ids = [p.loops[0].id, p.loops[1].id];
    p.with_midi_data(&[
        M::Playback {
            loop_id: ids[0],
            sequence: 0,
            cursor: 0,
            channels: u16::MAX,
            destination: 2,
        },
        M::Playback {
            loop_id: ids[1],
            sequence: 1,
            cursor: 1,
            channels: u16::MAX,
            destination: 3,
        },
        M::Input {
            port: 0,
            destination: 0,
        },
        M::Input {
            port: 1,
            destination: 1,
        },
        M::Capture {
            loop_id: ids[0],
            sequence: 0,
            source: 0,
        },
        M::Capture {
            loop_id: ids[1],
            sequence: 1,
            source: 1,
        },
        M::Merge {
            left: 3,
            right: 2,
            destination: 4,
        },
        M::Output { source: 4, port: 0 },
    ])
    .unwrap()
}
pub fn logical_expression(q: u32, mixed: bool) -> rt_fpga_compiler::LogicalStateProgram {
    logical(q, mixed)
        .with_midi_data(
            &(0..2)
                .map(|slot| M::State {
                    loop_id: rt_fpga_compiler::Identity {
                        slot,
                        generation: 1,
                    },
                    sequence: slot as u16,
                    input: slot as u16,
                    outputs: 1,
                    channels: 1 << slot,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
}
pub fn limits() -> StoreLimits {
    StoreLimits {
        block_events: 8,
        blocks: 512,
        max_events: 4096,
        views: 32,
        retirements: 32,
    }
}
pub fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 1,
    }
}
pub fn event(input: u16, offset: u32, bytes: &[u8]) -> InputEvent {
    InputEvent {
        input,
        offset,
        message: Message::parse(bytes).unwrap(),
    }
}
pub fn packed(message: Message) -> u32 {
    message
        .as_bytes()
        .iter()
        .enumerate()
        .fold((message.as_bytes().len() as u32) << 24, |v, (i, b)| {
            v | (u32::from(*b) << (8 * i))
        })
}
pub struct Oracle {
    pub pipeline: Pipeline,
    pub jobs: JobProcessor,
    pub midi: MidiMemory,
    pub audio: MediaStore,
    pub emitted: Vec<OutputEvent>,
    pub q: u32,
    pub mixed: bool,
}
impl Oracle {
    pub fn new(q: u32, mixed: bool) -> Self {
        Self::build(q, mixed, false)
    }
    pub fn build(q: u32, mixed: bool, expression: bool) -> Self {
        let logical = if expression {
            logical_expression(q, mixed)
        } else {
            logical(q, mixed)
        };
        let mut audio = MediaStore::new(q as usize).unwrap();
        let views: Vec<_> = (0..2)
            .map(|_| audio.create_recording().unwrap().0 as u64)
            .collect();
        audio.reserve_chunk_capacity(128).unwrap();
        for &view in &views {
            audio
                .reserve_view_capacity(
                    rt_fpga_reference::storage::ViewId(view as usize),
                    q as usize * 128,
                )
                .unwrap();
        }
        let compiled = RustTarget::new(&views).build(&logical).unwrap();
        let mut pipeline = Pipeline::prepare(
            compiled.plan,
            compiled.program,
            compiled.memory,
            logical.jobs_per_quantum as usize,
        )
        .unwrap();
        pipeline.trace_jobs(true);
        let mut jobs = JobProcessor::new(q as usize, 8);
        jobs.prepare_midi(BufferLimits {
            buffers: 8,
            cursors: 2,
            scratch_events: 16384,
            input_ports: 2,
            output_ports: 2,
        })
        .unwrap();
        let mut midi = MidiMemory::prepare(limits(), 2).unwrap();
        if expression {
            midi.prepare_state(2, 2).unwrap();
            pipeline.prepare_midi_state(&mut midi).unwrap();
        }
        Self {
            pipeline,
            jobs,
            midi,
            audio,
            emitted: Vec::new(),
            q,
            mixed,
        }
    }
    pub fn command(&mut self, slot: u32, operation: Operation) {
        self.pipeline
            .write_plans(
                id(slot),
                [
                    Some(PlannedTransition {
                        sequence: self.pipeline.state().frame() + 1,
                        effective: self.pipeline.state().frame(),
                        on_pulse: false,
                        remaining_ticks: 0,
                        operation,
                    }),
                    None,
                ],
            )
            .unwrap();
    }
    pub fn run(&mut self, events: &[InputEvent]) -> Vec<Job> {
        self.emitted.clear();
        let input = vec![Fixed::from_f32(0.25); self.q as usize];
        let mut left = input.clone();
        let mut right = left.clone();
        let inputs = [input.as_slice()];
        self.pipeline
            .process_with_midi(
                &mut self.jobs,
                &mut self.audio,
                if self.mixed { &inputs } else { &[] },
                &mut [&mut left, &mut right],
                self.q as usize,
                &mut self.midi,
                events,
                &mut |e| {
                    self.emitted.push(e);
                    Ok(())
                },
            )
            .unwrap();
        self.pipeline.emitted_jobs().to_vec()
    }
}
#[derive(Default)]
pub struct Sink {
    pub events: Vec<OutputEvent>,
    pub remaining: Option<usize>,
    prefix: bool,
    cycle: u64,
}
impl Sink {
    pub fn mixed() -> Self {
        Self {
            prefix: true,
            ..Default::default()
        }
    }
}
impl Agent for Sink {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            let p = if self.prefix { "midi_" } else { "" };
            m.set(&format!("{p}packet_ready"), u64::from(self.cycle % 5 != 0));
            m.set(
                &format!("{p}packet_refused"),
                u64::from(self.remaining == Some(0)),
            );
            self.cycle += 1;
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        let p = if self.prefix { "midi_" } else { "" };
        if e.rises(0)
            && m.get("reset") == 0
            && m.get(&format!("{p}packet_valid")) != 0
            && m.get(&format!("{p}packet_ready")) != 0
            && self.remaining != Some(0)
        {
            let data = (m.get(&format!("{p}packet_data")) as u32).to_le_bytes();
            self.events.push(OutputEvent {
                destination: m.get(&format!("{p}packet_destination")) as u16,
                offset: m.get(&format!("{p}packet_offset")) as u32,
                message: Message::parse(&data[..m.get(&format!("{p}packet_length")) as usize])
                    .unwrap(),
            });
            if let Some(n) = &mut self.remaining {
                *n -= 1;
            }
        }
    }
}
pub fn design() -> Design {
    let mut d = Design::new(
        "midi_data_jobs",
        ["midi_data_jobs", "midi_store"]
            .iter()
            .map(|n| crate::root().join(format!("fpga/midi/rtl/{n}.sv")))
            .collect(),
    );
    d.includes.push(crate::root().join("fpga/midi/rtl"));
    d.parameters(&[
        ("LOOPS", 2),
        ("INPUTS", 2),
        ("DESTINATIONS", 2),
        ("BUFFERS", 8),
        ("CURSORS", 2),
        ("SCRATCH_EVENTS", 16384),
        ("BLOCK_EVENTS", 8),
        ("BLOCKS", 512),
        ("MAX_EVENTS", 4096),
        ("VIEWS", 32),
        ("RETIREMENTS", 32),
    ])
}
pub fn compile() -> Result<Compiled> {
    design().build(&crate::root().join("work/marlin-suite/midi-data"))
}
pub fn job_bits(job: Job) -> Option<Bits> {
    let (kind, a, b, p0, p1, p2) = match job {
        Job::MidiInput { port, destination } => (0x20, port, destination.0, 0, 0, 0),
        Job::MidiEmpty { destination } => (0x21, destination.0, 0, 0, 0, 0),
        Job::MidiReadView {
            owner,
            sequence,
            cursor,
            position,
            frames,
            restart,
            channels,
            destination,
        } => (
            0x22,
            sequence.0,
            destination.0,
            position,
            u64::from(frames)
                | (u64::from(cursor.0) << 32)
                | (u64::from(channels) << 40)
                | (u64::from(restart) << 56),
            u64::from(owner.slot) | (u64::from(owner.generation) << 32),
        ),
        Job::MidiBeginCapture { sequence } => (0x23, sequence.0, 0, 0, 0, 0),
        Job::MidiAppend {
            sequence,
            source,
            position,
            frames,
        } => (0x24, sequence.0, source.0, position, u64::from(frames), 0),
        Job::MidiFinishCapture { sequence } => (0x25, sequence.0, 0, 0, 0, 0),
        Job::MidiMerge {
            left,
            right,
            destination,
        } => (0x26, left.0, right.0, u64::from(destination.0), 0, 0),
        Job::MidiRestoreState {
            owner,
            sequence,
            position,
        } => (
            0x28,
            sequence.0,
            0,
            position,
            u64::from(owner.slot) | (u64::from(owner.generation) << 32),
            0,
        ),
        Job::MidiCleanupState { owner, sequence } => (
            0x29,
            sequence.0,
            0,
            0,
            u64::from(owner.slot) | (u64::from(owner.generation) << 32),
            0,
        ),
        Job::MidiOutput { source, port } => (0x27, source.0, port, 0, 0, 0),
        _ => return None,
    };
    let mut bits = Bits::zero(256);
    bits.set(0, 8, kind)
        .set(8, 16, u64::from(a))
        .set(24, 16, u64::from(b))
        .set(64, 64, p0)
        .set(128, 64, p1)
        .set(192, 64, p2);
    Some(bits)
}
pub struct Driver<'a> {
    pub sim: Sim<'a>,
    pub memory: Rc<RefCell<Memory>>,
    pub sink: Rc<RefCell<Sink>>,
    pub address: usize,
}
impl<'a> Driver<'a> {
    pub fn new(compiled: &'a Compiled, q: u32) -> Result<Self> {
        let mut sim = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        sim.max_ps = 1_000_000_000_000_000;
        let address = (sim.model.get("arena_bytes") as usize + 271) & !15;
        let memory = Rc::new(RefCell::new(Memory::new(address / 4 + 32768)));
        let sink = Rc::new(RefCell::new(Sink::default()));
        sim.agent(memory.clone());
        sim.agent(sink.clone());
        sim.agent(crate::protocol::ReadyValid::new(
            "memory_valid",
            "memory_ready",
            &["memory_address", "memory_write", "memory_write_data"],
        ));
        sim.agent(crate::protocol::ReadyValid::new(
            "packet_valid",
            "packet_ready",
            &[
                "packet_data",
                "packet_length",
                "packet_offset",
                "packet_destination",
            ],
        ));
        sim.model.set_all(&[
            ("reset", 1),
            ("control_enable", 1),
            ("quantum_frames", u64::from(q)),
            ("response_ready", 0),
        ]);
        sim.idle(2);
        sim.model.set("reset", 0);
        let mut d = Self {
            sim,
            memory,
            sink,
            address,
        };
        assert_eq!(d.control(0, [0; 5]).0, 0);
        Ok(d)
    }
    pub fn control(&mut self, op: u64, args: [u64; 5]) -> (u64, u64, u64, u64) {
        issue_control(&mut self.sim, op, args)
    }
    pub fn prepare(&mut self, events: &[InputEvent]) -> u64 {
        write_events(&mut self.memory.borrow_mut(), self.address, events);
        self.sink.borrow_mut().events.clear();
        self.sim.model.set_all(&[
            ("event_address", self.address as u64),
            ("event_count", events.len() as u64),
            ("prepare_valid", 1),
        ]);
        self.sim
            .until(100, "prepare admission", |m| m.get("prepare_ready") != 0);
        self.sim.idle(1);
        self.sim.model.set("prepare_valid", 0);
        self.sim
            .until(5_000_000, "prepare done", |m| m.get("prepare_done") != 0);
        let e = self.sim.model.get("prepare_error");
        self.sim.idle(1);
        e
    }
    pub fn job(&mut self, job: Job) -> u64 {
        issue_job(&mut self.sim, job)
    }
    pub fn finish(&mut self) {
        issue_finish(&mut self.sim)
    }
    pub fn quantum(&mut self, f: &mut Oracle, events: &[InputEvent]) {
        assert_eq!(self.prepare(events), 0);
        let jobs = f.run(events);
        for job in jobs {
            assert_eq!(self.job(job), 0, "{job:?}");
        }
        self.finish();
        assert_eq!(self.sink.borrow().events, f.emitted);
    }
}
pub(super) fn write_events(memory: &mut Memory, address: usize, events: &[InputEvent]) {
    for (i, e) in events.iter().enumerate() {
        memory.words[address / 4 + 4 * i..address / 4 + 4 * i + 4].copy_from_slice(&[
            u32::from(e.input),
            e.offset,
            packed(e.message),
            0,
        ]);
    }
}

pub fn issue_control(sim: &mut Sim<'_>, op: u64, args: [u64; 5]) -> (u64, u64, u64, u64) {
    sim.model.set("command_op", op);
    for (i, a) in args.into_iter().enumerate() {
        sim.model.set(&format!("command_arg{i}"), a);
    }
    sim.model
        .set_all(&[("command_valid", 1), ("response_ready", 0)]);
    sim.until(2_000_000, "control admission", |m| {
        m.get("command_ready") != 0
    });
    sim.idle(1);
    sim.model.set("command_valid", 0);
    sim.until(2_000_000, "control response", |m| {
        m.get("response_valid") != 0 || m.get("publication_valid") != 0
    });
    if sim.model.get("publication_valid") != 0 {
        sim.model.set("publication_commit", 1);
        sim.idle(1);
        sim.model.set("publication_commit", 0);
    }
    sim.until(100, "published response", |m| m.get("response_valid") != 0);
    let r = (
        sim.model.get("response_error"),
        sim.model.get("response_handle"),
        sim.model.get("response_frame"),
        sim.model.get("response_count"),
    );
    sim.model.set("response_ready", 1);
    sim.idle(1);
    sim.model.set("response_ready", 0);
    r
}
pub fn issue_job(sim: &mut Sim<'_>, job: Job) -> u64 {
    let Some(bits) = job_bits(job) else { return 0 };
    sim.model.set_bits("job_data", &bits);
    sim.model.set("job_valid", 1);
    sim.until(100, "job admission", |m| m.get("job_ready") != 0);
    sim.idle(1);
    sim.model.set("job_valid", 0);
    sim.until(5_000_000, "job done", |m| m.get("job_done") != 0);
    let e = sim.model.get("job_error");
    sim.idle(1);
    e
}
pub fn issue_finish(sim: &mut Sim<'_>) {
    sim.model.set("finish_valid", 1);
    sim.until(100, "finish admission", |m| m.get("finish_ready") != 0);
    sim.idle(1);
    sim.model.set("finish_valid", 0);
    sim.until(500_000, "finish done", |m| m.get("finish_done") != 0);
    assert_eq!(sim.model.get("finish_error"), 0);
    sim.idle(1);
}
pub fn prepare_core(sim: &mut Sim<'_>, address: u64, count: u64) -> u64 {
    sim.model.set_all(&[
        ("event_address", address),
        ("event_count", count),
        ("prepare_valid", 1),
    ]);
    sim.until(100, "prepare admission", |m| m.get("prepare_ready") != 0);
    sim.idle(1);
    sim.model.set("prepare_valid", 0);
    sim.until(5_000_000, "prepare done", |m| m.get("prepare_done") != 0);
    let e = sim.model.get("prepare_error");
    sim.idle(1);
    e
}
