//! Arithmetic vectors and real sample-stream/reset checks, without an instruction interpreter.
use crate::{
    Result,
    drivers::audio::Audio,
    model::{Design, Probe},
    sim::Sim,
};
use serde_json::{Value, json};
const ONE: i32 = 1 << 24;
pub const CONFIGS: &[(&str, [u64; 7])] = &[
    ("default", [32, 16, 32, 32, 16, 2, 2]),
    ("small", [16, 8, 16, 3, 8, 2, 2]),
];
pub fn design(c: [u64; 7]) -> Design {
    let root = crate::root();
    let mut sources = super::state::design(c[..3].try_into().unwrap()).sources;
    sources.extend(
        "jp_sample_ram jp_divider job_processor audio_pipeline"
            .split_whitespace()
            .map(|n| root.join(format!("fpga/job_processor/rtl/{n}.sv"))),
    );
    Design::new("audio_pipeline", sources)
        .parameters(
            &[
                "PROGRAM_DEPTH",
                "STATE_DEPTH",
                "JOB_DEPTH",
                "MAX_FRAMES",
                "BUFFERS",
                "INPUT_PORTS",
                "OUTPUT_PORTS",
            ]
            .iter()
            .zip(c)
            .map(|(&k, v)| (k, v))
            .collect::<Vec<_>>(),
        )
        .probes(
            [
                "stream_write",
                "scratch_ram_write_address",
                "streaming",
                "scratch_write",
                "stream_store_valid",
            ]
            .iter()
            .map(|n| Probe::observe(&format!("sample_engine.{n}")))
            .collect(),
        )
}
pub fn run(name: &str, c: [u64; 7], random: u64) -> Result<()> {
    let model = design(c).build(&crate::root().join("work/marlin-suite/audio"))?;
    let mut sim = Sim::new(model.session(None)?, &[("clk", 10_000, 0)]);
    let mut driver = Audio::new(&mut sim, c, 20261110);
    directed_audio(&mut driver, &mut sim);
    randomized_audio(&mut driver, &mut sim, random);
    sample_protocol(&mut driver, &mut sim);
    arithmetic_stream_protocol(&mut driver, &mut sim);
    production_numeric_vectors(&mut driver, &mut sim);
    println!("audio/{name}: 5 regression groups passed, {random} seeded cases");
    Ok(())
}
pub fn ramp(start: i32, target: i32, duration: u64, elapsed: u64) -> Value {
    json!({"kind":"ramp","value":{"start":start,"target":target,"duration":duration,"elapsed":elapsed}})
}
pub fn samples(frames: u64, phase: usize) -> Vec<i32> {
    let edges = [
        -1,
        -3,
        3,
        i32::MIN,
        i32::MAX,
        ONE,
        -ONE,
        0,
        1,
        i32::MAX - 1,
        i32::MIN + 1,
        123456789,
    ];
    (0..frames as usize)
        .map(|i| edges[(i + phase) % edges.len()])
        .collect()
}
pub fn case(
    c: [u64; 7],
    frames: u64,
    instructions: Vec<Value>,
    memory: Vec<Value>,
    buffers: u64,
    outputs: u64,
    inputs: Vec<Vec<i32>>,
) -> Value {
    let mut value = super::state::case(instructions, memory, c[2], None, frames);
    value["buffers"] = json!(buffers);
    value["output_ports"] = json!(outputs);
    value["steps"] = json!([{"writes":[],"inputs":inputs}]);
    value
}
fn graph(d: &Audio, frames: u64, ramps: Vec<Value>) -> Value {
    case(
        d.capacities,
        frames,
        vec![
            json!({"kind":"process_input","port":0,"destination":0}),
            json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
            json!({"kind":"process_input","port":1,"destination":2}),
            json!({"kind":"process_gain","ramp":1,"source":2,"destination":3}),
            json!({"kind":"process_mix","left":1,"right":3,"destination":4}),
            json!({"kind":"process_output","source":4,"port":0}),
        ],
        ramps,
        6,
        2,
        vec![samples(frames, 0), samples(frames, 3)],
    )
}
fn frames(d: &Audio) -> Vec<u64> {
    [1, 3.min(d.capacities[3]), d.capacities[3]]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn clip(value: i128) -> i32 {
    value.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
}
fn product(a: i32, b: i32) -> i32 {
    clip(i128::from(a) * i128::from(b) / i128::from(ONE))
}
fn curve(r: &Value, n: usize) -> i32 {
    let start = r["start"].as_i64().unwrap();
    let target = r["target"].as_i64().unwrap();
    let duration = r["duration"].as_u64().unwrap();
    if duration == 0 {
        return target as i32;
    }
    (i128::from(start)
        + (i128::from(target) - i128::from(start))
            * i128::from((r["elapsed"].as_u64().unwrap() + n as u64).min(duration))
            / i128::from(duration)) as i32
}
fn expected(value: &Value, two: bool) -> Value {
    let inputs: Vec<Vec<i32>> =
        serde_json::from_value(value["steps"][0]["inputs"].clone()).unwrap();
    let memory = value["initial_state"]["memory"].as_array().unwrap();
    let first: Vec<_> = inputs[0]
        .iter()
        .enumerate()
        .map(|(i, &v)| product(v, curve(&memory[0]["value"], i)))
        .collect();
    let (mixed, buffers) = if two {
        let second: Vec<_> = inputs[1]
            .iter()
            .enumerate()
            .map(|(i, &v)| product(v, curve(&memory[1]["value"], i)))
            .collect();
        let mixed: Vec<_> = first
            .iter()
            .zip(&second)
            .map(|(&a, &b)| clip(i128::from(a) + i128::from(b)))
            .collect();
        (
            mixed.clone(),
            json!({"0":inputs[0],"1":first,"2":inputs[1],"3":second,"4":mixed}),
        )
    } else {
        let mixed: Vec<_> = inputs[0]
            .iter()
            .zip(&first)
            .map(|(&a, &b)| clip(i128::from(a) + i128::from(b)))
            .collect();
        (mixed.clone(), json!({"0":inputs[0],"1":first,"2":mixed}))
    };
    let mut outputs = vec![mixed];
    outputs.extend((1..value["output_ports"].as_u64().unwrap()).map(|_| vec![0; inputs[0].len()]));
    json!({"outputs":outputs,"buffers":buffers})
}
fn check(wanted: &Value, actual: &Value) {
    assert_eq!(actual["status"], json!({"kind":"completed"}));
    assert_eq!(actual["outputs"], wanted["outputs"]);
    for (b, samples) in wanted["buffers"].as_object().unwrap() {
        assert_eq!(&actual["buffers"][b.parse::<usize>().unwrap()], samples);
    }
}
fn directed_audio(d: &mut Audio, s: &mut Sim<'_>) {
    let mut settings: Vec<_> = [0, 1, -1, ONE, -ONE, ONE / 2, i32::MIN, i32::MAX]
        .into_iter()
        .map(|target| ramp(0, target, 0, 0))
        .collect();
    settings.extend([
        ramp(i32::MIN, i32::MAX, u32::MAX as u64, u32::MAX as u64 - 16),
        ramp(i32::MAX, i32::MIN, u32::MAX as u64, 1234567890),
        ramp(-ONE, ONE, 1, 0),
        ramp(ONE, -ONE, 17, 15),
        ramp(i32::MIN, i32::MAX, 0, u32::MAX as u64),
    ]);
    for n in frames(d) {
        for (i, setting) in settings.iter().enumerate() {
            let value = graph(
                d,
                n,
                vec![setting.clone(), settings[settings.len() - 1 - i].clone()],
            );
            d.load(s, &value);
            check(
                &expected(&value, true),
                &d.quantum(s, &value["steps"][0], true),
            );
        }
        for instruction in [
            json!({"kind":"process_input","port":65535,"destination":0}),
            json!({"kind":"process_mix","left":0,"right":65535,"destination":2}),
            json!({"kind":"process_output","source":65535,"port":0}),
            json!({"kind":"process_gain","ramp":0,"source":1,"destination":2}),
        ] {
            let value = case(
                d.capacities,
                n,
                vec![instruction],
                vec![ramp(ONE, ONE, 0, 0)],
                6,
                2,
                vec![samples(n, 0), samples(n, 3)],
            );
            d.load(s, &value);
            assert_eq!(
                d.execute(
                    s,
                    &serde_json::from_value::<Vec<Vec<i32>>>(value["steps"][0]["inputs"].clone())
                        .unwrap(),
                    true
                ),
                (2, 1)
            );
        }
    }
}
fn randomized_audio(d: &mut Audio, s: &mut Sim<'_>, count: u64) {
    let sizes = frames(d);
    for _ in 0..count {
        let n = sizes[d.random.below(sizes.len() as u64) as usize];
        let settings = (0..2)
            .map(|_| {
                ramp(
                    d.random.signed(),
                    d.random.signed(),
                    d.random.next() as u32 as u64,
                    d.random.next() as u32 as u64,
                )
            })
            .collect();
        let mut value = graph(d, n, settings);
        value["steps"][0]["inputs"] = json!(
            (0..2)
                .map(|_| (0..n).map(|_| d.random.signed()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
        d.load(s, &value);
        check(
            &expected(&value, true),
            &d.quantum(s, &value["steps"][0], true),
        );
    }
}
fn sample_protocol(d: &mut Audio, s: &mut Sim<'_>) {
    let n = d.capacities[3];
    let value = graph(d, n, vec![ramp(0, ONE, 71, 0), ramp(ONE, -ONE, 100, 0)]);
    d.load(s, &value);
    for (space, size) in [
        (0, d.capacities[5] * n),
        (1, d.capacities[6] * n),
        (2, d.capacities[4] * n),
        (3, 0),
    ] {
        d.sample_config(s, space, size, Some(i32::MAX), true);
        d.sample_config(s, space, size, None, true);
    }
    for f in 0..n {
        d.sample_config(
            s,
            1,
            n + f,
            Some(if f % 2 != 0 { i32::MIN } else { i32::MAX }),
            false,
        );
    }
    check(
        &expected(&value, true),
        &d.quantum(s, &value["steps"][0], true),
    );
    d.load(s, &value);
    let inputs: Vec<Vec<i32>> =
        serde_json::from_value(value["steps"][0]["inputs"].clone()).unwrap();
    for (p, data) in inputs.iter().enumerate() {
        for (f, &v) in data.iter().enumerate() {
            d.sample_config(s, 0, p as u64 * n + f as u64, Some(v), false);
        }
    }
    s.model.set_all(&[
        ("run_buffers", d.buffers),
        ("run_inputs", 2),
        ("run_outputs", 2),
        ("job_enable", 0),
    ]);
    d.start(s);
    let mut ready = false;
    for _ in 0..200 {
        if d.tick(s).1.get("jobs_busy") != 0 {
            ready = true;
            break;
        }
    }
    assert!(ready);
    s.model.set_all(&[
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
        ("run_valid", 1),
    ]);
    let queued = s.model.get("queued_jobs");
    for _ in 0..2 * n + 20 {
        let (before, after) = d.tick(s);
        assert_eq!(before.get("monitor_ready"), 0);
        assert_eq!(after.get("queued_jobs"), queued);
    }
    s.model.set_all(&[
        ("cfg_valid", 0),
        ("audio_cfg_valid", 0),
        ("run_valid", 0),
        ("job_enable", 1),
    ]);
    let mut gain = false;
    for _ in 0..300 {
        d.tick(s);
        if d.consumed().iter().any(|j| j["kind"] == "gain") {
            gain = true;
            break;
        }
    }
    assert!(gain);
    for _ in 0..10 {
        d.tick(s);
    }
    s.model.set("reset", 1);
    let (_, after) = d.tick(s);
    assert_eq!(
        after.get("busy") | after.get("queued_jobs") | after.get("buffer_ready"),
        0
    );
    s.model.set("reset", 0);
    d.tick(s);
    assert_eq!(d.sample_config(s, 0, 0, None, false), inputs[0][0]);
    let mut resumed = value;
    resumed["initial_state"] = d.program.snapshot(s);
    check(
        &expected(&resumed, true),
        &d.quantum(s, &resumed["steps"][0], true),
    );
    let n = 3.min(n);
    let unsupported = case(
        d.capacities,
        n,
        vec![
            json!({"kind":"process_input","port":0,"destination":0}),
            json!({"kind":"process_loop_capture","view":0,"enabled":1,"source":0}),
        ],
        vec![super::state::word(1); 2],
        2,
        1,
        vec![samples(n, 0), samples(n, 3)],
    );
    d.load(s, &unsupported);
    assert_eq!(
        d.execute(
            s,
            &serde_json::from_value::<Vec<Vec<i32>>>(unsupported["steps"][0]["inputs"].clone())
                .unwrap(),
            true
        ),
        (2, 3)
    );
}
fn arithmetic_stream_protocol(d: &mut Audio, s: &mut Sim<'_>) {
    use crate::{clock::Edges, model::Model, sim::Agent};
    use std::{cell::RefCell, rc::Rc};
    #[derive(Default)]
    struct Stream {
        enabled: bool,
        kind: String,
        writes: std::collections::BTreeMap<String, Vec<(u64, u64)>>,
    }
    impl Agent for Stream {
        fn sample(&mut self, m: &Model<'_>, e: Edges) {
            if !self.enabled || !e.rises(0) || m.get("reset") != 0 {
                return;
            }
            if m.get("monitor_valid") != 0 && m.get("monitor_ready") != 0 {
                self.kind = crate::codec::job(&m.bits("monitor_data"))["kind"]
                    .as_str()
                    .unwrap()
                    .into();
            }
            if m.get("sample_engine.stream_write") != 0 {
                assert!(matches!(self.kind.as_str(), "gain" | "mix"));
                let destination = if self.kind == "gain" { 1 } else { 2 };
                assert_eq!(
                    m.bits("buffer_ready").get(destination, 1),
                    0,
                    "buffer published before final write"
                );
                self.writes.entry(self.kind.clone()).or_default().push((
                    e.ps / 10_000,
                    m.get("sample_engine.scratch_ram_write_address"),
                ));
            }
        }
    }
    let n = d.capacities[3];
    let value = case(
        d.capacities,
        n,
        vec![
            json!({"kind":"process_input","port":0,"destination":0}),
            json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
            json!({"kind":"process_mix","left":0,"right":1,"destination":2}),
            json!({"kind":"process_output","source":2,"port":0}),
        ],
        vec![ramp(0, -ONE / 2, 0, 0)],
        3,
        1,
        vec![samples(n, 0)],
    );
    let stream = Rc::new(RefCell::new(Stream {
        enabled: true,
        ..Stream::default()
    }));
    s.agent(stream.clone());
    d.load(s, &value);
    check(
        &expected(&value, false),
        &d.quantum(s, &value["steps"][0], true),
    );
    for (kind, interval, destination) in [("gain", 1, 1), ("mix", 2, 2)] {
        let stream = stream.borrow();
        let writes = &stream.writes[kind];
        assert_eq!(writes.len() as u64, n);
        assert_eq!(
            writes.iter().map(|&(_, a)| a).collect::<Vec<_>>(),
            (destination * n..(destination + 1) * n).collect::<Vec<_>>()
        );
        assert!(writes.windows(2).all(|w| w[1].0 - w[0].0 == interval));
    }
    stream.borrow_mut().enabled = false;
    for count in [1, n]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        for kind in ["gain", "mix"] {
            for delay in 0..5 {
                let mut reset = value.clone();
                reset["program"]["quantum"] = json!(count);
                reset["initial_state"]["memory"] = json!([ramp(ONE, -ONE, count + 1, 0)]);
                reset["steps"][0]["inputs"] = json!([samples(count, 0)]);
                d.load(s, &reset);
                for (i, v) in samples(count, 0).into_iter().enumerate() {
                    d.sample_config(s, 0, i as u64, Some(v), false);
                }
                s.model.set_all(&[
                    ("run_buffers", 3),
                    ("run_inputs", 1),
                    ("run_outputs", 1),
                    ("job_enable", 1),
                    ("drain_ready", 0),
                ]);
                d.clear_jobs();
                d.start(s);
                let mut streaming = false;
                for _ in 0..1000 {
                    d.tick(s);
                    if d.consumed().last().is_some_and(|j| j["kind"] == kind)
                        && s.model.get("sample_engine.streaming") != 0
                    {
                        streaming = true;
                        break;
                    }
                }
                assert!(streaming, "{kind} streaming phase");
                for _ in 0..delay {
                    d.tick(s);
                }
                s.model.set("reset", 1);
                let (_, after) = d.tick(s);
                assert_eq!(
                    after.get("busy") | after.get("buffer_ready") | after.get("queued_jobs"),
                    0
                );
                s.model.set("reset", 0);
                for _ in 0..6 {
                    d.tick(s);
                    assert_eq!(
                        s.model.get("sample_engine.scratch_write")
                            | s.model.get("sample_engine.stream_store_valid"),
                        0,
                        "stale write after reset"
                    );
                }
                reset["initial_state"] = d.program.snapshot(s);
                check(
                    &expected(&reset, false),
                    &d.quantum(s, &reset["steps"][0], true),
                );
            }
        }
    }
}
fn production_numeric_vectors(d: &mut Audio, s: &mut Sim<'_>) {
    use rt_audio_math::{Q824, Ramp, Sample};
    let settings = [
        (0, ONE, 0, 0),
        (0, ONE, 1, 0),
        (0, ONE, 7, 0),
        (ONE, -ONE, 17, 15),
        (i32::MIN, i32::MAX, 33, 0),
        (i32::MAX, i32::MIN, 7, 0),
        (i32::MIN, i32::MAX, u32::MAX, 1234567890),
        (i32::MAX, i32::MIN, u32::MAX, u32::MAX - 16),
        (0, -ONE / 2, 0, 0),
        (0, i32::MIN, 0, 0),
        (i32::MIN, i32::MAX, 0, u32::MAX),
        (ONE, -ONE, 7, u32::MAX),
    ];
    let edges = [i32::MIN, i32::MAX, -3, -1, 0, 1, 3, ONE, -ONE, 123456789];
    for n in frames(d) {
        for (case_index, &(start, target, duration, elapsed)) in settings.iter().enumerate() {
            let first: Vec<_> = (0..n as usize)
                .map(|i| edges[(i + case_index) % edges.len()])
                .collect();
            let second: Vec<_> = (0..n as usize)
                .map(|i| edges[(i + case_index + 3) % edges.len()])
                .collect();
            let numeric = Ramp {
                start: Q824(start),
                target: Q824(target),
                duration,
                elapsed,
            };
            let wanted: Vec<_> = first
                .iter()
                .zip(&second)
                .enumerate()
                .map(|(i, (&a, &b))| Q824(a).multiply(numeric.at(i as u32)).mix(Q824(b)).0)
                .collect();
            let value = case(
                d.capacities,
                n,
                vec![
                    json!({"kind":"process_input","port":0,"destination":0}),
                    json!({"kind":"process_gain","ramp":0,"source":0,"destination":1}),
                    json!({"kind":"process_input","port":1,"destination":2}),
                    json!({"kind":"process_mix","left":1,"right":2,"destination":3}),
                    json!({"kind":"process_output","source":3,"port":0}),
                ],
                vec![ramp(start, target, u64::from(duration), u64::from(elapsed))],
                4,
                1,
                vec![first, second],
            );
            d.load(s, &value);
            assert_eq!(
                d.execute(
                    s,
                    &serde_json::from_value::<Vec<Vec<i32>>>(value["steps"][0]["inputs"].clone())
                        .unwrap(),
                    true
                ),
                (2, 0)
            );
            assert_eq!(d.read_samples(s).1[0], wanted);
            assert_eq!(
                d.program.snapshot(s)["memory"][0]["value"]["elapsed"],
                json!(u64::from(duration).min(u64::from(elapsed) + n))
            );
        }
    }
}
