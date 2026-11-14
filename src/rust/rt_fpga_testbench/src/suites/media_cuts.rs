//! Cut-time ownership against a stalled physical-view allocator, without an ARM observer.
use crate::{
    Result,
    bits::Bits,
    clock::Edges,
    model::{Design, Model, Probe},
    sim::{Agent, Sim},
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
const NONE: u64 = u32::MAX as u64;
#[derive(Default)]
struct Views {
    live: BTreeSet<u32>,
    pending: Option<(u64, u32)>,
    cycle: u64,
    creates: usize,
    removes: usize,
}
impl Agent for Views {
    fn drive(&mut self, m: &mut Model<'_>, e: Edges) {
        if e.rises(0) {
            m.set(
                "media_ready",
                u64::from(self.pending.is_none() && self.cycle % 7 != 0),
            );
            m.set(
                "media_done",
                u64::from(self.pending.is_some_and(|(at, _)| at == self.cycle)),
            );
            m.set("media_error", 0);
            m.set(
                "media_result_view",
                self.pending.map_or(0, |(_, id)| u64::from(id)),
            );
        }
    }
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if !e.rises(0) {
            return;
        }
        if m.get("media_done") != 0 {
            self.pending = None;
        }
        if m.get("media_valid") != 0 && m.get("media_ready") != 0 {
            let id = match m.get("media_opcode") {
                0 => {
                    let id = (0..16)
                        .find(|id| !self.live.contains(id))
                        .expect("view pool exceeded");
                    self.live.insert(id);
                    self.creates += 1;
                    id
                }
                3 => {
                    let id = m.get("media_view") as u32;
                    assert!(self.live.remove(&id), "double remove {id}");
                    self.removes += 1;
                    id
                }
                op => panic!("cut-time ownership performed unbounded store operation {op}"),
            };
            self.pending = Some((self.cycle + 3, id));
        }
        self.cycle += 1;
    }
}
fn request(s: &mut Sim<'_>, op: u64, latest: bool, slot: u64, generation: u64) -> (u64, Bits) {
    s.until(1000, "media cut ready", |m| m.get("request_ready") != 0);
    s.model.set_all(&[
        ("request_valid", 1),
        ("request_opcode", op),
        ("request_latest", u64::from(latest)),
        ("request_slot", slot),
        ("request_generation", generation),
    ]);
    let (before, _) = s.tick(&["request_ready"]);
    assert_eq!(before.get("request_ready"), 1);
    s.model.set("request_valid", 0);
    s.until(1000, "bounded media cut completion", |m| {
        m.get("response_valid") != 0
    });
    let result = (s.model.get("response_error"), s.model.bits("response_data"));
    s.idle(1);
    result
}
fn observe(
    s: &mut Sim<'_>,
    mode: u64,
    content: u64,
    capture: u64,
    frames: u64,
    length: u64,
    frame: u64,
) -> Bits {
    let mut state = Bits::zero(832);
    state
        .set(0, 1, 1)
        .set(1, 32, 1)
        .set(33, 2, mode)
        .set(64, 64, length)
        .set(192, 64, frames)
        .set(320, 64, content)
        .set(384, 64, capture);
    s.model.set_bits("request_state", &state);
    s.model
        .set_all(&[("request_frame", frame), ("request_epoch", 7)]);
    let (error, row) = request(s, 0, false, 0, 0);
    assert_eq!(error, 0);
    assert_eq!(row.get(832, 64), frame);
    assert_eq!(row.get(896, 64), 7);
    assert_eq!(&row.0[..26], &state.0[..]);
    row
}
pub fn ownership() -> Result<()> {
    let root = crate::root();
    let design = Design::new(
        "media_cuts",
        vec![
            root.join("fpga/loop_pipeline/rtl/media_cuts.sv"),
            root.join("fpga/state_processor/rtl/sp_ram.sv"),
        ],
    )
    .probes(vec![
        Probe::inject("bank0"),
        Probe::inject("empty_generation"),
    ])
    .build(&root.join("work/marlin-suite/media-cuts"))?;
    let mut s = Sim::new(design.session(None)?, &[("clk", 10000, 0)]);
    let views = Rc::new(RefCell::new(Views::default()));
    s.agent(views.clone());
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(12);
    observe(&mut s, 0, 0, 0, 0, 0, 0);
    assert_eq!(
        request(&mut s, 1, true, 0, 0).0,
        5,
        "latest selection must fail before export capacity"
    );
    for slot in 0..4 {
        let (error, row) = request(&mut s, 1, false, 0, 0);
        assert_eq!(error, 0);
        assert_eq!(row.get(1088, 32), slot);
        assert_eq!(row.get(1120, 32), 1);
        assert_eq!(row.get(1600, 32), NONE);
        assert_eq!(row.get(1632, 32), 1);
    }
    assert_eq!(request(&mut s, 1, false, 0, 0).0, 2);
    assert_eq!(request(&mut s, 1, true, 0, 0).0, 5);
    for slot in 1..4 {
        assert_eq!(request(&mut s, 2, false, slot, 1).0, 0);
    }
    observe(&mut s, 2, 0, 1, 0, 0, 64);
    let first_roots = (s.model.get("capture_view0"), s.model.get("capture_view1"));
    assert_ne!(first_roots.0, first_roots.1);
    observe(&mut s, 2, 0, 1, 64, 0, 128);
    let row = observe(&mut s, 0, 1, 1, 64, 64, 128);
    assert_eq!(row.get(960, 32), 0);
    assert_eq!(row.get(1024, 32), 0);
    assert_eq!(row.get(1216, 64), 64);
    assert_eq!(row.get(1344, 64), 64);
    assert_eq!(row.get(1408, 64), 128);
    assert_eq!(row.get(1472, 64), 3);
    assert_eq!(row.get(1280, 32), first_roots.0);
    assert_eq!(row.get(1312, 32), first_roots.1);
    let (error, pinned) = request(&mut s, 1, false, 0, 0);
    assert_eq!(error, 0);
    assert_eq!(pinned.get(1088, 32), 1);
    assert_eq!(pinned.get(1120, 32), 2);
    // Device cuts continue with no pin/release/lookup service during 1,050 quanta.
    let mut frame = 128;
    for take in 2..=76 {
        frame += 32;
        observe(&mut s, 2, take - 1, take, 0, 64, frame);
        for quantum in 1..=12 {
            frame += 32;
            observe(&mut s, 2, take - 1, take, quantum * 32, 64, frame);
        }
        frame += 32;
        observe(&mut s, 0, take, take, 384, 384, frame);
        let v = views.borrow();
        assert!(v.live.contains(&(first_roots.0 as u32)));
        assert!(v.live.contains(&(first_roots.1 as u32)));
        assert!(v.live.len() <= 6, "retakes leaked roots");
    }
    let (error, retained) = request(&mut s, 3, false, 1, 2);
    assert_eq!(error, 0);
    for (offset, width) in [
        (1152, 64),
        (1216, 64),
        (1280, 32),
        (1312, 32),
        (1344, 64),
        (1408, 64),
        (1472, 64),
        (1536, 32),
        (1600, 64),
        (1664, 64),
    ] {
        assert_eq!(
            retained.get(offset, width),
            pinned.get(offset, width),
            "historical descriptor changed at {offset}"
        );
    }
    let (error, empty) = request(&mut s, 3, false, 0, 1);
    assert_eq!(error, 0);
    assert_eq!(empty.get(1152, 64), 0);
    assert_eq!(empty.get(1632, 32), 1);
    assert_eq!(
        request(&mut s, 2, false, 1, 1).0,
        3,
        "stale export released current root"
    );
    assert_eq!(request(&mut s, 2, false, 1, 2).0, 0);
    assert!(!views.borrow().live.contains(&(first_roots.0 as u32)));
    assert_eq!(request(&mut s, 3, false, 1, 2).0, 3);
    assert_eq!(
        s.model.get("fault"),
        0,
        "ordinary service refusal faulted autonomous cuts"
    );
    // Work is independent of take length: no reference-vector slice at finalization.
    let before = views.borrow().creates;
    observe(&mut s, 2, 76, 77, 0, 384, frame + 32);
    observe(&mut s, 2, 76, 77, 240_000, 384, frame + 240_032);
    let start = s.cycles;
    let long = observe(&mut s, 0, 77, 77, 240_000, 240_000, frame + 240_032);
    assert!(s.cycles - start < 250);
    assert_eq!(views.borrow().creates, before + 2);
    assert_eq!(long.get(1216, 64), 240_000);
    let empty_cut = observe(&mut s, 0, 88, 77, 240_000, 0, frame + 240_064);
    assert_eq!(empty_cut.get(1632, 32), 2);
    let (error, historical_empty) = request(&mut s, 3, false, 0, 1);
    assert_eq!(error, 0);
    assert_eq!(historical_empty.get(1152, 64), 0);
    assert_eq!(historical_empty.get(1632, 32), 1);
    s.model.set("request_epoch", 6);
    assert_eq!(request(&mut s, 2, false, 0, 1).0, 3);
    s.model.set("request_epoch", 7);
    assert_eq!(request(&mut s, 3, false, 0, 1).0, 0);
    s.model
        .inject("empty_generation", Some(&Bits::from_u64(u32::MAX.into())));
    let mut state = Bits::zero(832);
    state.set(0, 1, 1).set(1, 32, 1).set(320, 64, 89);
    s.model.set_bits("request_state", &state);
    assert_eq!(request(&mut s, 0, false, 0, 0).0, 8);
    assert_eq!(
        s.model.get("fault"),
        1,
        "cut-time exhaustion must contain audio"
    );
    s.model.inject("empty_generation", None);
    s.model.set("reset", 1);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(12);
    assert_eq!(s.model.get("fault"), 0);
    assert_eq!(
        request(&mut s, 3, false, 0, 1).0,
        3,
        "reset retained an old lease"
    );
    println!(
        "media cuts: 1,050 unserviced device cuts, retained export/empty identities, bounded 240k-frame finalization; {} creates / {} removes",
        views.borrow().creates,
        views.borrow().removes
    );
    Ok(())
}

