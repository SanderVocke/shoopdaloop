//! Common logical sessions run independently through production Rust and the complete RTL.
use crate::{
    Result,
    bits::Bits,
    codec,
    drivers::{loops::Loops, store::Geometry, transition},
    images,
    memory::Mode,
    model::{Design, Probe},
    sim::Sim,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub const DEFAULT: [u64; 10] = [8, 16, 32, 64, 32, 64, 32, 32, 4, 4];
pub fn design(depths: [u64; 10], g: &Geometry, width: u64, mode: Mode) -> Design {
    let root = crate::root();
    let mut d = super::pipeline::design(depths[3..].try_into().unwrap(), g, width, mode, 1);
    d.top = if mode == Mode::Legacy {
        "loop_pipeline"
    } else {
        "loop_pipeline_axi"
    }
    .into();
    d.sources
        .extend(super::transition::design(depths[..3].try_into().unwrap()).sources);
    d.sources
        .push(root.join("fpga/loop_pipeline/rtl/loop_pipeline.sv"));
    if mode != Mode::Legacy {
        d.sources
            .push(root.join("fpga/axi_memory/rtl/loop_pipeline_axi.sv"));
    }
    d.sources.sort();
    d.sources.dedup();
    for (n, v) in ["LOOP_DEPTH", "REQUEST_DEPTH", "EVENT_DEPTH"]
        .into_iter()
        .zip(depths)
    {
        d.parameters.insert(n.into(), v.to_string());
    }
    let base = if mode == Mode::Legacy { "" } else { "core." };
    let gc = format!("{base}audio.media.reclaimer");
    d.probes = [
        format!("{base}phase"),
        format!("{gc}.phase"),
        format!("{gc}.cycle_budget"),
        format!("{gc}.maintenance_charge"),
        format!("{gc}.lock_held"),
        format!("{gc}.lock_request"),
        format!("{gc}.lock_grant"),
        format!("{gc}.request_valid"),
        format!("{gc}.request_ready"),
        format!("{gc}.retire_valid"),
        format!("{gc}.retire_ready"),
    ]
    .iter()
    .map(|n| Probe::observe(n))
    .collect();
    d.probes
        .extend(crate::drivers::execution::probes(mode != Mode::Legacy));
    let mut seen = std::collections::BTreeSet::new();
    d.probes.retain(|p| seen.insert(p.path.clone()));
    d
}
pub fn physical(image: &Value, depths: [u64; 10], g: &Geometry) -> Value {
    let mut case = image.clone();
    case["queue_capacity"] = json!(depths[5]);
    case["media"] = images::reblock(&image["media"], g.block_chunks, g.slots);
    case
}
fn completion(
    value: &Value,
    names: &BTreeMap<u64, String>,
    submitted: &BTreeMap<String, (u64, u64)>,
    q: u64,
) -> Value {
    let name = &names[&value["sequence"].as_u64().unwrap()];
    let (requested, at) = submitted[name];
    let delivered = at.max(requested.div_ceil(q) * q);
    let terminal = value["outcome"]["frame"].as_u64().unwrap();
    let kind = value["outcome"]["kind"].as_str().unwrap();
    let outcome = if kind == "failed" {
        format!("failed:{}", value["outcome"]["reason"].as_str().unwrap())
    } else {
        kind.into()
    };
    json!({"name":name,"requested":requested,"delivered":if kind == "cancelled" && terminal<delivered {None}else{Some(delivered)},"terminal":terminal,"outcome":outcome})
}
fn collect(
    d: &mut Loops,
    s: &mut Sim<'_>,
    names: &BTreeMap<u64, String>,
    submitted: &BTreeMap<String, (u64, u64)>,
    q: u64,
) -> Vec<Value> {
    d.collect_completed(s)
        .iter()
        .map(|r| {
            assert_eq!(r["result"]["kind"], "accepted");
            completion(&r["result"]["value"], names, submitted, q)
        })
        .collect()
}
fn sort_names(rows: &mut [Value]) {
    rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
}
pub fn production_scenarios(
    d: &mut Loops,
    s: &mut Sim<'_>,
    scenario: &Value,
    image: &Value,
    depths: [u64; 10],
) -> Value {
    let execution = std::rc::Rc::new(std::cell::RefCell::new(
        crate::drivers::execution::Execution::new(
            d.pipeline.store.memory.clone(),
            d.pipeline.store.mode != Mode::Legacy,
        ),
    ));
    s.agent(execution.clone());
    let mut timings = Vec::new();
    let q = scenario["quantum"].as_u64().unwrap();
    let case = physical(image, depths, &d.pipeline.store.geometry);
    d.initialize(s, &case, &image["transition"]);
    let mut actual =
        json!({"format_version":1,"name":scenario["name"],"numeric":"q824","quanta":[],"media":[]});
    let mut names = BTreeMap::new();
    let mut submitted = BTreeMap::new();
    let mut sequence = 0;
    let mut gains = Vec::<Value>::new();
    let mut held = Vec::new();
    let mut admissions = Vec::new();
    let mut ids = BTreeMap::new();
    let mut exports = Vec::<(Value, Vec<u64>)>::new();
    let mut last = Value::Null;
    let mut states = Vec::new();
    for (number, step) in scenario["steps"].as_array().unwrap().iter().enumerate() {
        let frame = number as u64 * q;
        let mut outcomes = Vec::new();
        if step["collect_completed"] == true {
            outcomes = collect(d, s, &names, &submitted, q);
            outcomes.append(&mut held);
        }
        outcomes.append(&mut admissions);
        for action in step["actions"].as_array().unwrap() {
            sequence += 1;
            let name = action["name"].as_str().unwrap().to_owned();
            names.insert(sequence, name.clone());
            submitted.insert(
                name.clone(),
                (action["at"].as_u64().unwrap_or(frame), frame),
            );
            match action["kind"].as_str().unwrap() {
                "transition" => {
                    let mut c = action.clone();
                    c["sequence"] = json!(sequence);
                    let (error, value) = d.submit(s, &c);
                    if error == 2 {
                        admissions.push(json!({"name":name,"requested":action["at"],"delivered":null,"terminal":frame,"outcome":"rejected:full"}));
                    } else if error != 0 {
                        held.push(json!({"name":name,"requested":action["at"],"delivered":frame,"terminal":frame,"outcome":format!("failed:{}",transition::ERRORS[error as usize])}));
                    } else {
                        ids.insert(name, transition::decode_identity(value.get(0, 64)));
                    }
                }
                "gain" => gains.push(action.clone()),
                "cancel" => {
                    let id = &ids[action["request"].as_str().unwrap()];
                    let (error, _) = d.transition.command(
                        s,
                        5,
                        id["slot"].as_u64().unwrap(),
                        &Bits::from_u64(id["generation"].as_u64().unwrap()),
                        Some(0),
                        false,
                    );
                    held.push(json!({"name":name,"requested":frame,"delivered":frame,"terminal":frame,"outcome":if error == 0 {"applied".into()}else{format!("failed:{}",transition::ERRORS[error as usize])}}));
                }
                "export" => {
                    let binding = image["loop_views"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|v| v["id"] == action["target"])
                        .unwrap();
                    let mut views = Vec::new();
                    for view in binding["views"].as_array().unwrap() {
                        let view = view.as_u64().unwrap();
                        let frames = d
                            .pipeline
                            .store
                            .configure(s, 1, view, None, false)
                            .get(64, 64);
                        let r = d.pipeline.store.command(
                            s,
                            &json!({"kind":"slice","source":view,"start":0,"frames":frames}),
                            d.stalls,
                            false,
                            true,
                        );
                        assert_eq!(r["status"]["kind"], "completed");
                        views.push(r["value"]["value"].as_u64().unwrap());
                    }
                    exports.push((action["name"].clone(), views));
                    held.push(json!({"name":name,"requested":frame,"delivered":frame,"terminal":frame,"outcome":"applied"}));
                }
                kind => panic!("unsupported production action {kind}"),
            }
        }
        let mut future = Vec::new();
        for action in gains.drain(..) {
            if action["at"].as_u64().unwrap() > frame {
                future.push(action);
                continue;
            }
            let binding = image["gains"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["id"] == action["node"])
                .unwrap();
            for address in binding["ramps"].as_array().unwrap() {
                let address = address.as_u64().unwrap();
                let old = codec::decode_state(
                    &d.pipeline
                        .audio
                        .program
                        .configure(s, true, address, None, false),
                );
                let old = &old["value"];
                let start = old["start"].as_i64().unwrap();
                let target = old["target"].as_i64().unwrap();
                let duration = old["duration"].as_u64().unwrap();
                let elapsed = old["elapsed"].as_u64().unwrap();
                let start = if duration == 0 {
                    target
                } else {
                    (i128::from(start)
                        + (i128::from(target) - i128::from(start))
                            * i128::from(elapsed.min(duration))
                            / i128::from(duration)) as i64
                };
                let value = json!({"kind":"ramp","value":{"start":start,"target":action["target"],"duration":action["duration"],"elapsed":0}});
                d.pipeline.audio.program.configure(
                    s,
                    true,
                    address,
                    Some(&codec::state(&value)),
                    false,
                );
            }
            held.push(json!({"name":action["name"],"requested":action["at"],"delivered":frame,"terminal":frame,"outcome":"applied"}));
        }
        gains = future;
        execution.borrow_mut().begin();
        last = d.step(s,&json!({"transition":{"collect_completed":false,"collect":[],"cancel":[],"commands":[]},"writes":[],"work":{"kind":"audio","value":{"inputs":step["inputs"]}}}),true);
        timings.push(execution.borrow().finish());
        assert!(
            last["error"].is_null(),
            "{} quantum {number}: {}",
            scenario["name"],
            last["error"]
        );
        states = scenario["loops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|logical| {
                let mut state = last["transition"]["states"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["id"] == logical["id"])
                    .unwrap()
                    .clone();
                state.as_object_mut().unwrap().remove("revision_watermark");
                state
            })
            .collect();
        let mut pending: Vec<_> = last["transition"]["requests"].as_array().unwrap().iter().filter(|r| !r.is_null() && r["outcome"].is_null() && r["effective"].as_u64().unwrap() <= frame).map(|r| json!({"name":names[&r["command"]["sequence"].as_u64().unwrap()],"requested":r["command"]["at"],"effective":r["effective"],"remaining_ticks":r["remaining_ticks"]})).collect();
        sort_names(&mut pending);
        sort_names(&mut outcomes);
        actual["quanta"].as_array_mut().unwrap().push(json!({"frame":frame,"next_frame":last["transition"]["next_frame"],"outputs":{"encoding":"q824","channels":last["audio"]["outputs"]},"states":states,"outcomes":outcomes,"events":last["transition"]["events"],"pending":pending}));
    }
    let outcomes = actual["quanta"].as_array_mut().unwrap().last_mut().unwrap()["outcomes"]
        .as_array_mut()
        .unwrap();
    outcomes.extend(collect(d, s, &names, &submitted, q));
    outcomes.extend(held);
    outcomes.extend(admissions);
    sort_names(outcomes);
    let mut media = images::logical_media(&last["audio"]["media"], &image["loop_views"], &states);
    for (name, views) in exports {
        let mut channels = Vec::new();
        for view in views {
            let frames = d
                .pipeline
                .store
                .configure(s, 1, view, None, false)
                .get(64, 64);
            let r = d.pipeline.store.command(
                s,
                &json!({"kind":"read","view":view,"position":0,"length":frames,"frames":frames}),
                d.stalls,
                false,
                true,
            );
            assert_eq!(r["status"]["kind"], "completed");
            channels.push(r["value"]["value"].clone());
        }
        media.push(json!({"name":name,"frames":channels[0].as_array().unwrap().len(),"samples":{"encoding":"q824","channels":channels}}));
    }
    sort_names(&mut media);
    actual["media"] = json!(media);
    let directory = crate::artifact_directory("execution-timing");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("timing.json"),
        serde_json::to_vec_pretty(&json!({"name":scenario["name"],"quanta":timings})).unwrap(),
    )
    .unwrap();
    actual
}
fn check_image(image: &Value, depths: [u64; 10], g: &Geometry) -> Result<()> {
    let needs = [
        image["transition"]["limits"]["loop_slots"]
            .as_u64()
            .unwrap(),
        image["transition"]["limits"]["pending_requests"]
            .as_u64()
            .unwrap(),
        image["transition"]["limits"]["events"].as_u64().unwrap(),
        image["program"]["instructions"].as_array().unwrap().len() as u64,
        image["initial_state"]["memory"].as_array().unwrap().len() as u64,
        image["program"]["jobs_per_quantum"].as_u64().unwrap(),
        image["program"]["quantum"].as_u64().unwrap(),
        image["buffers"].as_u64().unwrap(),
        image["input_ports"].as_u64().unwrap(),
        image["output_ports"].as_u64().unwrap(),
    ];
    for ((name, need), available) in [
        "LOOP_DEPTH",
        "REQUEST_DEPTH",
        "EVENT_DEPTH",
        "PROGRAM_DEPTH",
        "STATE_DEPTH",
        "JOB_DEPTH",
        "MAX_FRAMES",
        "BUFFERS",
        "INPUT_PORTS",
        "OUTPUT_PORTS",
    ]
    .into_iter()
    .zip(needs)
    .zip(depths)
    {
        if need > available {
            return Err(format!("{}: {name} needs {need}, has {available}", image["name"]).into());
        }
    }
    if image["media"]["views"].as_array().unwrap().len() as u64 > g.views {
        return Err(format!("{}: view capacity exceeded", image["name"]).into());
    }
    Ok(())
}
pub fn run(
    scenarios: &[Value],
    depths: [u64; 10],
    g: Geometry,
    width: u64,
    mode: Mode,
) -> Result<()> {
    let compiled = design(depths, &g, width, mode)
        .build(&crate::root().join("work/marlin-suite/production"))?;
    for scenario in scenarios {
        println!("production/AXI{width}/{mode:?}/{}", scenario["name"]);
        let crate::reference::Reference {
            image,
            fixed: expected,
            floating,
            audit,
        } = crate::reference::Reference::new(scenario)?;
        check_image(&image, depths, &g)?;
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Loops::new(&mut s, depths, g.clone(), mode, 20261112)?;
        let actual = production_scenarios(&mut d, &mut s, scenario, &image, depths);
        let output = crate::artifact_directory("production-results");
        std::fs::create_dir_all(&output)?;
        std::fs::write(
            output.join("actual.json"),
            serde_json::to_vec_pretty(&actual)?,
        )?;
        std::fs::write(
            output.join("expected.json"),
            serde_json::to_vec_pretty(&expected)?,
        )?;
        for (name, value) in [
            ("scenario", scenario),
            ("image", &image),
            ("float", &floating),
            ("audit", &audit),
        ] {
            std::fs::write(
                output.join(format!("{name}.json")),
                serde_json::to_vec_pretty(value)?,
            )?;
        }
        assert_eq!(
            actual,
            expected,
            "{}: exact production comparison; artifacts {}",
            scenario["name"],
            output.display()
        );
        d.pipeline.store.memory.borrow_mut().report(true);
    }
    Ok(())
}
