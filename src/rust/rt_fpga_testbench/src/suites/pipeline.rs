//! Ordered audio/media jobs, retained ranges, publication and fault boundaries.
use crate::{
    Result,
    clock::Edges,
    codec,
    drivers::{pipeline::Pipeline, store::Geometry},
    memory::Mode,
    model::{Design, Model, Probe},
    random::Random,
    sim::{Agent, Sim},
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
const ONE: i32 = 1 << 24;
fn word(v: u64) -> Value {
    json!({"kind":"word","value":v})
}
fn product(a: i32) -> i32 {
    (i64::from(a) / 2) as i32
}
pub fn design(c: [u64; 7], g: &Geometry, width: u64, mode: Mode, response_cycles: u64) -> Design {
    let root = crate::root();
    let mut design = super::media::design(g, width, mode);
    design.top = if mode == Mode::Legacy {
        "media_audio_pipeline"
    } else {
        "media_audio_pipeline_axi"
    }
    .into();
    design.sources.extend(super::audio::design(c).sources);
    design.sources.sort();
    design.sources.dedup();
    design.sources.extend([
        root.join("fpga/media_store/rtl/memory_delay.sv"),
        root.join("fpga/media_store/rtl/media_audio_pipeline.sv"),
    ]);
    if mode != Mode::Legacy {
        design
            .sources
            .push(root.join("fpga/axi_memory/rtl/media_audio_pipeline_axi.sv"));
    }
    for (name, value) in [
        "PROGRAM_DEPTH",
        "STATE_DEPTH",
        "JOB_DEPTH",
        "MAX_FRAMES",
        "BUFFERS",
        "INPUT_PORTS",
        "OUTPUT_PORTS",
    ]
    .into_iter()
    .zip(c)
    .chain([("MEMORY_RESPONSE_CYCLES", response_cycles)])
    {
        design.parameters.insert(name.into(), value.to_string());
    }
    let prefix = if mode == Mode::Legacy {
        "pipeline.sample_engine"
    } else {
        "core.pipeline.sample_engine"
    };
    design.probes = [
        "playback_write",
        "scratch_ram_write_address",
        "media_read_ready",
        "scratch_write",
        "streaming",
        "stream_store_valid",
        "media_fault",
    ]
    .iter()
    .map(|n| Probe::observe(&format!("{prefix}.{n}")))
    .collect();
    design
}
fn core(d: &Pipeline, n: &str) -> String {
    format!(
        "{}.{n}",
        if d.store.mode == Mode::Legacy {
            "pipeline.sample_engine"
        } else {
            "core.pipeline.sample_engine"
        }
    )
}
pub fn record_case(d: &Pipeline, frames: u64, seed: u64) -> Value {
    let mut random = Random::new(seed);
    let cf = d.store.geometry.chunk_frames;
    let instructions = vec![
        json!({"kind":"process_input","port":0,"destination":0}),
        json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
        json!({"kind":"process_loop_capture","view":1,"enabled":2,"source":1}),
        json!({"kind":"process_loop_playback","view":3,"position":4,"length":5,"enabled":6,"destination":3}),
        json!({"kind":"process_output","source":3,"port":0}),
    ];
    let mut case = super::audio::case(
        d.audio.capacities,
        frames,
        instructions,
        vec![
            super::audio::ramp(ONE / 2, ONE / 2, 0, 0),
            word(0),
            word(1),
            word(0),
            word(0),
            word(frames / cf),
            word(0),
        ],
        5,
        2,
        vec![],
    );
    case["program"]["chunk_frames"] = json!(cf);
    case["media"] = json!({"chunk_frames":cf,"chunk_limit":d.store.geometry.slots,"block_chunks":d.store.geometry.block_chunks,"samples":[],"free_chunks":[],"views":[{"chunks":[],"frames":0}]});
    let writes =
        |record, play| json!([{"address":2,"value":word(record)},{"address":6,"value":word(play)}]);
    let first: Vec<_> = (0..frames).map(|_| random.signed()).collect();
    case["steps"] = json!([{"writes":[],"inputs":[first]},{"writes":writes(0,1),"inputs":[vec![0;frames as usize]]},{"writes":writes(1,0),"inputs":[vec![-3;frames as usize]],"media_actions":[{"kind":"clear","view":0}]},{"writes":writes(0,1),"inputs":[vec![0;frames as usize]]}]);
    case
}
fn check(actual: &Value, outputs: Vec<Vec<i32>>, buffers: &[(usize, Vec<i32>)]) {
    assert_eq!(actual["status"], json!({"kind":"completed"}));
    assert_eq!(actual["outputs"], json!(outputs));
    for (index, data) in buffers {
        assert_eq!(actual["buffers"][*index], json!(data));
    }
}
fn record_session(d: &mut Pipeline, s: &mut Sim<'_>, frames: u64, seed: u64) {
    let case = record_case(d, frames, seed);
    d.load(s, &case);
    let captured: Vec<Vec<i32>> = [0, 2]
        .into_iter()
        .map(|i| {
            serde_json::from_value::<Vec<i32>>(case["steps"][i]["inputs"][0].clone())
                .unwrap()
                .into_iter()
                .map(product)
                .collect()
        })
        .collect();
    for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let data = match i {
            1 => captured[0].clone(),
            3 => captured[1].clone(),
            _ => vec![0; frames as usize],
        };
        check(
            &d.quantum(s, step, true),
            vec![data, vec![0; frames as usize]],
            &[],
        );
    }
    let result = d.store.command(
        s,
        &json!({"kind":"read","view":0,"position":0,"length":frames,"frames":frames}),
        true,
        true,
        true,
    );
    assert_eq!(
        result["value"],
        json!({"kind":"samples","value":captured[1]})
    );
}
pub fn directed_pipeline(d: &mut Pipeline, s: &mut Sim<'_>) {
    let cf = d.store.geometry.chunk_frames;
    for frames in [cf, d.audio.capacities[3]]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        record_session(d, s, frames, 20261110);
    }
    let mut data: Vec<i32> = (0..cf).map(|i| 100000 + i as i32 * 128).collect();
    let instructions = vec![
        json!({"kind":"process_loop_playback","view":0,"position":2,"length":3,"enabled":1,"destination":0}),
        json!({"kind":"process_gain","ramp":4,"source":0,"destination":1}),
        json!({"kind":"process_loop_capture","view":0,"enabled":1,"source":1}),
        json!({"kind":"process_loop_playback","view":0,"position":5,"length":6,"enabled":1,"destination":3}),
        json!({"kind":"process_output","source":3,"port":0}),
    ];
    let mut case = super::audio::case(
        d.audio.capacities,
        cf,
        instructions,
        vec![
            word(0),
            word(1),
            word(0),
            word(1),
            super::audio::ramp(ONE / 2, ONE / 2, 0, 0),
            word(1),
            word(2),
        ],
        5,
        1,
        vec![],
    );
    case["program"]["chunk_frames"] = json!(cf);
    case["media"] = json!({"chunk_frames":cf,"samples":data,"free_chunks":[],"views":[{"chunks":[0],"frames":cf}]});
    d.load(s, &case);
    for index in 0..3 {
        let writes: Vec<_> = [(2, index), (3, index + 1), (5, index + 1), (6, index + 2)]
            .into_iter()
            .map(|(a, v)| json!({"address":a,"value":word(v)}))
            .collect();
        let actual = d.quantum(s, &json!({"writes":writes,"inputs":[]}), true);
        data = data.into_iter().map(product).collect();
        check(&actual, vec![data.clone()], &[(3, data.clone())]);
    }
}
pub fn randomized_pipeline(d: &mut Pipeline, s: &mut Sim<'_>, count: u64) {
    for i in 0..count {
        record_session(d, s, d.store.geometry.chunk_frames, 20261111 + i);
    }
}

