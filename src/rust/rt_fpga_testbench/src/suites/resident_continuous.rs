//! The public owner pauses while the actual continuous capture/audio pipeline runs.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    drivers::{
        frames::Frames,
        state::Program,
        store::Store,
        stream::Source,
        transition::{self as abi, Transition},
    },
    memory::Mode,
    model::{Model, Probe},
    reference::Reference,
    sim::{Agent, Sim},
    suites::continuous::{self, Config},
};
use rt_fpga_adapter::{
    audio_actions::{self as a, Identity, Progress, Record, Service},
    audio_control::{self, Completion},
    audio_image::Transfer,
    observations::{Reader, Reference as SnapshotRef, Scope},
    protocol,
    transition_service::wire,
};
use rt_runtime::Frame;
use serde_json::json;
use shoop_loops::{
    LoopId,
    runtime::{Reply, Selection},
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

struct Owner {
    source: Rc<RefCell<Source>>,
    service: Service,
    commands: VecDeque<(u64, u64, Transfer)>,
    expected: VecDeque<(u64, u32, u8)>,
    reader: Option<(SnapshotRef, Reader)>,
    snapshots: Vec<(SnapshotRef, [u64; 30])>,
    batch: u64,
    ids: Vec<Identity>,
    records: Vec<Record>,
    paused_at: Option<u64>,
    pause_transfers: u64,
    resuming: bool,
    read_index: usize,
}
impl Owner {
    fn enqueue(&mut self, t: Transfer) {
        self.enqueue_batch(vec![t]);
    }
    fn enqueue_batch(&mut self, operations: Vec<Transfer>) {
        self.batch += 1;
        let mut source = self.source.borrow_mut();
        source.queue.push_back(Bits(
            audio_control::header(0, self.batch, operations.len() as u32, 64)
                .unwrap()
                .to_vec(),
        ));
        for (ordinal, t) in operations.into_iter().enumerate() {
            self.expected
                .push_back((self.batch, ordinal as u32, t.opcode));
            source.queue.push_back(Bits(t.packet(0).to_vec()));
        }
    }
    fn next(&mut self, frame: u64) {
        if let Some((seq, at, t)) = self.commands.pop_front() {
            let t = self.service.schedule(seq, at, t).unwrap();
            self.enqueue(t);
        } else {
            self.paused_at = Some(frame);
            self.pause_transfers = self.source.borrow().sent;
        }
    }
    fn read(&mut self) {
        if self.read_index < self.ids.len() {
            let t = self
                .service
                .inspect(self.ids[self.read_index], false)
                .unwrap();
            self.enqueue(t);
        }
    }
}
impl Agent for Owner {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set("control_result_ready", u64::from(!self.expected.is_empty()));
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) || m.get("reset") != 0 {
            return;
        }
        if let Some(at) = self.paused_at {
            if !self.resuming {
                assert_eq!(self.source.borrow().sent, self.pause_transfers);
                assert_eq!(m.get("control_valid"), 0);
                assert_eq!(m.get("control_result_ready"), 0);
                if m.get("process_sequence") >= at + 1040 {
                    self.resuming = true;
                    self.read();
                }
            }
        }
        if m.get("control_result_valid") == 0 || m.get("control_result_ready") == 0 {
            return;
        }
        let reply = Completion::decode(&m.bits("control_result_data").array()).unwrap();
        let (batch, ordinal, opcode) = self
            .expected
            .pop_front()
            .expect("unexpected public transport reply");
        assert_eq!(
            (
                reply.batch,
                reply.operation,
                reply.configuration,
                reply.opcode,
                reply.epoch
            ),
            (batch, ordinal, true, opcode, 1)
        );
        if self.reader.is_some() {
            assert_eq!(opcode, 48);
            if self
                .reader
                .as_mut()
                .unwrap()
                .1
                .accept(reply.error, reply.payload)
                .unwrap()
            {
                let (reference, reader) = self.reader.take().unwrap();
                self.snapshots.push((reference, reader.finish().unwrap()));
                self.read_index += 1;
                self.read();
            }
            return;
        }
        match self
            .service
            .finish(reply.error, reply.payload, reply.quantum * 32)
            .unwrap()
        {
            Progress::Transfer(t) => self.enqueue(t),
            Progress::Admitted { identity, .. } => {
                self.ids.push(identity);
                self.next(m.get("process_sequence"));
            }
            Progress::Record { record, .. } => {
                assert!(record.terminal && record.state);
                let snapshot = if matches!(record.opcode, a::QUERY | a::EXPORT) {
                    Some(SnapshotRef::decode(&record, 1).unwrap())
                } else {
                    None
                };
                self.records.push(record);
                if let Some(reference) = snapshot {
                    self.reader = Some((reference, Reader::new(reference)));
                    self.enqueue_batch((0..30).map(|i| reference.word(i).unwrap()).collect());
                } else {
                    self.read_index += 1;
                    self.read();
                }
            }
            _ => panic!("public operation refused"),
        }
    }
}
struct Cuts {
    states: Rc<RefCell<Vec<Bits>>>,
    events: Rc<RefCell<Vec<serde_json::Value>>>,
}
impl Agent for Cuts {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("cut_snapshot_valid") != 0 {
            self.states.borrow_mut().push(m.bits("cut_snapshot_data"));
        }
        if e.rises(0) && m.get("core.core.timing.ew_en") != 0 {
            self.events
                .borrow_mut()
                .push(abi::event(&m.bits("core.core.timing.ew_data")));
        }
    }
}
pub fn delivery(mode: Mode) -> Result<()> {
    let id = |slot| json!({"slot":slot,"generation":1});
    let changes = [
        (256, "record", None),
        (258, "play", None),
        (288, "record", None),
        (290, "play", None),
        (320, "seek", None),
        (352, "seek", Some(1000)),
        (384, "stop", None),
        (416, "play", None),
        (448, "seek", Some(1)),
    ];
    let observations = [
        (257, a::QUERY, None),
        (259, a::EXPORT, Some(Selection::Latest)),
        (289, a::QUERY, None),
        (291, a::EXPORT, Some(Selection::Playback)),
        (353, a::QUERY, None),
    ];
    let operation = |kind| {
        if kind == "play" || kind == "seek" {
            json!({"kind":kind,"position":0,"retrigger":true})
        } else {
            json!({"kind":kind})
        }
    };
    let timing = |skip: Option<u64>| {
        skip.map(|skip| json!({"kind":"after_ticks","source":id(0),"skip":skip}))
            .unwrap_or(json!({"kind":"at"}))
    };
    let steps:Vec<_>=(0..1312).map(|q|json!({"inputs":(0..2).map(|ch|(0..32).map(|i|(ch+1)*100003+q*1013+i*37).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "actions":changes.iter().enumerate().filter(|(_, (at,_,_))|*at==q).map(|(n,(at,kind,skip))|json!({"kind":"transition","name":format!("a{n}"),"target":id(0),"at":at*32,"timing":timing(*skip),"operation":operation(*kind)})).collect::<Vec<_>>(),"collect_completed":true})).collect();
    let scenario = json!({"format_version":1,"name":"continuous-native-public","quantum":32,"chunk_frames":32,"input_ports":2,"output_ports":2,"request_capacity":4,
        "loops":[{"id":id(0),"channels":2,"sync_source":id(0),"repeat_sync":true,"content":null}],
        "nodes":[{"id":id(0),"operation":{"kind":"playback","target":id(0)}},{"id":id(1),"operation":{"kind":"capture","target":id(0)}}],
        "connections":(0..2).flat_map(|port|[json!({"source":{"node":null,"port":port},"destination":{"node":id(1),"port":port}}),json!({"source":{"node":id(0),"port":port},"destination":{"node":null,"port":port}})]).collect::<Vec<_>>(),"media":[],"steps":steps});
    let reference = Reference::new(&scenario)?;
    let c = Config {
        q: 32,
        cf: 32,
        p: 32,
        channels: 2,
        slots: 1024,
        bc: 32,
        control_depth: 64,
        ticks: 512,
        monitoring: false,
        routing: "identity".into(),
        ..Config::default()
    };
    let design = continuous::live_design(&c, mode)
        .parameters(&[
            ("LOOP_DEPTH", 1),
            ("REQUEST_DEPTH", 4),
            ("EVENT_DEPTH", 6),
            ("CAPTURE_CHANNELS", 2),
            ("RESIDENT_ACTIONS", 1),
            ("REUSE_VIEWS", 1),
            ("PROGRAM_DEPTH", 64),
            ("STATE_DEPTH", 32),
            ("JOB_DEPTH", 64),
        ])
        .probes(vec![
            Probe::observe("core.core.timing.frame"),
            Probe::observe("core.core.timing.ew_en"),
            Probe::observe("core.core.timing.ew_data"),
        ]);
    let compiled = design.build(&crate::root().join("work/marlin-suite/resident-continuous"))?;
    let mut s = Sim::new(
        compiled.session(None)?,
        &[("clk", 10000, 0), ("audio_clk", c.period_ps, c.phase_ps)],
    );
    s.max_ps = 2_000_000_000_000;
    let store = Store::new(&mut s, continuous::geometry(&c), mode, 20261114, "store_")?;
    store.memory.borrow_mut().control.stalls = mode != Mode::Instant;
    let mut d = Transition::new([1, 4, 6]);
    d.busy_signal = "transition_busy";
    d.load(&mut s, &reference.image["transition"]);
    let mut program = Program::new([64, 32, 64]);
    let mut image = reference.image.clone();
    image["queue_capacity"] = json!(64);
    program.load(&mut s, &image);
    s.model.set_all(&[
        ("epoch", 1),
        ("job_enable", 1),
        (
            "run_instruction_count",
            image["program"]["instructions"].as_array().unwrap().len() as u64,
        ),
        (
            "run_state_count",
            image["initial_state"]["memory"].as_array().unwrap().len() as u64,
        ),
        ("run_quantum", 32),
        ("run_chunk_frames", 32),
        ("run_job_budget", 64),
        ("run_buffers", image["buffers"].as_u64().unwrap()),
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
    let commands = changes
        .iter()
        .enumerate()
        .map(|(n, (at, kind, skip))| {
            let mut command = super::transition::command(
                0,
                n as u64 + 1,
                operation(*kind),
                at * 32,
                skip.map(|_| 0),
                skip.unwrap_or(0),
            );
            command["target"]["generation"] = json!(1);
            if skip.is_some() {
                command["timing"]["source"]["generation"] = json!(1);
            }
            (
                n as u64 + 1,
                at * 32,
                Transfer {
                    opcode: a::TRANSITION,
                    state: false,
                    address: 0,
                    words: wire::install(&abi::command(&command).array(), 0),
                },
            )
        })
        .chain(
            observations
                .iter()
                .enumerate()
                .map(|(n, (at, opcode, selection))| {
                    let mut t = Transfer {
                        opcode: *opcode,
                        state: false,
                        address: 0,
                        words: [0; 20],
                    };
                    protocol::put(&mut t.words, 0, 64, 1_u64 << 32);
                    protocol::put(
                        &mut t.words,
                        64,
                        2,
                        u64::from(*selection == Some(Selection::Latest)),
                    );
                    (10 + n as u64, at * 32, t)
                }),
        )
        .collect();
    let owner = Rc::new(RefCell::new(Owner {
        source,
        service: Service::new(64, 32)?,
        commands,
        expected: VecDeque::new(),
        reader: None,
        snapshots: vec![],
        batch: 0,
        ids: vec![],
        records: vec![],
        paused_at: None,
        pause_transfers: 0,
        resuming: false,
        read_index: 0,
    }));
    owner.borrow_mut().next(0);
    s.agent(owner.clone());
    let cuts = Rc::new(RefCell::new(Vec::new()));
    let events = Rc::new(RefCell::new(Vec::new()));
    s.agent(Cuts {
        states: cuts.clone(),
        events: events.clone(),
    });
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
    s.model.set("enable", 1);
    let clock = Frames {
        ticks: c.ticks,
        period_ps: c.period_ps,
    };
    clock.settle(&mut s, 5);
    let mut actual = vec![];
    for frame in 0..1312 * 32u64 {
        let q = frame / 32;
        let i = frame % 32;
        let input = Bits(
            (0..2)
                .map(|ch| {
                    scenario["steps"][q as usize]["inputs"][ch][i as usize]
                        .as_i64()
                        .unwrap() as u32
                })
                .collect(),
        );
        let (_, output) = clock.pulse(&mut s, Some(&input));
        for channel in 0..2 {
            let expected = if frame < 64 {
                0
            } else {
                let f = frame - 64;
                reference.fixed["quanta"][(f / 32) as usize]["outputs"]["channels"][channel]
                    [(f % 32) as usize]
                    .as_i64()
                    .unwrap() as i32
            };
            assert_eq!(
                output.get(channel * 32, 32) as u32 as i32,
                expected,
                "frame {frame}, channel {channel}, {mode:?}"
            );
            let floating = if frame < 64 {
                0.
            } else {
                let f = frame - 64;
                reference.floating["quanta"][(f / 32) as usize]["outputs"]["channels"][channel]
                    [(f % 32) as usize]
                    .as_f64()
                    .unwrap()
            };
            assert!(
                (f64::from(expected) / 16777216. - floating).abs() <= 2. / 16777216.,
                "independent float bound at {frame}"
            );
        }
        actual.push(output.array::<2>());
        assert_eq!(
            s.model.get("recovery_requested"),
            0,
            "fault at frame {frame}, {mode:?}: {}",
            s.model.get("recovery_reason")
        );
    }
    s.until(100000, "final resident quantum", |m| {
        m.get("processed_quanta") == 1312
    });
    assert_eq!(cuts.borrow().len(), 2624);
    let expected_events: Vec<_> = reference.fixed["quanta"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|q| q["events"].as_array().unwrap().iter().cloned())
        .collect();
    assert_eq!(
        *events.borrow(),
        expected_events,
        "complete ordered events, {mode:?}"
    );
    assert_eq!(
        s.model.get("missed_deadlines")
            | s.model.get("discarded_quanta")
            | s.model.get("input_losses"),
        0
    );
    let owner = owner.borrow();
    assert!(owner.resuming);
    assert_eq!(owner.records.len(), changes.len() + observations.len());
    assert_eq!(owner.snapshots.len(), observations.len());
    let outcomes: Vec<_> = reference.fixed["quanta"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|q| q["outcomes"].as_array().unwrap().iter())
        .collect();
    for (index, (record, (at, kind, skip))) in owner
        .records
        .iter()
        .take(changes.len())
        .zip(changes)
        .enumerate()
    {
        assert_eq!(record.effective_frame, at * 32);
        assert_eq!(record.error, 0);
        assert_eq!(protocol::get(&record.payload, 504, 8), 0);
        let mut command = super::transition::command(
            0,
            index as u64 + 1,
            operation(kind),
            at * 32,
            skip.map(|_| 0),
            skip.unwrap_or(0),
        );
        command["target"]["generation"] = json!(1);
        if skip.is_some() {
            command["timing"]["source"]["generation"] = json!(1);
        }
        let payload = Bits::from(record.payload);
        let compact = std::array::from_fn(|i| payload.get(64 + i * 32, 32) as u32);
        let raw = wire::result(
            &compact,
            Some(&abi::command(&command).array()),
            payload.get(32, 32) as u32,
        )?;
        let request = abi::request(&Bits::from(raw), payload.get(0, 32));
        let outcome = &request["outcome"];
        let name = format!("a{index}");
        let expected = outcomes.iter().find(|r| r["name"] == name).unwrap();
        let actual = json!({"name":name,"requested":at*32,"delivered":request["effective"],"terminal":outcome["frame"],"outcome":if outcome["kind"]=="failed" {format!("failed:{}",outcome["reason"].as_str().unwrap())} else {outcome["kind"].as_str().unwrap().to_owned()}});
        assert_eq!(actual, **expected, "retained terminal {index}, {mode:?}");
        assert_eq!(record.terminal_frame, outcome["frame"].as_u64().unwrap());
    }
    let scope = Scope::new(2, Frame(0))?;
    let target = LoopId::from_parts(0, 1).unwrap();
    for ((snapshot, raw), (at, _, selection)) in owner.snapshots.iter().zip(observations) {
        assert_eq!(snapshot.frame.0, at * 32);
        let mut expected = reference.fixed["quanta"][at as usize]["states"][0].clone();
        if expected["mode"] == "playing" {
            expected["position"] = json!(expected["position"].as_u64().unwrap() - 32);
        }
        if expected["mode"] == "recording" {
            expected["capture_frames"] = json!(expected["capture_frames"].as_u64().unwrap() - 32);
        }
        let words: [u32; 26] = std::array::from_fn(|i| {
            if i % 2 == 0 {
                raw[i / 2] as u32
            } else {
                (raw[i / 2] >> 32) as u32
            }
        });
        let mut state = abi::state(&Bits::from(words), 0);
        state.as_object_mut().unwrap().remove("revision_watermark");
        assert_eq!(state, expected, "historical pre-audio state {at}, {mode:?}");
        let cut = match scope.decode(*snapshot, raw, target, selection)? {
            Reply::State(cut) => cut,
            Reply::Export { cut, media, .. } => {
                let source = media.view.audio.unwrap();
                let start = if at == 259 { 256 } else { 288 };
                assert_eq!(
                    source.snapshot.capture.unwrap().start_frame,
                    Frame(start * 32)
                );
                assert_eq!(source.snapshot.cut_frame, Frame((start + 2) * 32));
                assert_eq!(media.view.description.frames, 64);
                cut
            }
            _ => panic!("snapshot reply kind changed"),
        };
        assert_eq!(cut.frame, Frame(at * 32));
        assert_eq!((cut.gain, cut.balance), (1., 0.));
    }
    let mut watermark = 0;
    for (q, pair) in cuts.borrow().chunks_exact(2).enumerate() {
        let mut state = abi::state(&Bits(pair[1].0[..26].to_vec()), 0);
        state.as_object_mut().unwrap().remove("revision_watermark");
        let expected = &reference.fixed["quanta"][q]["states"][0];
        assert_eq!(&state, expected, "full state quantum {q}, {mode:?}");
        watermark = watermark
            .max(expected["content_revision"].as_u64().unwrap())
            .max(expected["capture_revision"].as_u64().unwrap());
        assert_eq!(
            pair[1].get(448, 64),
            watermark,
            "revision watermark {q}, {mode:?}"
        );
    }
    let out = crate::artifact_directory("resident-continuous");
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join(format!("{mode:?}.json")),
        serde_json::to_vec(
            &json!({"audio":actual,"pause_at":owner.paused_at,"pause_quanta":1040,"snapshot_words":owner.snapshots.iter().map(|(_,r)|r).collect::<Vec<_>>(),"records":owner.records.iter().map(|r|json!({"sequence":r.sequence,"effective":r.effective_frame,"terminal":r.terminal_frame,"payload":r.payload})).collect::<Vec<_>>()}),
        )?,
    )?;
    println!(
        "resident continuous/{mode:?}: 1,040-quanta ARM pause, two stereo takes, future AT/self-sync/tick/STOP, retained public results and exact audio/state"
    );
    Ok(())
}
