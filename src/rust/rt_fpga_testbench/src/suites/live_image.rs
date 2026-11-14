//! Full-size staged updates, retained transport credits and real continuous I/O.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    drivers::{
        frames::Frames,
        state::Program,
        store::Store,
        stream::{Source, control_packet},
        transition::Transition,
    },
    memory::Mode,
    model::{Model, Probe},
    sim::{Agent, Sim},
    suites::continuous::{self, Config},
};
use rt_audio_math::{Q824, Sample};
use rt_fpga_adapter::{
    audio_actions::{Identity, Progress, Record, Service},
    audio_control::{self, Completion},
    audio_image::{self, Transfer},
    image::{Image, Instruction, Ramp, Value},
};
use serde_json::json;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

enum Phase {
    Begin,
    Uploads,
    Actions(usize),
    Filling,
    Partial,
    Paused,
    Drain(usize),
    FinishPartial(usize),
    Records(usize),
    Complete,
}
struct Publication {
    last: u64,
    eligible: Rc<RefCell<BTreeMap<u64, u64>>>,
}
impl Agent for Publication {
    fn observe(&mut self, m: &Model<'_>, e: Edges) {
        let current = m.get("io.tx_request_sync");
        if current != self.last {
            self.last = current;
            if let Some(quantum) = m.get("process_sequence").checked_sub(1) {
                self.eligible.borrow_mut().insert(quantum, e.ps);
            }
        }
    }
}
struct Control {
    source: Rc<RefCell<Source>>,
    old: Image,
    new: Image,
    uploads: VecDeque<Transfer>,
    phase: Phase,
    expected: VecDeque<(u64, u32, u8, u8)>,
    batch: u64,
    owner: Service,
    ids: Vec<Identity>,
    paused_at: Option<u64>,
    pause_transfers: u64,
    held: Option<Bits>,
    outcomes: Vec<Record>,
}
impl Control {
    fn enqueue(&mut self, operations: Vec<Transfer>) {
        self.batch += 1;
        let mut queue = self.source.borrow_mut();
        queue.queue.push_back(Bits(
            audio_control::header(0, self.batch, operations.len() as u32, 128)
                .unwrap()
                .to_vec(),
        ));
        for (index, operation) in operations.into_iter().enumerate() {
            self.expected
                .push_back((self.batch, index as u32, 1, operation.opcode));
            queue.queue.push_back(Bits(operation.packet(0).to_vec()));
        }
    }
    fn upload_batch(&mut self) {
        let count = self.uploads.len().min(127);
        let batch = self.uploads.drain(..count).collect();
        self.enqueue(batch);
    }
    fn begin_action(&mut self, index: usize) {
        self.phase = Phase::Actions(index);
        let (at, operation) = if index == 0 {
            (64 * 32, Transfer::activate(1))
        } else {
            (
                80 * 32,
                Transfer::set_ramp(index as u16 - 1, -(1 << 24), 128, 1),
            )
        };
        let transfer = self
            .owner
            .schedule(index as u64 + 1, at, operation)
            .unwrap();
        self.enqueue(vec![transfer]);
    }
    fn fill_replies(&mut self) {
        self.phase = Phase::Filling;
        for count in [127, 1] {
            self.batch += 1;
            let mut source = self.source.borrow_mut();
            source.queue.push_back(Bits(
                audio_control::header(0, self.batch, count, 128)
                    .unwrap()
                    .to_vec(),
            ));
            for ordinal in 0..count {
                source
                    .queue
                    .push_back(control_packet(0, 0, &Bits::default(), 0, 0));
                self.expected.push_back((self.batch, ordinal, 0, 0));
            }
        }
    }
    fn read_action(&mut self, index: usize) {
        self.phase = Phase::Records(index);
        let transfer = self.owner.inspect(self.ids[index], false).unwrap();
        self.enqueue(vec![transfer]);
    }
}
impl Agent for Control {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set(
                "control_result_ready",
                u64::from(!matches!(
                    self.phase,
                    Phase::Filling | Phase::Partial | Phase::Paused
                )),
            );
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) || m.get("enable") == 0 {
            return;
        }
        if matches!(self.phase, Phase::Filling)
            && m.get("controls.result_count") == 128
            && self.source.borrow().queue.is_empty()
        {
            self.batch += 1;
            let mut source = self.source.borrow_mut();
            source.queue.push_back(Bits(
                audio_control::header(0, self.batch, 2, 128)
                    .unwrap()
                    .to_vec(),
            ));
            source
                .queue
                .push_back(Bits(Transfer::status().packet(0).to_vec()));
            for ordinal in 0..2 {
                self.expected.push_back((self.batch, ordinal, 1, 7));
            }
            self.phase = Phase::Partial;
        }
        if matches!(self.phase, Phase::Partial)
            && m.get("controls.count") == 2
            && self.source.borrow().queue.is_empty()
        {
            self.paused_at = Some(m.get("process_sequence"));
            self.pause_transfers = self.source.borrow().sent;
            self.held = Some(m.bits("control_result_data"));
            self.phase = Phase::Paused;
        }
        if matches!(self.phase, Phase::Paused) {
            assert_eq!(m.get("control_valid"), 0);
            assert_eq!(m.get("control_result_ready"), 0);
            assert_eq!(m.get("controls.result_count"), 128);
            assert_eq!(m.get("controls.count"), 2);
            assert_eq!(m.bits("control_result_data"), *self.held.as_ref().unwrap());
            assert_eq!(self.source.borrow().sent, self.pause_transfers);
            return;
        }
        if m.get("control_result_valid") == 0 || m.get("control_result_ready") == 0 {
            return;
        }
        let words: [u32; 34] = m.bits("control_result_data").0.try_into().unwrap();
        let reply = Completion::decode(&words).unwrap();
        let (batch, ordinal, kind, opcode) = self
            .expected
            .pop_front()
            .expect("unexpected transport result");
        assert_eq!(
            (
                reply.batch,
                reply.operation,
                u8::from(reply.configuration),
                reply.opcode
            ),
            (batch, ordinal, kind, opcode)
        );
        assert_eq!(reply.epoch, 1);
        match self.phase {
            Phase::Begin => {
                assert_eq!(reply.error, 0);
                assert_eq!(u64::from(reply.payload[0]), 1);
                let copies: Vec<_> = (0..512).map(|i| (i, i)).collect();
                self.uploads = audio_image::prepare(&self.old, &self.new, &copies, 1)
                    .unwrap()
                    .uploads
                    .into();
                self.phase = Phase::Uploads;
                self.upload_batch();
            }
            Phase::Uploads => {
                assert_eq!(reply.error, 0);
                if self.expected.is_empty() {
                    if self.uploads.is_empty() {
                        self.begin_action(0);
                    } else {
                        self.upload_batch();
                    }
                }
            }
            Phase::Actions(index) => match self
                .owner
                .finish(reply.error, reply.payload, reply.quantum * 32)
                .unwrap()
            {
                Progress::Transfer(transfer) => self.enqueue(vec![transfer]),
                Progress::Admitted {
                    identity,
                    effective_frame,
                } => {
                    assert_eq!(effective_frame, if index == 0 { 64 * 32 } else { 80 * 32 });
                    self.ids.push(identity);
                    if index < 2 {
                        self.begin_action(index + 1);
                    } else {
                        self.fill_replies();
                    }
                }
                _ => panic!("resident configuration admission failed"),
            },
            Phase::Drain(n) => {
                assert_eq!(reply.error, 0);
                if n == 127 {
                    self.source
                        .borrow_mut()
                        .queue
                        .push_back(Bits(Transfer::status().packet(0).to_vec()));
                    self.phase = Phase::FinishPartial(0);
                } else {
                    self.phase = Phase::Drain(n + 1);
                }
            }
            Phase::FinishPartial(n) => {
                assert_eq!(reply.error, 0);
                if n == 1 {
                    self.read_action(0);
                } else {
                    self.phase = Phase::FinishPartial(n + 1);
                }
            }
            Phase::Records(index) => match self
                .owner
                .finish(reply.error, reply.payload, reply.quantum * 32)
                .unwrap()
            {
                Progress::Record { record, .. } => {
                    assert!(record.published && record.terminal);
                    assert_eq!(record.error, 0);
                    assert_eq!(
                        record.terminal_frame,
                        if index == 0 { 64 * 32 } else { 80 * 32 }
                    );
                    self.outcomes.push(record);
                    if index < 2 {
                        self.read_action(index + 1);
                    } else {
                        self.phase = Phase::Complete;
                    }
                }
                _ => panic!("historical action result missing"),
            },
            _ => panic!("reply consumed during retained-credit pause"),
        }
    }
}
pub fn continuous_large_image() -> Result<()> {
    let c = Config {
        q: 32,
        cf: 32,
        p: 32,
        channels: 2,
        monitoring: false,
        routing: "identity".into(),
        ticks: 512,
        control_depth: 128,
        ..Config::default()
    };
    let mut scenario = crate::prepared::scenario(32, 32, 2, false, "identity");
    for step in scenario["steps"].as_array_mut().unwrap() {
        step["actions"] = json!([]);
    }
    let prepared = crate::prepared::prepare(scenario, c.bc, c.slots)?;
    let mut old: Image = serde_json::from_value(prepared.image.clone())?;
    let program = vec![
        Instruction::ProcessInput {
            port: 0,
            destination: 0,
        },
        Instruction::ProcessInput {
            port: 1,
            destination: 1,
        },
        Instruction::ProcessGain {
            ramp: 0,
            source: 0,
            destination: 2,
        },
        Instruction::ProcessGain {
            ramp: 1,
            source: 1,
            destination: 3,
        },
        Instruction::ProcessOutput { source: 2, port: 0 },
        Instruction::ProcessOutput { source: 3, port: 1 },
    ];
    old.program.instructions = program.clone();
    old.program.jobs_per_quantum = 512;
    old.buffers = 4;
    old.initial_state.memory = vec![Value::Word(0); 512];
    old.initial_state.memory[0] = Value::Ramp(Ramp {
        start: 0,
        target: 1 << 24,
        duration: 8192,
        elapsed: 0,
    });
    old.initial_state.memory[1] = old.initial_state.memory[0];
    let mut new = old.clone();
    new.program.instructions = vec![
        Instruction::ProcessLogicalCapture {
            loop_id: old.transition.description.loops[0].id,
            view: 2,
            source: 0
        };
        506
    ];
    new.program.instructions.extend(program);
    new.initial_state.memory[0] = Value::Ramp(Ramp {
        start: 0,
        target: 0,
        duration: 0,
        elapsed: 0,
    });
    new.initial_state.memory[1] = new.initial_state.memory[0];
    let mut design = continuous::design(&c, Mode::Fixed, false, &prepared).parameters(&[
        ("PROGRAM_DEPTH", 512),
        ("STATE_DEPTH", 512),
        ("JOB_DEPTH", 512),
    ]);
    design.probes.extend([
        Probe::observe("controls.result_count"),
        Probe::observe("controls.count"),
    ]);
    let compiled = design.build(&crate::root().join("work/marlin-suite/live-image"))?;
    let mut s = Sim::new(
        compiled.session(None)?,
        &[("clk", 10000, 0), ("audio_clk", c.period_ps, c.phase_ps)],
    );
    s.max_ps = 1_000_000_000_000;
    let mut store = Store::new(
        &mut s,
        continuous::geometry(&c),
        Mode::Fixed,
        20261114,
        "store_",
    )?;
    store.memory.borrow_mut().control.stalls = true;
    let mut transition = Transition::new([16, 128, 256]);
    transition.busy_signal = "transition_busy";
    transition.load(&mut s, &serde_json::to_value(&old.transition)?);
    let mut program = Program::new([512, 512, 512]);
    let mut case = serde_json::to_value(&old)?;
    case["queue_capacity"] = json!(512);
    program.load(&mut s, &case);
    store.load(
        &mut s,
        &crate::images::reblock(&case["media"], c.bc, c.slots),
    );
    s.model.set_all(&[
        ("epoch", 1),
        ("job_enable", 1),
        ("run_instruction_count", 6),
        ("run_state_count", 512),
        ("run_quantum", 32),
        ("run_chunk_frames", 32),
        ("run_job_budget", 512),
        ("run_buffers", 4),
        ("run_inputs", 2),
        ("run_outputs", 2),
    ]);
    let source = Source::attach(
        &mut s,
        "control_valid",
        "control_ready",
        "control_data",
        vec![],
    );
    let dimensions = audio_image::Dimensions {
        instructions: 512,
        states: 512,
        jobs: 512,
        buffers: 4,
        copies: 512,
    };
    let control = Rc::new(RefCell::new(Control {
        source,
        old,
        new,
        uploads: VecDeque::new(),
        phase: Phase::Begin,
        expected: VecDeque::new(),
        batch: 0,
        owner: Service::new(128, 32)?,
        ids: vec![],
        paused_at: None,
        pause_transfers: 0,
        held: None,
        outcomes: vec![],
    }));
    control
        .borrow_mut()
        .enqueue(vec![Transfer::begin(0, dimensions)]);
    s.agent(control.clone());
    s.agent(crate::protocol::ReadyValid::new(
        "control_valid",
        "control_ready",
        &["control_data"],
    ));
    s.agent(crate::protocol::ReadyValid::new(
        "control_result_valid",
        "control_result_ready",
        &["control_result_data"],
    ));
    let clock = Frames {
        ticks: c.ticks,
        period_ps: c.period_ps,
    };
    s.model.set("enable", 1);
    clock.settle(&mut s, 5);
    let initial = rt_audio_math::Ramp {
        start: Q824::ZERO,
        target: Q824::ONE,
        duration: 8192,
        elapsed: 0,
    };
    let changed = rt_audio_math::Ramp {
        start: initial.at(80 * 32),
        target: Q824(-(1 << 24)),
        duration: 128,
        elapsed: 0,
    };
    let mut resume = false;
    let mut samples = Vec::new();
    let mut publication = Vec::new();
    let eligible = Rc::new(RefCell::new(BTreeMap::new()));
    s.agent(Publication {
        last: 0,
        eligible: eligible.clone(),
    });
    let mut inputs = BTreeMap::new();
    let mut float_error = 0f64;
    let mut frame = 0;
    loop {
        if !resume
            && control
                .borrow()
                .paused_at
                .is_some_and(|at| s.model.get("process_sequence") >= at + 1040)
        {
            control.borrow_mut().phase = Phase::Drain(0);
            resume = true;
        }
        let input = Bits(vec![
            (100000 + frame * 37) as u32,
            (200000 + frame * 37) as u32,
        ]);
        let (time, actual) = clock.pulse(&mut s, Some(&input));
        if frame % 32 == 31 {
            inputs.insert(frame / 32, time);
        }
        let expected: Vec<i32> = (0..2)
            .map(|channel| {
                if frame < 64 {
                    0
                } else {
                    let at = frame - 64;
                    let gain = if at < 80 * 32 {
                        initial.at(at as u32)
                    } else {
                        changed.at((at - 80 * 32) as u32)
                    };
                    Q824((100000 * (channel + 1) + at * 37) as i32)
                        .multiply(gain)
                        .0
                }
            })
            .collect();
        let actual: Vec<i32> = (0..2).map(|i| actual.get(i * 32, 32) as i32).collect();
        assert_eq!(actual, expected, "live staged image at frame {frame}");
        for (channel, &sample) in actual.iter().enumerate() {
            let reference = if frame < 64 {
                0.
            } else {
                let at = frame - 64;
                let gain = if at < 2560 {
                    at as f64 / 8192.
                } else {
                    let progress = (at - 2560).min(128) as f64 / 128.;
                    0.3125 + (-1. - 0.3125) * progress
                };
                (100000. * (channel + 1) as f64 + at as f64 * 37.) / (1 << 24) as f64 * gain
            };
            let error = (f64::from(sample) / (1 << 24) as f64 - reference).abs();
            float_error = float_error.max(error);
            assert!(
                error <= 2. / (1 << 24) as f64,
                "independent floating bound at frame {frame}"
            );
        }
        samples.push(actual);
        assert_eq!(
            s.model.get("recovery_requested"),
            0,
            "live update fault reason {} at frame {frame}",
            s.model.get("recovery_reason")
        );
        if frame >= 64 && (frame - 64) % 32 == 0 {
            let quantum = (frame - 64) / 32;
            let input = inputs[&quantum];
            let eligible = eligible.borrow()[&quantum];
            let cutoff = time - 2 * c.period_ps;
            assert!(cutoff >= eligible, "late output publication");
            let margin = cutoff - eligible;
            assert!(
                margin >= (cutoff - input) / 10,
                "less than10% publication margin for quantum {quantum}"
            );
            publication.push(json!({"quantum":quantum,"last_input_ps":input,"eligible_ps":eligible,"cutoff_ps":cutoff,"margin_ps":margin}));
        }
        frame += 1;
        if matches!(control.borrow().phase, Phase::Complete) {
            break;
        }
        assert!(frame < 1200 * 32, "live update service failed to finish");
    }
    let control = control.borrow();
    let paused = control.paused_at.unwrap();
    assert!(paused < 64, "publication missed its required lead time");
    assert_eq!(control.outcomes.len(), 3);
    assert_eq!(s.model.get("missed_deadlines"), 0);
    assert_eq!(s.model.get("discarded_quanta"), 0);
    let path = crate::root().join("work/fpga-completion/live-image-continuous.json");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(
            &json!({"configuration":c,"paused_at_quantum":paused,"unserviced_quanta":1040,
        "retained_transport_results":128,"partial_ingress_operations":1,"samples":samples,"publication":publication,
        "processed":s.model.get("processed_quanta"),"missed":s.model.get("missed_deadlines"),"discarded":s.model.get("discarded_quanta"),
        "activation_frame":control.outcomes[0].terminal_frame,"parameter_frames":[control.outcomes[1].terminal_frame,control.outcomes[2].terminal_frame],
        "max_float_absolute_error":float_error,"axi":store.memory.borrow_mut().report(true)}),
        )?,
    )?;
    Ok(())
}