struct Snapshots(Rc<RefCell<Vec<Bits>>>);
impl Agent for Snapshots {
    fn sample(&mut self, m: &Model<'_>, e: Edges) {
        if e.rises(0) && m.get("cut_snapshot_valid") != 0 {
            self.0.borrow_mut().push(m.bits("cut_snapshot_data"));
        }
    }
}
pub fn samples() -> Result<()> {
    use crate::{
        drivers::{
            audio::Audio,
            config::Config,
            store::Geometry,
            transition::{self as wire, Transition},
        },
        memory::{Memory, Mode},
    };
    use serde_json::json;
    let output = crate::artifact_directory("media-cuts");
    std::fs::create_dir_all(&output)?;
    let id = |slot| json!({"slot":slot,"generation":1});
    let transitions = [
        (64, "record"),
        (128, "play"),
        (256, "record"),
        (320, "play"),
    ];
    let actions:Vec<_>=transitions.iter().enumerate().map(|(n,(at,kind))|json!({"kind":"transition","name":format!("a{n}"),"target":id(0),"at":at,"timing":{"kind":"at"},"operation":if *kind=="play" {json!({"kind":"play","position":0,"retrigger":true})} else {json!({"kind":"record"})}})).collect();
    let steps:Vec<_>=(0..1050).map(|q|json!({"inputs":(0..2).map(|channel|(0..32).map(|i|(channel+1)*100003+q*1013+i*37).collect::<Vec<_>>()).collect::<Vec<_>>(),"actions":if q==0 {actions.clone()} else {vec![]},"collect_completed":false})).collect();
    let scenario = json!({"format_version":1,"name":"device-media-cuts-stereo","quantum":32,"chunk_frames":32,
        "input_ports":2,"output_ports":2,"request_capacity":16,
        "loops":[{"id":id(0),"channels":2,"sync_source":null,"repeat_sync":false,"content":null}],
        "nodes":[{"id":id(0),"operation":{"kind":"playback","target":id(0)}},{"id":id(1),"operation":{"kind":"capture","target":id(0)}}],
        "connections":(0..2).flat_map(|port|[json!({"source":{"node":null,"port":port},"destination":{"node":id(1),"port":port}}),json!({"source":{"node":id(0),"port":port},"destination":{"node":null,"port":port}})]).collect::<Vec<_>>(),
        "media":[],"steps":steps});
    let reference = crate::reference::Reference::new(&scenario)?;
    let depths = [1, 16, 18, 64, 32, 64, 32, 16, 2, 2];
    let mut g = Geometry::new([32, 1024, 32], 32);
    g.reuse_views = true;
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
        let compiled = super::production::design(depths, &g, 64, mode)
            .parameters(&[("CAPTURE_CHANNELS", 2)])
            .build(&crate::root().join("work/marlin-suite/autonomous-media"))?;
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
        let memory = Memory::attach(&mut s, mode, g.words(), 0x01000000, &output, 20261114)?;
        memory.borrow_mut().control.stalls = mode != Mode::Instant;
        Memory::fill(&memory, &mut s, 0xa5a5a5a5);
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        s.agent(Snapshots(snapshots.clone()));
        let mut d = Transition::new([1, 16, 18]);
        d.busy_signal = "transition_busy";
        d.load(&mut s, &reference.image["transition"]);
        s.model.set_all(&[
            ("job_enable", 1),
            ("maintenance_enable", 1),
            ("arena_base", 0),
        ]);
        let mut audio = Audio::coordinated(&mut s, depths[3..].try_into().unwrap(), 511);
        let mut image = reference.image.clone();
        image["queue_capacity"] = json!(64);
        audio.program.load(&mut s, &image);
        audio.frames = 32;
        audio.outputs = 2;
        audio.buffers = image["buffers"].as_u64().unwrap();
        for (n, (at, kind)) in transitions.iter().enumerate() {
            let command = super::transition::command(
                0,
                n as u64 + 1,
                if *kind == "play" {
                    super::transition::play(0, true)
                } else {
                    json!({"kind":"record"})
                },
                *at,
                None,
                0,
            );
            let mut command = command;
            command["target"]["generation"] = json!(1);
            d.ok(&mut s, 4, 0, &wire::command(&command));
        }
        let config = Config::new("audio_cfg", "audio_cfg_space");
        let mut actual = vec![];
        for (q, step) in scenario["steps"].as_array().unwrap().iter().enumerate() {
            for channel in 0..2 {
                for i in 0..32 {
                    let value = step["inputs"][channel][i].as_i64().unwrap() as i32;
                    config.access(
                        &mut s,
                        0,
                        (channel * 32 + i) as u64,
                        Some(&Bits::from_u64(value as u32 as u64)),
                        false,
                    );
                }
            }
            s.model.set_all(&[
                ("run_buffers", audio.buffers),
                ("run_inputs", 2),
                ("run_outputs", 2),
            ]);
            audio.program.run_inputs(&mut s, &[]);
            s.until(20000, "managed audio ready", |m| m.get("run_ready") != 0);
            s.idle(1);
            s.model.set("run_valid", 0);
            s.until(100000, "managed audio done", |m| m.get("done") != 0);
            assert_eq!(
                (s.model.get("error_stage"), s.model.get("error_code")),
                (0, 0),
                "q={q} mode={mode:?}"
            );
            let output: Vec<Vec<i32>> = (0..2)
                .map(|channel| {
                    (0..32)
                        .map(|i| {
                            config
                                .access(&mut s, 1, channel * 32 + i, None, false)
                                .get(0, 32) as u32 as i32
                        })
                        .collect()
                })
                .collect();
            assert_eq!(
                json!(output),
                reference.fixed["quanta"][q]["outputs"]["channels"],
                "q={q} mode={mode:?}"
            );
            actual.push(output);
        }
        let rows = snapshots.borrow();
        assert_eq!(rows.len(), 2100);
        for (q, pair) in rows.chunks_exact(2).enumerate() {
            assert_eq!(pair[0].get(832, 64), q as u64 * 32);
            assert_eq!(pair[1].get(832, 64), (q as u64 + 1) * 32);
            assert_eq!(
                pair[1].get(192, 64),
                reference.fixed["quanta"][q]["states"][0]["capture_frames"]
                    .as_u64()
                    .unwrap()
            );
            let expected = &reference.fixed["quanta"][q]["states"][0];
            for (field, offset) in [
                ("content_length", 64),
                ("position", 128),
                ("cycles", 256),
                ("content_revision", 320),
                ("capture_revision", 384),
            ] {
                assert_eq!(
                    pair[1].get(offset, 64),
                    expected[field].as_u64().unwrap(),
                    "{field} q={q} mode={mode:?}"
                );
            }
            assert_eq!(
                pair[1].get(33, 2),
                match expected["mode"].as_str().unwrap() {
                    "stopped" => 0,
                    "playing" => 1,
                    "recording" => 2,
                    mode => panic!("unknown mode {mode}"),
                }
            );
            assert_eq!(
                pair[1].get(35, 1) != 0,
                expected["repeat_sync"].as_bool().unwrap()
            );
            assert_eq!(
                (pair[1].get(36, 1) != 0).then(|| pair[1].get(512, 64)),
                expected["last_pulse"].as_u64()
            );
        }
        memory.borrow_mut().report(true);
        std::fs::write(
            output.join(format!("samples-{mode:?}.json")),
            serde_json::to_vec(
                &json!({"memory":format!("{mode:?}"),"quanta":1050,"outputs":actual,"cuts":rows.iter().map(|r|r.0.clone()).collect::<Vec<_>>()}),
            )?,
        )?;
        println!(
            "managed media/{mode:?}: 1,050 quanta, 33,600 exact stereo frames and 2,100 coherent cuts; no host TP/media/observation service after initial admission"
        );
    }
    Ok(())
}

