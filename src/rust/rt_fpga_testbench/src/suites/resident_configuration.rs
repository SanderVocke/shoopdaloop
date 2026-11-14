//! Canonical graph activation through the actual autonomous audio/control wrapper.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    drivers::{
        frames::Frames, state::Program, store::Store, stream::Source, transition::Transition,
    },
    memory::Mode,
    model::{Model, Probe},
    sim::{Agent, Sim},
    suites::continuous::{self, Config},
};
use rt_audio_math::Sample;
use rt_fpga_adapter::{
    audio_actions::{Identity, Progress, Record, Service},
    audio_control::{self, Completion},
    audio_image::{Dimensions, Transfer},
    image::{Instruction, Ramp, Value},
    observations::{Reader, Reference},
    protocol, resident,
    resident_parameters::Command,
    resident_stages as stage,
};
use rt_runtime::{Epoch, graph::activation::StageId};
use shoop_loops::{LoopId, runtime::NodePairStageId, topology::staging};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

struct Replies(Rc<RefCell<VecDeque<Completion>>>);
impl Agent for Replies {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("control_result_valid") != 0 && m.get("control_result_ready") != 0 {
            self.0
                .borrow_mut()
                .push_back(Completion::decode(&m.bits("control_result_data").array()).unwrap());
        }
    }
}
struct Host {
    source: Rc<RefCell<Source>>,
    replies: Rc<RefCell<VecDeque<Completion>>>,
    clock: Frames,
    frame: u64,
    batch: u64,
    graph_before_initial: bool,
}
impl Host {
    fn pulse(&mut self, s: &mut Sim<'_>) {
        let input = Bits(vec![
            (100003 + self.frame * 37) as u32,
            (200009 + self.frame * 31) as u32,
        ]);
        let (_, actual) = self.clock.pulse(s, Some(&input));
        for ch in 0..2 {
            let first = if self.graph_before_initial { 32 } else { 64 };
            let expected = if self.frame < 64 + first * 32 {
                0
            } else {
                let at = self.frame - 64;
                let gain = rt_audio_math::Ramp {
                    start: rt_audio_math::Q824::ZERO,
                    target: rt_audio_math::Q824::ONE,
                    duration: 8192,
                    elapsed: 0,
                }
                .at((at - if at < 64 * 32 { 32 * 32 } else { 64 * 32 }) as u32);
                rt_audio_math::Q824(
                    (if ch == 0 {
                        100003 + at * 37
                    } else {
                        200009 + at * 31
                    }) as i32,
                )
                .multiply(gain)
                .0
            };
            assert_eq!(
                actual.get(ch * 32, 32) as u32 as i32,
                expected,
                "canonical activation audio frame {} channel {ch}",
                self.frame
            );
        }
        assert_eq!(s.model.get("recovery_requested"), 0);
        self.frame += 1;
    }
    fn until(&mut self, s: &mut Sim<'_>, frame: u64) {
        while self.frame < frame {
            self.pulse(s);
        }
    }
    fn batch(&mut self, s: &mut Sim<'_>, operations: Vec<Transfer>) -> Vec<Completion> {
        assert!(self.replies.borrow().is_empty());
        self.batch += 1;
        let count = operations.len();
        {
            let mut source = self.source.borrow_mut();
            source.queue.push_back(Bits::from(
                audio_control::header(0, self.batch, count as u32, 64).unwrap(),
            ));
            for t in &operations {
                source.queue.push_back(Bits::from(t.packet(0)));
            }
        }
        let deadline = self.frame + 32 * 20;
        while self.replies.borrow().len() < count {
            assert!(self.frame < deadline, "control batch stuck");
            self.pulse(s);
        }
        let replies: Vec<_> = self.replies.borrow_mut().drain(..).collect();
        for (i, r) in replies.iter().enumerate() {
            assert_eq!(
                (r.batch, r.operation, r.opcode, r.configuration, r.epoch),
                (self.batch, i as u32, operations[i].opcode, true, 1)
            );
        }
        replies
    }
    fn ok(&mut self, s: &mut Sim<'_>, operations: Vec<Transfer>) -> Vec<Completion> {
        let replies = self.batch(s, operations);
        for reply in &replies {
            assert_eq!(reply.error, 0, "configuration opcode {}", reply.opcode);
        }
        replies
    }
    fn admit(
        &mut self,
        s: &mut Sim<'_>,
        owner: &mut Service,
        sequence: u64,
        at: u64,
        action: Transfer,
    ) -> Identity {
        let mut t = owner.schedule(sequence, at, action).unwrap();
        loop {
            let r = self.ok(s, vec![t]).pop().unwrap();
            match owner.finish(r.error, r.payload, r.quantum * 32).unwrap() {
                Progress::Transfer(next) => t = next,
                Progress::Admitted {
                    identity,
                    effective_frame,
                } => {
                    assert_eq!(effective_frame, at);
                    return identity;
                }
                _ => panic!("resident configuration admission refused"),
            }
        }
    }
    fn record(&mut self, s: &mut Sim<'_>, owner: &mut Service, id: Identity) -> Record {
        let t = owner.inspect(id, false).unwrap();
        let r = self.ok(s, vec![t]).pop().unwrap();
        match owner.finish(r.error, r.payload, r.quantum * 32).unwrap() {
            Progress::Record { record, .. } => record,
            _ => panic!("missing retained configuration"),
        }
    }
    fn stage_image(&mut self, s: &mut Sim<'_>, bank: u16, version: u64, relocated: bool) -> u64 {
        let r = self.ok(
            s,
            vec![Transfer::begin_bank(
                bank,
                version,
                Dimensions {
                    instructions: 6,
                    states: 4,
                    jobs: 16,
                    buffers: 4,
                    copies: 0,
                },
            )],
        );
        let nonce = protocol::get(&r[0].payload, 0, 64);
        let mut transfers = Vec::new();
        for (i, instruction) in program(if relocated { 2 } else { 0 })
            .into_iter()
            .enumerate()
        {
            transfers.push(Transfer::upload(
                false,
                i as u16,
                rt_fpga_adapter::image_wire::instruction_words(instruction).unwrap(),
                nonce,
            ));
        }
        for i in 0..4 {
            transfers.push(Transfer::upload(
                true,
                i,
                rt_fpga_adapter::image_wire::value_words(Value::Ramp(Ramp {
                    start: -17,
                    target: -17,
                    duration: 0,
                    elapsed: 0,
                })),
                nonce,
            ));
        }
        transfers.push(Command::begin(nonce, 2).transfer());
        for i in 0..2 {
            transfers.push(
                Command::binding(nonce, i, 0, if relocated { i as u16 + 2 } else { i as u16 })
                    .transfer(),
            );
        }
        self.ok(s, transfers);
        nonce
    }
}
fn program(ramp: u16) -> Vec<Instruction> {
    vec![
        Instruction::ProcessInput {
            port: 0,
            destination: 0,
        },
        Instruction::ProcessInput {
            port: 1,
            destination: 1,
        },
        Instruction::ProcessGain {
            ramp,
            source: 0,
            destination: 2,
        },
        Instruction::ProcessGain {
            ramp: ramp + 1,
            source: 1,
            destination: 3,
        },
        Instruction::ProcessOutput { source: 2, port: 0 },
        Instruction::ProcessOutput { source: 3, port: 1 },
    ]
}
pub fn activation() -> Result<()> {
    activation_inner(false)
}
pub fn graph_before_initial() -> Result<()> {
    activation_inner(true)
}
fn activation_inner(graph_before_initial: bool) -> Result<()> {
    let c = Config {
        q: 32,
        cf: 32,
        p: 32,
        channels: 2,
        control_depth: 64,
        ticks: 512,
        monitoring: false,
        routing: "identity".into(),
        ..Config::default()
    };
    let prepared = continuous::prepare(&c)?;
    let mut old: rt_fpga_adapter::image::Image = serde_json::from_value(prepared.image.clone())?;
    old.transition.description.loops.clear();
    old.transition.dependency_order.clear();
    old.transition.limits.loop_slots = 1;
    old.transition.limits.pending_requests = 4;
    old.transition.limits.events = 6;
    old.program.instructions = program(0);
    old.program.jobs_per_quantum = 16;
    old.buffers = 4;
    old.initial_state.memory = vec![
        Value::Ramp(Ramp {
            start: 0,
            target: 0,
            duration: 0,
            elapsed: 0
        });
        4
    ];
    let compiled = continuous::live_design(&c, Mode::Fixed)
        .parameters(&[
            ("LOOP_DEPTH", 1),
            ("REQUEST_DEPTH", 4),
            ("EVENT_DEPTH", 6),
            ("CAPTURE_CHANNELS", 2),
            ("RESIDENT_ACTIONS", 1),
            ("REUSE_VIEWS", 1),
            ("PROGRAM_DEPTH", 16),
            ("STATE_DEPTH", 8),
            ("JOB_DEPTH", 16),
        ])
        .probes(vec![Probe::observe("core.core.timing.frame")])
        .build(&crate::root().join("work/marlin-suite/resident-configuration"))?;
    let mut s = Sim::new(
        compiled.session(None)?,
        &[("clk", 10000, 0), ("audio_clk", c.period_ps, c.phase_ps)],
    );
    let store = Store::new(
        &mut s,
        continuous::geometry(&c),
        Mode::Fixed,
        20261114,
        "store_",
    )?;
    store.memory.borrow_mut().control.stalls = true;
    let mut timing = Transition::new([1, 4, 6]);
    timing.busy_signal = "transition_busy";
    timing.load(&mut s, &serde_json::to_value(&old.transition)?);
    let mut boot = serde_json::to_value(&old)?;
    boot["queue_capacity"] = serde_json::json!(16);
    Program::new([16, 8, 16]).load(&mut s, &boot);
    s.model.set_all(&[
        ("epoch", 1),
        ("job_enable", 1),
        ("run_instruction_count", 6),
        ("run_state_count", 4),
        ("run_quantum", 32),
        ("run_chunk_frames", 32),
        ("run_job_budget", 16),
        ("run_buffers", 4),
        ("run_inputs", 2),
        ("run_outputs", 2),
        ("control_result_ready", 1),
    ]);
    let source = Source::attach(
        &mut s,
        "control_valid",
        "control_ready",
        "control_data",
        vec![],
    );
    let replies = Rc::new(RefCell::new(VecDeque::new()));
    s.agent(Replies(replies.clone()));
    let mut host = Host {
        source,
        replies,
        clock: Frames {
            ticks: c.ticks,
            period_ps: c.period_ps,
        },
        frame: 0,
        batch: 0,
        graph_before_initial,
    };
    let mut owner = Service::new(64, 32)?;
    s.model.set("enable", 1);
    host.clock.settle(&mut s, 5);
    host.ok(
        &mut s,
        vec![
            Command::seed(
                0,
                Ramp {
                    start: 0,
                    target: 1 << 24,
                    duration: 8192,
                    elapsed: 0,
                },
            )
            .transfer(),
        ],
    );
    let mut nonce = host.stage_image(&mut s, 0, 0, false);
    let mut graph = StageId::from_parts(11, 0, 1).unwrap();
    let pair = NodePairStageId::from_parts([21, 0, 1]).unwrap();
    let topo = staging::StageId::from_parts(31, 1).unwrap();
    let target = LoopId::from_parts(0, 7).unwrap();
    let mut spec = [0; 6];
    protocol::put(&mut spec, 0, 1, 1);
    protocol::put(&mut spec, 1, 32, 7);
    protocol::put(&mut spec, 98, 1, 1);
    host.ok(
        &mut s,
        vec![
            stage::graph_ready(graph, Epoch(0)),
            stage::pair_ready(pair, Some(target)),
            stage::topology_ready(topo, spec, true),
        ],
    );
    if graph_before_initial {
        let before = host.admit(
            &mut s,
            &mut owner,
            1,
            32 * 32,
            stage::graph(graph, Epoch(0), Epoch(0), nonce),
        );
        host.until(&mut s, 40 * 32);
        let r = host.record(&mut s, &mut owner, before);
        assert_eq!(resident::tag(&r.payload), resident::GRAPH_ACTIVATED);
        let next = StageId::from_parts(11, 0, 2).unwrap();
        host.ok(
            &mut s,
            vec![
                stage::graph_release(graph),
                stage::graph_ready(next, Epoch(1)),
            ],
        );
        graph = next;
        nonce = host.stage_image(&mut s, 1, 1, false);
    }
    let offset = u64::from(graph_before_initial);
    let initial = host.admit(
        &mut s,
        &mut owner,
        1 + offset,
        64 * 32,
        stage::initial(graph, pair, topo, Epoch(offset), Epoch(0), nonce),
    );
    let sent = host.source.borrow().sent;
    s.model.set("control_result_ready", 0);
    host.until(&mut s, 80 * 32);
    assert_eq!(host.source.borrow().sent, sent);
    assert_eq!(s.model.get("control_valid"), 0);
    assert_eq!(
        s.model.get("core.core.timing.frame"),
        s.model.get("process_sequence") * 32,
        "initial reset advancing musical time"
    );
    s.model.set("control_result_ready", 1);
    let record = host.record(&mut s, &mut owner, initial);
    assert!(record.terminal);
    assert_eq!(record.terminal_frame, 64 * 32);
    assert_eq!(resident::tag(&record.payload), resident::INITIAL_ACTIVATED);
    let graph2 = StageId::from_parts(11, 0, 2 + offset as u32).unwrap();
    host.ok(
        &mut s,
        vec![
            stage::graph_release(graph),
            stage::graph_ready(graph2, Epoch(1 + offset)),
        ],
    );
    let nonce = host.stage_image(
        &mut s,
        if graph_before_initial { 0 } else { 1 },
        1 + offset,
        true,
    );
    let graph_action = host.admit(
        &mut s,
        &mut owner,
        2 + offset,
        128 * 32,
        stage::graph(graph2, Epoch(1 + offset), Epoch(1), nonce),
    );
    let repeat = host.admit(
        &mut s,
        &mut owner,
        3 + offset,
        130 * 32,
        resident::repeat(target, false),
    );
    let query = host.admit(
        &mut s,
        &mut owner,
        4 + offset,
        131 * 32,
        resident::state(target),
    );
    let sent = host.source.borrow().sent;
    s.model.set("control_result_ready", 0);
    host.until(&mut s, 160 * 32);
    assert_eq!(host.source.borrow().sent, sent);
    assert_eq!(s.model.get("control_valid"), 0);
    s.model.set("control_result_ready", 1);
    let r = host.record(&mut s, &mut owner, graph_action);
    assert_eq!(r.terminal_frame, 128 * 32);
    assert_eq!(resident::tag(&r.payload), resident::GRAPH_ACTIVATED);
    assert_eq!(protocol::get(&r.payload, 96, 64), 2 + offset);
    let r = host.record(&mut s, &mut owner, repeat);
    assert_eq!(resident::tag(&r.payload), resident::DONE);
    let r = host.record(&mut s, &mut owner, query);
    let reference = Reference::decode(&r, 1)?;
    assert_eq!(
        (
            reference.frame.0,
            reference.graph_epoch.0,
            reference.topology_epoch.0
        ),
        (131 * 32, 2 + offset, 1)
    );
    let mut reader = Reader::new(reference);
    for r in host.ok(
        &mut s,
        (0..30).map(|i| reference.word(i).unwrap()).collect(),
    ) {
        reader.accept(r.error, r.payload)?;
    }
    let words = reader.finish()?;
    let state = protocol::state(
        &words[..13]
            .iter()
            .flat_map(|w| [*w as u32, (*w >> 32) as u32])
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        0,
    )?;
    assert!(!state.repeat_sync);
    assert_eq!(s.model.get("missed_deadlines"), 0);
    assert_eq!(s.model.get("input_losses"), 0);
    println!(
        "resident configuration: {} exact stereo frames, frame-preserving initial activation, relocated live ramps, separate graph/topology epochs, repeat command and retained historical query, two ARM service pauses",
        host.frame
    );
    Ok(())
}
