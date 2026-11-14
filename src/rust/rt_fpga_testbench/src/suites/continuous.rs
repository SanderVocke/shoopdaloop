//! Continuous frame deadlines, control backpressure, and production samples/state/media.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    drivers::{
        frames::Frames,
        state::Program,
        store::{Geometry, Store},
        stream::Source,
        transition::Transition,
    },
    images,
    memory::Mode,
    model::{Design, Model, Probe},
    prepared::{self, Prepared},
    sim::{Agent, Sim},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, path::Path, rc::Rc};
#[derive(Clone, Debug, serde::Serialize)]
pub struct Config {
    pub q: u64,
    pub cf: u64,
    pub p: u64,
    pub banks: u64,
    pub channels: u64,
    pub slots: u64,
    pub bc: u64,
    pub width: u64,
    pub period_ps: u64,
    pub phase_ps: u64,
    pub ticks: u64,
    pub control_depth: u64,
    pub monitoring: bool,
    pub routing: String,
    pub stalls: bool,
    pub overrun: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            q: 16,
            cf: 8,
            p: 8,
            banks: 3,
            channels: 2,
            slots: 256,
            bc: 32,
            width: 64,
            period_ps: 13_000,
            phase_ps: 3_000,
            ticks: 1024,
            control_depth: 3,
            monitoring: true,
            routing: "reverse".into(),
            stalls: true,
            overrun: false,
        }
    }
}
pub fn sources(demo: bool) -> Vec<std::path::PathBuf> {
    let root = crate::root();
    let mut sources = rt_fpga_sim::pipeline_sources();
    sources.extend(
        "audio_lane_ram audio_io audio_command_fifo audio_image audio_actions audio_native audio_observations audio_control audio_loop_pipeline"
            .split_whitespace()
            .map(|n| root.join(format!("fpga/audio_io/rtl/{n}.sv"))),
    );
    if demo {
        sources.extend(
            "audio_boot audio_events audio_demo"
                .split_whitespace()
                .map(|n| root.join(format!("fpga/audio_io/rtl/{n}.sv"))),
        );
    }
    sources
}
pub fn geometry(c: &Config) -> Geometry {
    Geometry::new([32, c.slots, c.cf], c.bc)
}
pub fn prepare(c: &Config) -> Result<Prepared> {
    prepared::prepare(
        prepared::scenario(c.q, c.cf, c.channels, c.monitoring, &c.routing),
        c.bc,
        c.slots,
    )
}
pub fn design(c: &Config, mode: Mode, demo: bool, p: &Prepared) -> Design {
    design_inner(c, mode, demo.then_some(p))
}
pub fn live_design(c: &Config, mode: Mode) -> Design {
    design_inner(c, mode, None)
}
fn design_inner(c: &Config, mode: Mode, p: Option<&Prepared>) -> Design {
    let demo = p.is_some();
    assert_ne!(mode, Mode::Legacy);
    let g = geometry(c);
    let mut d = Design::new(
        if demo {
            "audio_demo"
        } else {
            "audio_loop_pipeline"
        },
        sources(demo),
    );
    let mut parameters = if let Some(p) = p {
        vec![
            ("Q", c.q),
            ("CHANNELS", c.channels),
            ("BOOT_WORDS", p.boot.len() as u64),
            ("EVENT_WORDS", p.events.len() as u64),
            (
                "INSTRUCTIONS",
                p.image["program"]["instructions"].as_array().unwrap().len() as u64,
            ),
            (
                "STATES",
                p.image["initial_state"]["memory"].as_array().unwrap().len() as u64,
            ),
            ("BUFFERS", p.image["buffers"].as_u64().unwrap()),
            (
                "JOB_BUDGET",
                p.image["program"]["jobs_per_quantum"].as_u64().unwrap(),
            ),
        ]
    } else {
        vec![
            ("MAX_FRAMES", c.q),
            ("INPUT_PORTS", c.channels),
            ("OUTPUT_PORTS", c.channels),
            ("REQUEST_DEPTH", 128),
            ("EVENT_DEPTH", 256),
            ("CONTROL_DEPTH", c.control_depth),
        ]
    };
    parameters.extend([
        ("P", c.p),
        ("RX_BANKS", c.banks),
        ("TX_BANKS", c.banks),
        ("AXI_DATA_WIDTH", c.width),
        ("PHYSICAL_BASE", 0x01000000),
    ]);
    parameters.extend(
        g.parameters().into_iter().filter(|(n, _)| {
            !matches!(*n, "READ_BUFFERS" | "WRITE_BUFFERS" | "REUSE_VIEWS") || !demo
        }),
    );
    d = d.parameters(&parameters);
    if let Some(p) = p {
        for name in ["BOOT", "EVENT"] {
            d.parameters.insert(
                format!("{name}_FILE"),
                format!(
                    "\"{}\"",
                    p.directory
                        .join(format!(
                            "{}.hex",
                            if name == "BOOT" { "boot" } else { "events" }
                        ))
                        .display()
                ),
            );
        }
    } else {
        d.probes = vec![Probe::observe("io.tx_request_sync")];
    }
    if mode == Mode::Instant {
        d = d.memory(g.words(), 0x01000000);
    }
    d
}
struct Measure {
    q: u64,
    times: Rc<RefCell<BTreeMap<u64, u64>>>,
    last: u64,
    pub rows: Vec<Value>,
}
impl Agent for Measure {
    fn observe(&mut self, m: &Model<'_>, e: Edges) {
        let current = m.get("io.tx_request_sync");
        if current != self.last {
            self.last = current;
            if let Some(sequence) = m.get("process_sequence").checked_sub(1) {
                let last_input = sequence * self.q + self.q - 1;
                if let Some(&time) = self.times.borrow().get(&last_input) {
                    self.rows.push(json!({"sequence":sequence,"publication_ps":e.ps,"input_to_eligible_ps":e.ps-time}));
                }
            }
        }
    }
}
struct Hold {
    q: u64,
    p: u64,
    released: bool,
}
struct IsolatedService;
impl Agent for IsolatedService {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("enable") != 0 {
            for signal in ["control_valid", "response_ready"] {
                assert_eq!(
                    m.get(signal),
                    0,
                    "service traffic during autonomy test: {signal}"
                );
            }
            for signal in [
                "command_ready",
                "cfg_ready",
                "audio_cfg_ready",
                "store_cmd_ready",
                "store_cfg_ready",
                "store_write_ready",
                "store_read_valid",
            ] {
                assert_eq!(m.get(signal), 0, "live legacy service access: {signal}");
            }
            assert_eq!(
                m.get("control_result_valid"),
                0,
                "unexpected control result during service pause"
            );
        }
    }
}
impl Agent for Hold {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) && !self.released {
            self.released = m.get("session_frame") >= self.q + self.p + 1;
            m.set("job_enable", u64::from(self.released));
        }
    }
}
pub fn frames(
    s: &mut Sim<'_>,
    clock: &Frames,
    c: &Config,
    expected: &Value,
    start: u64,
    count: u64,
    drive_input: bool,
    missed: &[u64],
    times: Option<&Rc<RefCell<BTreeMap<u64, u64>>>>,
) -> Vec<Vec<i32>> {
    let mut actual = Vec::new();
    for frame in start..start + count {
        let input = Bits(
            (0..c.channels)
                .map(|ch| ((ch + 1) * 100000 + frame * 37 + 1) as u32)
                .collect(),
        );
        let (_, raw) = clock.pulse_observe(s, drive_input.then_some(&input), |time| {
            if let Some(times) = times {
                times.borrow_mut().insert(frame, time);
            }
        });
        let values: Vec<_> = (0..c.channels)
            .map(|ch| raw.get(ch as usize * 32, 32) as i32)
            .collect();
        let latency = c.q + c.p;
        let wanted: Vec<i32> = if frame < latency || missed.contains(&((frame - latency) / c.q)) {
            vec![0; c.channels as usize]
        } else {
            let source = frame - latency;
            (0..c.channels)
                .map(|ch| {
                    expected["quanta"][(source / c.q) as usize]["outputs"]["channels"][ch as usize]
                        [(source % c.q) as usize]
                        .as_i64()
                        .unwrap() as i32
                })
                .collect()
        };
        assert_eq!(
            values,
            wanted,
            "frame {frame}, recovery {}, core error {}",
            s.model.get("recovery_reason"),
            s.model.get("error_code")
        );
        assert_eq!(
            s.model.get("recovery_requested"),
            0,
            "frame {frame}, recovery {}",
            s.model.get("recovery_reason")
        );
        actual.push(values);
    }
    actual
}
pub fn run(name: &str, c: &Config, mode: Mode, output: &Path) -> Result<Value> {
    let prepared = prepare(c)?;
    run_prepared(name, c, mode, output, prepared, 13, false, true)
}
fn resident_prepared(c: &Config, quanta: u64) -> Result<Prepared> {
    let mut scenario = prepared::scenario(c.q, c.cf, c.channels, c.monitoring, &c.routing);
    let id = |slot| json!({"slot":slot,"generation":1});
    scenario["loops"][0]["sync_source"] = id(0);
    scenario["loops"][1]["sync_source"] = id(0);
    scenario["loops"][1]["repeat_sync"] = json!(true);
    let steps = scenario["steps"].as_array_mut().unwrap();
    let mut actions = Vec::new();
    for step in steps.iter_mut() {
        actions.extend(step["actions"].as_array_mut().unwrap().drain(..));
    }
    for (name, target, source, at, skip) in [
        ("self-sync", 0, 0, 20 * c.q, 2),
        ("source-sync", 1, 0, 24 * c.q, 1),
        ("cancel-on-stop", 0, 0, 26 * c.q, 100),
    ] {
        actions.push(
            json!({"kind":"transition","name":name,"target":id(target),"at":at,
            "timing":{"kind":"after_ticks","source":id(source),"skip":skip},
            "operation":{"kind":"seek","position":0,"retrigger":true}}),
        );
    }
    actions.push(
        json!({"kind":"transition","name":"future-stop","target":id(0),"at":40*c.q,
        "timing":{"kind":"at"},"operation":{"kind":"stop"}}),
    );
    steps[0]["actions"] = json!(actions);
    for n in steps.len() as u64..quanta + 3 {
        let inputs: Vec<Vec<i32>> = (0..c.channels)
            .map(|ch| {
                (0..c.q)
                    .map(|f| ((ch + 1) * 100000 + (n * c.q + f) * 37 + 1) as i32)
                    .collect()
            })
            .collect();
        steps.push(json!({"inputs":inputs,"actions":[],"collect_completed":false}));
    }
    let prepared = prepared::prepare(scenario, c.bc, c.slots)?;
    assert!(
        prepared.events.is_empty(),
        "service-pause actions must be resident at arm"
    );
    Ok(prepared)
}
pub fn service_pause(name: &str, c: &Config, mode: Mode, output: &Path) -> Result<Value> {
    let prepared = resident_prepared(c, 1040)?;
    let report = run_prepared(name, c, mode, output, prepared, 1040, true, true)?;
    assert_eq!(report["control_transfers"], 0);
    assert!(report["processed"].as_u64().unwrap() >= 1024);
    Ok(report)
}
pub fn command_flood() -> Result<()> {
    let c = Config {
        control_depth: 128,
        ticks: 256,
        ..Config::default()
    };
    let mut prepared = resident_prepared(&c, 160)?;
    for batch in 1..=16 {
        prepared.events.push(Bits(
            rt_fpga_adapter::audio_control::header(0, batch, 64, 128)?.to_vec(),
        ));
        for _ in 0..64 {
            prepared.events.push(crate::drivers::stream::control_packet(
                0,
                0,
                &Bits::default(),
                0,
                0,
            ));
        }
    }
    let output = crate::root().join("work/fpga-completion/control-flood");
    let report = run_prepared(
        "flood",
        &c,
        Mode::Fixed,
        &output,
        prepared,
        160,
        false,
        false,
    )?;
    assert!(report["control_stalls"].as_u64().unwrap() > 0);
    assert_eq!(report["control_result_held"], true);
    assert!(
        report["control_transfers"].as_u64().unwrap() <= 2 * 65 + 128,
        "full result credits were overrun"
    );
    Ok(())
}
fn run_prepared(
    name: &str,
    c: &Config,
    mode: Mode,
    output: &Path,
    prepared: Prepared,
    quanta: u64,
    hold_reply: bool,
    drain_results: bool,
) -> Result<Value> {
    let compiled = design(c, mode, false, &prepared)
        .build(&crate::root().join("work/marlin-suite/continuous"))?;
    let mut s = Sim::new(
        compiled.session(None)?,
        &[("clk", 10_000, 0), ("audio_clk", c.period_ps, c.phase_ps)],
    );
    s.max_ps = 100_000_000_000.max((quanta + 4) * c.q * c.ticks * c.period_ps + 1_000_000_000);
    let begin = std::time::Instant::now();
    let mut store = Store::new(&mut s, geometry(c), mode, 20261112, "store_")?;
    store.memory.borrow_mut().control.stalls = c.stalls;
    let mut transition = Transition::new([16, 128, 256]);
    transition.busy_signal = "transition_busy";
    s.model.set_all(&[
        ("epoch", 1),
        ("job_enable", 1),
        ("control_result_ready", u64::from(drain_results)),
    ]);
    if !drain_results {
        s.agent(crate::protocol::ReadyValid::new(
            "control_result_valid",
            "control_result_ready",
            &["control_result_data"],
        ));
    }
    transition.load(&mut s, &prepared.image["transition"]);
    for transfer in &prepared.transition_setup {
        let (error, _) = transition.raw_command(
            &mut s,
            u64::from(transfer.opcode),
            u64::from(transfer.index),
            &Bits(transfer.words.to_vec()),
            Some(0),
            false,
        );
        assert_eq!(error, 0, "prepared transition setup");
    }
    let mut program = Program::new([256, 128, 256]);
    let mut case = prepared.image.clone();
    case["queue_capacity"] = json!(256);
    program.load(&mut s, &case);
    store.load(
        &mut s,
        &images::reblock(&prepared.image["media"], c.bc, c.slots),
    );
    for (n, v) in [
        (
            "run_instruction_count",
            prepared.image["program"]["instructions"]
                .as_array()
                .unwrap()
                .len() as u64,
        ),
        (
            "run_state_count",
            prepared.image["initial_state"]["memory"]
                .as_array()
                .unwrap()
                .len() as u64,
        ),
        ("run_quantum", c.q),
        ("run_chunk_frames", c.cf),
        (
            "run_job_budget",
            prepared.image["program"]["jobs_per_quantum"]
                .as_u64()
                .unwrap(),
        ),
        ("run_buffers", prepared.image["buffers"].as_u64().unwrap()),
        ("run_inputs", c.channels),
        ("run_outputs", c.channels),
    ] {
        s.model.set(n, v);
    }
    let source = Source::attach(
        &mut s,
        "control_valid",
        "control_ready",
        "control_data",
        prepared.events.clone(),
    );
    let held_reply = hold_reply.then(|| {
        let (error, words) = transition.hold_raw_command(&mut s, 0, 0, &Bits::default(), false);
        assert_eq!(error, 0);
        words
    });
    if hold_reply {
        s.agent(IsolatedService);
        s.model.set_all(&[
            ("command_valid", 1),
            ("cfg_valid", 1),
            ("audio_cfg_valid", 1),
            ("store_cmd_valid", 1),
            ("store_cfg_valid", 1),
            ("store_write_valid", 1),
            ("store_read_ready", 1),
        ]);
    }
    let times = Rc::new(RefCell::new(BTreeMap::new()));
    let measure = Rc::new(RefCell::new(Measure {
        q: c.q,
        times: times.clone(),
        last: 0,
        rows: Vec::new(),
    }));
    s.agent(measure.clone());
    s.model.set("enable", 1);
    let clock = Frames {
        ticks: c.ticks,
        period_ps: c.period_ps,
    };
    clock.settle(&mut s, 5);
    if c.overrun {
        s.agent(Hold {
            q: c.q,
            p: c.p,
            released: false,
        });
    }
    let actual = frames(
        &mut s,
        &clock,
        c,
        &prepared.expected,
        0,
        quanta * c.q + c.p,
        true,
        if c.overrun { &[0] } else { &[] },
        Some(&times),
    );
    s.model.set_all(&[
        ("enable", 0),
        ("command_valid", 0),
        ("cfg_valid", 0),
        ("audio_cfg_valid", 0),
        ("store_cmd_valid", 0),
        ("store_cfg_valid", 0),
        ("store_write_valid", 0),
        ("store_read_ready", 0),
    ]);
    s.until(1_000_000, "continuous processing drain", |m| {
        m.get("busy") == 0
    });
    clock.settle(&mut s, 10);
    source.borrow_mut().enabled = false;
    if let Some(words) = held_reply {
        assert_eq!(s.model.get("response_valid"), 1);
        assert_eq!(s.model.get("response_error"), 0);
        assert_eq!(
            s.model.bits("response_data"),
            words,
            "held reply changed while audio ran"
        );
        s.model.set("response_ready", 1);
        s.idle(1);
        s.model.set("response_ready", 0);
    }
    let processed = s.model.get("processed_quanta");
    assert!(processed >= 12);
    let mut states = transition.states(&mut s);
    for state in &mut states {
        state.as_object_mut().unwrap().remove("revision_watermark");
    }
    assert_eq!(
        json!(states),
        prepared.expected["quanta"][processed as usize - 1]["states"]
    );
    let mut media = images::logical_media(
        &store.snapshot(&mut s),
        &prepared.image["loop_views"],
        &states,
    );
    media.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    assert_eq!(json!(media), prepared.expected["media"]);
    let names: Vec<_> = prepared.scenario["steps"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|step| {
            step["actions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|action| action["name"].clone())
        })
        .collect();
    let mut outcomes = Vec::new();
    for (slot, original) in prepared.request_commands.iter().enumerate() {
        let (error, raw) =
            transition.raw_command(&mut s, 13, slot as u64, &Bits::default(), Some(0), false);
        assert_eq!(error, 0);
        let raw: [u32; 26] = raw.0.try_into().unwrap();
        let words = rt_fpga_adapter::transition_service::wire::result(&raw, Some(original), 1)?;
        let words = Bits(words.to_vec());
        let outcome = words.get(37, 3);
        assert_ne!(outcome, 0, "request remained pending: {}", names[slot]);
        let mut row = json!({"name":names[slot],"requested":words.get(192,64),
            "delivered":words.get(448,64),"terminal":words.get(576,64),
            "outcome":(["pending","applied","superseded","cancelled","failed"][outcome as usize])});
        if outcome == 4 {
            row["reason"] = json!(crate::drivers::transition::ERRORS[words.get(40, 4) as usize]);
        }
        outcomes.push(row);
    }
    outcomes.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    assert_eq!(
        json!(outcomes),
        prepared.expected["quanta"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["outcomes"]
    );
    assert_eq!(s.model.get("missed_deadlines"), u64::from(c.overrun));
    assert_eq!(s.model.get("discarded_quanta"), u64::from(c.overrun));
    let report = json!({"case":{"name":name,"audio_period_ps":c.period_ps,"phase_ps":c.phase_ps,"frame_cycles":c.ticks,"stalls":c.stalls,"overrun":c.overrun},"configuration":c,"wall_seconds":begin.elapsed().as_secs_f64(),"frames":actual,"states":states,"media":media,"outcomes":outcomes,"processed":processed,"missed":s.model.get("missed_deadlines"),"discarded":s.model.get("discarded_quanta"),"rx_high_water":s.model.get("rx_high_water"),"tx_high_water":s.model.get("tx_high_water"),"publication":measure.borrow().rows,"axi":store.memory.borrow_mut().report(true),"control_stalls":source.borrow().stalls,"control_transfers":source.borrow().sent,"control_result_held":!drain_results&&s.model.get("control_result_valid")!=0,"simulated_ps":s.ps()});
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}