#[derive(Default)]
struct Writes {
    path: String,
    records: Vec<(u64, u64)>,
    cycle: u64,
}
impl Agent for Writes {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) {
            self.cycle += 1;
            if m.get("reset") == 0 && m.get(&format!("{}.playback_write", self.path)) != 0 {
                assert_eq!(
                    m.get("buffer_ready") & 1,
                    0,
                    "playback published before final write"
                );
                self.records.push((
                    self.cycle,
                    m.get(&format!("{}.scratch_ram_write_address", self.path)),
                ));
            }
        }
    }
}
fn playback_case(d: &Pipeline, frames: u64, second: bool) -> Value {
    let cf = d.store.geometry.chunk_frames;
    let mut memory = vec![word(0), word(0), word(1), word(1)];
    let mut instructions = vec![
        json!({"kind":"process_loop_playback","view":0,"position":1,"length":2,"enabled":3,"destination":0}),
    ];
    if second {
        memory.extend([word(1), word(0), word(1), word(1)]);
        instructions.push(json!({"kind":"process_loop_playback","view":4,"position":5,"length":6,"enabled":7,"destination":1}));
    }
    instructions.push(json!({"kind":"process_output","source":0,"port":0}));
    let views = if second { 2 } else { 1 };
    let mut case = super::audio::case(
        d.audio.capacities,
        frames,
        instructions,
        memory,
        2,
        1,
        vec![],
    );
    case["program"]["chunk_frames"] = json!(cf);
    case["media"] = json!({"chunk_frames":cf,"samples":(0..views*cf).map(|i| 13-7*i as i32).collect::<Vec<_>>(),"free_chunks":[],"views":(0..views).map(|i| json!({"chunks":[i],"frames":cf})).collect::<Vec<_>>()});
    case
}
fn warm(d: &mut Pipeline, s: &mut Sim<'_>, case: &Value, writes: &Rc<RefCell<Writes>>) {
    d.load(s, case);
    let cf = d.store.geometry.chunk_frames;
    let result = d.store.command(s,&json!({"kind":"read","view":0,"position":0,"length":cf,"frames":case["program"]["quantum"]}),false,true,true);
    assert_eq!(result["status"]["kind"], "completed");
    writes.borrow_mut().records.clear();
}
pub fn playback_stream_protocol(d: &mut Pipeline, s: &mut Sim<'_>) {
    let path = core(d, "").trim_end_matches('.').to_owned();
    let writes = Rc::new(RefCell::new(Writes {
        path,
        ..Default::default()
    }));
    s.agent(writes.clone());
    let frames = d.audio.capacities[3].min(d.store.geometry.chunk_frames);
    let case = playback_case(d, frames, false);
    let expected: Vec<_> = (0..frames).map(|i| 13 - 7 * i as i32).collect();
    warm(d, s, &case, &writes);
    check(
        &d.quantum(s, &case["steps"][0], false),
        vec![expected.clone()],
        &[],
    );
    assert_eq!(
        writes
            .borrow()
            .records
            .iter()
            .map(|(_, a)| *a)
            .collect::<Vec<_>>(),
        (0..frames).collect::<Vec<_>>()
    );
    assert!(
        writes
            .borrow()
            .records
            .windows(2)
            .all(|w| w[1].0 - w[0].0 == 1)
    );
    for prefix in [0, 1.min(frames - 1), frames / 2, frames - 1]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        warm(d, s, &case, &writes);
        d.audio.clear_jobs();
        s.model.set_all(&[
            ("run_buffers", 2),
            ("run_inputs", 0),
            ("run_outputs", 1),
            ("job_enable", 1),
            ("drain_ready", 0),
        ]);
        d.audio.start(s);
        let ready = core(d, "media_read_ready");
        let write = core(d, "playback_write");
        s.until(10_000, "playback reset placement", |m| {
            writes.borrow().records.len() as u64 >= prefix
                && m.get(&ready) != 0
                && (prefix != 0 || m.get(&write) != 0)
        });
        s.model.set("reset", 1);
        s.idle(1);
        s.model.set("reset", 0);
        for _ in 0..6 {
            s.idle(1);
            assert_eq!(
                s.model.get(&core(d, "scratch_write")) | s.model.get("buffer_ready"),
                0
            );
        }
        warm(d, s, &case, &writes);
        check(
            &d.quantum(s, &case["steps"][0], false),
            vec![expected.clone()],
            &[],
        );
    }
    if frames >= 16 && d.store.mode != Mode::Legacy {
        let abort = playback_case(d, frames, true);
        warm(d, s, &abort, &writes);
        {
            let mut m = d.store.memory.borrow_mut();
            m.control.pause_responses = true;
            m.control.fail_next = true;
        }
        d.audio.clear_jobs();
        s.model.set_all(&[
            ("run_buffers", 2),
            ("run_inputs", 0),
            ("run_outputs", 1),
            ("job_enable", 1),
            ("drain_ready", 0),
        ]);
        d.audio.start(s);
        s.until(10_000, "cached playback overlaps failing prefetch", |_| {
            !writes.borrow().records.is_empty()
        });
        assert!(d.store.memory.borrow().active().is_some_and(|a| a.error));
        d.store.memory.borrow_mut().control.pause_responses = false;
        s.until(10_000, "playback fault completion", |m| m.get("done") != 0);
        assert_eq!(s.model.get("error_stage"), 2);
        assert_eq!(s.model.get("error_code"), 8);
        assert_eq!(s.model.get("buffer_ready") & 1, 0);
        for _ in 0..6 {
            s.idle(1);
            assert_eq!(
                s.model.get(&core(d, "playback_write")) | s.model.get(&core(d, "scratch_write")),
                0
            );
        }
        warm(d, s, &case, &writes);
        check(&d.quantum(s, &case["steps"][0], false), vec![expected], &[]);
    }
}
pub fn pipeline_protocol(d: &mut Pipeline, s: &mut Sim<'_>) {
    let cf = d.store.geometry.chunk_frames;
    let case = record_case(d, cf, 20261110);
    let inputs: Vec<Vec<i32>> = serde_json::from_value(case["steps"][0]["inputs"].clone()).unwrap();
    d.load(s, &case);
    for (port, data) in inputs.iter().enumerate() {
        for (frame, &sample) in data.iter().enumerate() {
            d.audio.sample_config(
                s,
                0,
                port as u64 * d.audio.capacities[3] + frame as u64,
                Some(sample),
                false,
            );
        }
    }
    s.model.set_all(&[
        ("run_buffers", 5),
        ("run_inputs", 1),
        ("run_outputs", 2),
        ("job_enable", 0),
        ("drain_ready", 0),
    ]);
    d.audio.start(s);
    s.until(200, "jobs start", |m| m.get("jobs_busy") != 0);
    d.store.set(
        s,
        &[
            ("cmd_valid", 1),
            ("cmd_opcode", 0),
            ("cfg_valid", 1),
            ("cfg_write", 1),
            ("cfg_space", 0),
            ("cfg_address", 0),
            ("cfg_write_data", 0),
        ],
    );
    s.model.set_all(&[
        ("run_valid", 1),
        ("cfg_valid", 1),
        ("cfg_state", 1),
        ("cfg_write", 1),
        ("cfg_address", 0),
        ("cfg_write_data", 7),
        ("audio_cfg_valid", 1),
        ("audio_cfg_space", 0),
        ("audio_cfg_write", 1),
        ("audio_cfg_address", 0),
        ("audio_cfg_write_data", 123),
    ]);
    for _ in 0..25 {
        let before = d.audio.tick(s).0;
        assert_eq!(
            before.get("cfg_ready")
                | before.get("audio_cfg_ready")
                | before.get("run_ready")
                | s.model.get("store_cmd_ready")
                | s.model.get("store_cfg_ready"),
            0
        );
    }
    d.store.set(s, &[("cmd_valid", 0), ("cfg_valid", 0)]);
    s.model.set_all(&[
        ("run_valid", 0),
        ("cfg_valid", 0),
        ("audio_cfg_valid", 0),
        ("job_enable", 1),
    ]);
    s.until(10_000, "pipeline finish after intrusion", |m| {
        m.get("done") != 0
    });
    assert_eq!(s.model.get("error_code"), 0);
    d.audio.tick(s);
    let (_, outputs) = d.audio.read_samples(s);
    assert_eq!(outputs, vec![vec![0; cf as usize]; 2]);
    d.audio.program.snapshot(s);
    d.store.snapshot(s);
    let mut fault = case.clone();
    fault["media"]["samples"] = json!((1..=cf).map(|i| i as i32).collect::<Vec<_>>());
    fault["media"]["views"] = json!([{"chunks":[0],"frames":cf}]);
    d.load(s, &fault);
    for write in fault["steps"][1]["writes"].as_array().unwrap() {
        d.audio.program.configure(
            s,
            true,
            write["address"].as_u64().unwrap(),
            Some(&codec::state(&write["value"])),
            false,
        );
    }
    d.store.memory.borrow_mut().control.fail_next = true;
    assert_eq!(
        d.audio.execute(s, &vec![vec![0; cf as usize]], true),
        (2, 8)
    );
    assert_eq!(s.model.get("run_ready"), 0);
    d.load(s, &case);
    check(
        &d.quantum(s, &case["steps"][0], true),
        vec![vec![0; cf as usize]; 2],
        &[],
    );
    if matches!(d.store.mode, Mode::Fixed | Mode::Dramsim3) && d.audio.capacities[3] >= 16 {
        let frames = d.audio.capacities[3];
        let instructions = vec![
            json!({"kind":"process_input","port":0,"destination":0}),
            json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
            json!({"kind":"process_mix","left":0,"right":1,"destination":2}),
            json!({"kind":"process_loop_playback","view":1,"position":2,"length":3,"enabled":4,"destination":3}),
            json!({"kind":"process_output","source":2,"port":0}),
        ];
        let data: Vec<_> = (0..frames).map(|i| 7 * i as i32 - 13).collect();
        let half: Vec<_> = data.iter().copied().map(product).collect();
        let mixed: Vec<_> = data
            .iter()
            .zip(&half)
            .map(|(&a, &b)| {
                (i64::from(a) + i64::from(b)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
            })
            .collect();
        let mut abort = super::audio::case(
            d.audio.capacities,
            frames,
            instructions,
            vec![
                super::audio::ramp(ONE / 2, ONE / 2, 0, 0),
                word(0),
                word(0),
                word(frames / cf),
                word(1),
            ],
            4,
            1,
            vec![data.clone()],
        );
        abort["program"]["chunk_frames"] = json!(cf);
        abort["media"] = json!({"chunk_frames":cf,"samples":(0..frames).map(|i| i as i32).collect::<Vec<_>>(),"free_chunks":[],"views":[{"chunks":(0..frames/cf).collect::<Vec<_>>(),"frames":frames}]});
        for (kind, destination, wanted) in [("gain", 1, &half), ("mix", 2, &mixed)] {
            d.load(s, &abort);
            {
                let mut m = d.store.memory.borrow_mut();
                m.control.stalls = false;
                m.control.pause_responses = true;
                m.control.fail_next = true;
            }
            for (frame, &sample) in data.iter().enumerate() {
                d.audio
                    .sample_config(s, 0, frame as u64, Some(sample), false);
            }
            d.audio.clear_jobs();
            s.model.set_all(&[
                ("run_buffers", 4),
                ("run_inputs", 1),
                ("run_outputs", 1),
                ("job_enable", 1),
                ("drain_ready", 0),
            ]);
            d.audio.start(s);
            let streaming = core(d, "streaming");
            let store = core(d, "stream_store_valid");
            s.until(1000, "arithmetic overlaps failing prefetch", |m| {
                d.audio.consumed().last().is_some_and(|j| j["kind"] == kind)
                    && m.get(&streaming) != 0
                    && m.get(&store) != 0
            });
            assert!(d.store.memory.borrow().active().is_some_and(|a| a.error));
            d.store.memory.borrow_mut().control.pause_responses = false;
            let mut ready_at_fault = None;
            let mut done = false;
            for _ in 0..1000 {
                let after = d.audio.tick(s).1;
                if s.model.get(&core(d, "media_fault")) != 0 && ready_at_fault.is_none() {
                    ready_at_fault = Some(after.get("buffer_ready") & (1 << destination));
                }
                if after.get("done") != 0 {
                    assert_eq!(after.get("error_stage"), 2);
                    assert_eq!(after.get("error_code"), 8);
                    done = true;
                    break;
                }
            }
            assert!(done);
            let ready = ready_at_fault.expect("media fault visible");
            assert_eq!(s.model.get("buffer_ready") & (1 << destination), ready);
            if ready != 0 {
                let (buffers, _) = d.audio.read_samples(s);
                assert_eq!(buffers[destination], json!(wanted));
            }
            for _ in 0..6 {
                s.idle(1);
                assert_eq!(
                    s.model.get(&core(d, "stream_store_valid"))
                        | s.model.get(&core(d, "scratch_write")),
                    0
                );
            }
        }
        d.load(s, &case);
        check(
            &d.quantum(s, &case["steps"][0], true),
            vec![vec![0; cf as usize]; 2],
            &[],
        );
    }
}
pub fn blocked_audio(d: &mut Pipeline, s: &mut Sim<'_>) {
    let cf = d.store.geometry.chunk_frames;
    let bc = d.store.geometry.block_chunks;
    for frames in [cf, d.audio.capacities[3]]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        record_session(d, s, frames, 20261110);
    }
    let mut shared = record_case(d, cf, 20261112);
    shared["initial_state"]["memory"][2] = word(0);
    shared["initial_state"]["memory"][6] = word(1);
    shared["initial_state"]["memory"][3] = word(1);
    let mut samples: Vec<i32> = (1..=3 * cf).map(|i| i as i32).collect();
    samples.resize((3u64.div_ceil(bc) * bc * cf) as usize, 0);
    shared["media"]["samples"] = json!(samples);
    shared["media"]["views"] = json!([{"chunks":[0,1,2],"frames":3*cf}]);
    shared["steps"] = json!([{"writes":[],"inputs":[vec![0;cf as usize]],"media_actions":[{"kind":"slice","source":0,"start":cf,"frames":cf},{"kind":"clear","view":0}]},{"writes":[{"address":2,"value":word(1)}],"inputs":[vec![1234;cf as usize]]}]);
    d.load(s, &shared);
    for step in shared["steps"].as_array().unwrap() {
        check(
            &d.quantum(s, step, true),
            vec![
                (cf + 1..=2 * cf).map(|i| i as i32).collect(),
                vec![0; cf as usize],
            ],
            &[],
        );
    }
    let result = d.store.command(
        s,
        &json!({"kind":"read","view":0,"position":0,"length":cf,"frames":cf}),
        true,
        true,
        true,
    );
    assert_eq!(
        result["value"],
        json!({"kind":"samples","value":vec![617;cf as usize]})
    );
}
pub fn run(
    name: &str,
    c: [u64; 7],
    g: Geometry,
    width: u64,
    mode: Mode,
    response_cycles: u64,
    random: u64,
) -> Result<()> {
    let compiled = design(c, &g, width, mode, response_cycles)
        .build(&crate::root().join("work/marlin-suite/pipeline"))?;
    if g.block_chunks != 1 {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Pipeline::new(&mut s, c, g, mode, 20261110)?;
        blocked_audio(&mut d, &mut s);
        d.store.memory.borrow_mut().report(true);
    } else {
        for (label, test) in [
            (
                "directed_pipeline",
                directed_pipeline as fn(&mut Pipeline, &mut Sim<'_>),
            ),
            ("playback_stream_protocol", playback_stream_protocol),
            ("pipeline_protocol", pipeline_protocol),
        ] {
            println!("pipeline/{name}/{width}/{mode:?}/{label}");
            let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
            let mut d = Pipeline::new(&mut s, c, g.clone(), mode, 20261110)?;
            test(&mut d, &mut s);
            d.store.memory.borrow_mut().report(true);
        }
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Pipeline::new(&mut s, c, g, mode, 20261110)?;
        randomized_pipeline(&mut d, &mut s, random);
        d.store.memory.borrow_mut().report(true);
    }
    Ok(())
}