pub fn retained_records() -> Result<()> {
    let root = crate::root();
    for slots in [3, 256] {
        let compiled = Design::new(
            "cut_records",
            vec![
                root.join("fpga/loop_pipeline/rtl/cut_records.sv"),
                root.join("fpga/state_processor/rtl/sp_ram.sv"),
            ],
        )
        .parameters(&[("SLOTS", slots)])
        .build(&root.join("work/marlin-suite/cut-records"))?;
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10000, 0)]);
        s.model.set_all(&[("reset", 1), ("epoch", 7)]);
        s.idle(2);
        s.model.set("reset", 0);
        s.idle(1);
        let value = |slot: u64| {
            let mut data = Bits::zero(1856);
            for word in 0..29 {
                data.set(
                    word * 64,
                    64,
                    (slot << 48) | ((word as u64) << 32) | 0xa58724d3,
                );
            }
            data
        };
        let commit = |s: &mut Sim<'_>, slot: u64, generation: u64, data: &Bits| {
            s.model.set_all(&[
                ("commit_valid", 1),
                ("commit_slot", slot),
                ("commit_generation", generation),
            ]);
            s.model.set_bits("commit_data", data);
            s.until(10, "snapshot capture admission", |m| {
                m.get("commit_ready") != 0
            });
            s.idle(1);
            s.model.set("commit_valid", 0);
            let start = s.cycles;
            s.until(33, "bounded snapshot copy", |m| m.get("commit_done") != 0);
            assert!(s.cycles - start <= 30);
            let error = s.model.get("commit_error");
            s.idle(1);
            error
        };
        let read =
            |s: &mut Sim<'_>, slot: u64, generation: u64, epoch: u64, word: u64, ack: bool| {
                s.model.set_all(&[
                    ("request_valid", 1),
                    ("request_slot", slot),
                    ("request_generation", generation),
                    ("request_epoch", epoch),
                    ("request_word", word),
                    ("request_ack", u64::from(ack)),
                ]);
                s.until(10, "snapshot read admission", |m| {
                    m.get("request_ready") != 0
                });
                s.idle(1);
                s.model.set("request_valid", 0);
                s.until(10, "snapshot read response", |m| {
                    m.get("response_valid") != 0
                });
                let result = (s.model.get("response_error"), s.model.get("response_data"));
                s.idle(1);
                result
            };
        for slot in 0..slots {
            assert_eq!(commit(&mut s, slot, 1, &value(slot)), 0);
        }
        assert_eq!(
            commit(&mut s, 0, 2, &Bits::zero(1856)),
            5,
            "full records allowed overwrite"
        );
        s.idle(1024 * 64);
        for slot in 0..slots {
            for word in 0..29 {
                assert_eq!(
                    read(&mut s, slot, 1, 7, word, false),
                    (0, value(slot).get(word as usize * 64, 64))
                );
            }
        }
        for (slot, generation, epoch, word) in
            [(0, 2, 7, 0), (0, 1, 6, 0), (slots, 1, 7, 0), (0, 1, 7, 29)]
        {
            assert_eq!(read(&mut s, slot, generation, epoch, word, false), (3, 0));
        }
        assert_eq!(read(&mut s, 0, 2, 7, 0, true), (3, 0));
        assert_eq!(read(&mut s, 0, 1, 7, 0, false), (0, value(0).get(0, 64)));
        assert_eq!(read(&mut s, 0, 1, 7, 0, true), (0, 0));
        assert_eq!(read(&mut s, 0, 1, 7, 0, false), (3, 0));
        assert_eq!(commit(&mut s, 0, 2, &value(99)), 0);
        assert_eq!(read(&mut s, 0, 1, 7, 0, true), (3, 0));
        assert_eq!(
            read(&mut s, 0, 2, 7, 28, false),
            (0, value(99).get(28 * 64, 64))
        );
        assert_eq!(read(&mut s, 1, 1, 7, 0, true), (0, 0));
        s.model.set_all(&[
            ("commit_valid", 1),
            ("commit_slot", 1),
            ("commit_generation", 2),
        ]);
        s.model.set_bits("commit_data", &value(111));
        s.idle(1);
        s.model.set("commit_valid", 0);
        s.idle(8);
        assert_eq!(s.model.get("commit_ready"), 0);
        s.model.set_all(&[("reset", 1), ("epoch", 8)]);
        s.idle(1);
        s.model.set("reset", 0);
        s.idle(1);
        assert_eq!(read(&mut s, 1, 2, 7, 0, false), (3, 0));
        assert_eq!(read(&mut s, 0, 2, 8, 0, false), (3, 0));
        assert_eq!(commit(&mut s, 0, 1, &value(22)), 0);
        assert_eq!(read(&mut s, 0, 1, 7, 0, true), (3, 0));
        assert_eq!(read(&mut s, 0, 1, 8, 0, false), (0, value(22).get(0, 64)));
        println!(
            "retained cut records/{slots}: all credits filled, 1,024-quantum pause, exact historical words, generation/epoch/ack/reuse and partial-copy reset"
        );
    }
    Ok(())
}
