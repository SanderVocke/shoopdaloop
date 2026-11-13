//! Prepared fabric ROMs compiled through the production graph lowerer.
use crate::{
    Result,
    bits::Bits,
    codec,
    drivers::{stream::control_packet, transition},
    images,
};
use serde_json::{Value, json};
use std::path::PathBuf;
pub struct Prepared {
    pub scenario: Value,
    pub image: Value,
    pub expected: Value,
    pub boot: Vec<Bits>,
    pub events: Vec<Bits>,
    pub directory: PathBuf,
}
pub fn scenario(q: u64, cf: u64, channels: u64, monitoring: bool, routing: &str) -> Value {
    let id = |slot| json!({"slot":slot,"generation":1});
    let ep = |node: Value, port| json!({"node":node,"port":port});
    let mut loops = Vec::new();
    let mut nodes = Vec::new();
    let mut connections = Vec::new();
    for c in 0..channels {
        loops.push(
            json!({"id":id(c),"channels":1,"sync_source":null,"repeat_sync":false,"content":null}),
        );
        nodes.extend([
            json!({"id":id(2*c),"operation":{"kind":"playback","target":id(c)}}),
            json!({"id":id(2*c+1),"operation":{"kind":"capture","target":id(c)}}),
        ]);
        let input = match routing {
            "reverse" => channels - 1 - c,
            "fanout" => 0,
            _ => c,
        };
        connections.push(json!({"source":ep(Value::Null,input),"destination":ep(id(2*c+1),0)}));
        if routing != "sparse" || c != channels - 1 {
            connections.push(json!({"source":ep(id(2*c),0),"destination":ep(Value::Null,c)}));
            if monitoring {
                connections
                    .push(json!({"source":ep(Value::Null,input),"destination":ep(Value::Null,c)}));
            }
        }
    }
    let steps: Vec<_> = (0..18).map(|n| {let actions: Vec<_> = (0..channels).filter_map(|c| {let operation = match (n,c%2) {(0,_)|(6,0)=>json!({"kind":"record"}),(2,_)|(8,0)=>json!({"kind":"play","position":0,"retrigger":true}),(5,0)=>json!({"kind":"stop"}),_=>return None};Some(json!({"kind":"transition","name":format!("q{n}-c{c}"),"target":id(c),"at":n*q,"timing":{"kind":"at"},"operation":operation}))}).collect();let inputs: Vec<Vec<i32>> = (0..channels).map(|c| (0..q).map(|f| ((c+1)*100000+(n*q+f)*37+1) as i32).collect()).collect();json!({"inputs":inputs,"actions":actions,"collect_completed":false})}).collect();
    json!({"format_version":1,"name":format!("audio-io-{channels}x{channels}-q{q}"),"quantum":q,"chunk_frames":cf,"input_ports":channels,"output_ports":channels,"request_capacity":128,"loops":loops,"nodes":nodes,"connections":connections,"media":[],"steps":steps})
}
pub fn prepare(scenario: Value, bc: u64, slots: u64) -> Result<Prepared> {
    prepare_with_views(scenario, bc, slots, 32)
}
pub fn prepare_with_views(scenario: Value, bc: u64, slots: u64, views: u64) -> Result<Prepared> {
    let crate::reference::Reference {
        image,
        fixed: expected,
        floating,
        audit,
    } = crate::reference::Reference::new(&scenario)?;
    let t = &image["transition"];
    let limits = &t["limits"];
    let mut boot = Vec::new();
    let loops = t["description"]["loops"].as_array().unwrap();
    let q = scenario["quantum"].as_u64().unwrap();
    for i in 0..limits["loop_slots"].as_u64().unwrap() {
        let spec = loops
            .iter()
            .find(|v| v["id"]["slot"] == i)
            .unwrap_or(&Value::Null);
        boot.push(control_packet(0, 0, &transition::spec(spec), 1, i));
    }
    let order = t["dependency_order"].as_array().unwrap();
    for (i, id) in order.iter().enumerate() {
        boot.push(control_packet(
            0,
            0,
            &Bits::from_u64(id["slot"].as_u64().unwrap()),
            2,
            i as u64,
        ));
    }
    let mut data = Bits::zero(640);
    for (i, v) in [
        limits["loop_slots"].as_u64().unwrap(),
        limits["pending_requests"].as_u64().unwrap(),
        limits["events"].as_u64().unwrap(),
        order.len() as u64,
        q,
    ]
    .into_iter()
    .enumerate()
    {
        data.set(i * 32, 32, v);
    }
    boot.push(control_packet(0, 0, &data, 3, 0));
    for (i, v) in image["program"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        boot.push(control_packet(0, 1, &codec::instruction(v)?, 0, i as u64));
    }
    for (i, v) in image["initial_state"]["memory"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        boot.push(control_packet(
            0,
            1,
            &codec::state(v),
            0,
            i as u64 | (1 << 31),
        ));
    }
    for i in 0..views {
        boot.push(control_packet(0, 2, &Bits::default(), 1, i));
    }
    for (i, v) in [0, 0, 0, slots / bc].into_iter().enumerate() {
        boot.push(control_packet(0, 2, &Bits::from_u64(v), 0, i as u64));
    }
    boot.push(control_packet(0, 3, &Bits::default(), 0, 0));
    let mut events = Vec::new();
    let mut sequence = 0;
    for (n, step) in scenario["steps"].as_array().unwrap().iter().enumerate() {
        for action in step["actions"].as_array().unwrap() {
            sequence += 1;
            let mut c = action.clone();
            c["sequence"] = json!(sequence);
            events.push(control_packet(n as u64, 0, &transition::command(&c), 4, 0));
        }
        events.push(control_packet(n as u64, 2, &Bits::default(), 0, 0));
    }
    use std::hash::{Hash, Hasher};
    let mut hash = std::hash::DefaultHasher::new();
    scenario.to_string().hash(&mut hash);
    bc.hash(&mut hash);
    slots.hash(&mut hash);
    views.hash(&mut hash);
    let directory =
        crate::root().join(format!("work/marlin-suite/prepared/{:016x}", hash.finish()));
    std::fs::create_dir_all(&directory)?;
    for (name, value) in [
        ("scenario", &scenario),
        ("image", &image),
        ("expected", &expected),
        ("float", &floating),
        ("audit", &audit),
    ] {
        std::fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_vec_pretty(value)?,
        )?;
    }
    for (name, values) in [("boot", &boot), ("events", &events)] {
        let text: String = values
            .iter()
            .map(|v| {
                let wide = v.resized(768);
                let text = wide
                    .0
                    .iter()
                    .rev()
                    .map(|v| format!("{v:08x}"))
                    .collect::<String>();
                format!("{}\n", &text[text.len() - 187..])
            })
            .collect();
        let path = directory.join(format!("{name}.hex"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(&text) {
            std::fs::write(path, text)?;
        }
    }
    let _ = images::reblock(&image["media"], bc, slots);
    Ok(Prepared {
        scenario,
        image,
        expected,
        boot,
        events,
        directory,
    })
}
