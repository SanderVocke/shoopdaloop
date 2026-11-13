#![cfg(all(feature = "app", not(target_arch = "wasm32")))]
use rt_audio_math::{Q824, Sample};
use rt_fpga_adapter::{image::*, loop_media::Media, protocol};
use rt_fpga_sim::{Simulator, runtime};
use rt_runtime::{Epoch, Error, Frame, Sequence};
use shoop_loops::{
    LoopId, Request, Timing, Transition,
    runtime::{Reply, Selection},
};
use shoop_wasm_test_support::shoop_test;
fn settle(sim: &mut Simulator<'_>, media: &mut Media, frame: u64) {
    sim.command(16, 0, [0; 20]).unwrap();
    let state = sim.command(12, 0, [0; 20]).unwrap();
    media
        .observe(protocol::state(&state, 0).unwrap(), Frame(frame), sim)
        .unwrap();
}
fn command(sim: &mut Simulator<'_>, transition: Transition, frame: u64) {
    sim.command(
        4,
        0,
        protocol::transition(Request {
            target: LoopId::from_parts(0, 1).unwrap(),
            sequence: Sequence(frame + 1),
            at: Frame(frame),
            timing: Timing::At,
            transition,
        })
        .unwrap(),
    )
    .unwrap();
}
#[shoop_test]
fn float_media_reads_preserve_interleaving_partial_pages_leases_and_scopes() {
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let runtime = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&runtime, None, true).unwrap();
    let case: rt_backend_testbench::spec::Scenario =
        serde_json::from_str(include_str!("../fixtures/instant-gains.json")).unwrap();
    let mut image = rt_backend_testbench::rtl::compile(&case).unwrap();
    image.buffers = 2;
    image.input_ports = 2;
    image.output_ports = 0;
    image.initial_state.memory = vec![Value::Word(0), Value::Word(1)];
    image.gains.clear();
    image.media.views = vec![
        View {
            chunks: vec![],
            frames: 0
        };
        2
    ];
    image.transition.description.loops = vec![LoopSpec {
        id: Identity {
            slot: 0,
            generation: 1,
        },
        kind: "primitive".into(),
        length: 0,
        sync_source: None,
        repeat_sync: false,
        timeline: None,
    }];
    image.transition.dependency_order = vec![Identity {
        slot: 0,
        generation: 1,
    }];
    image.program.instructions = vec![
        Instruction::ProcessInput {
            port: 0,
            destination: 0,
        },
        Instruction::ProcessInput {
            port: 1,
            destination: 1,
        },
        Instruction::ProcessLogicalCapture {
            loop_id: Identity {
                slot: 0,
                generation: 1,
            },
            view: 0,
            source: 0,
        },
        Instruction::ProcessLogicalCapture {
            loop_id: Identity {
                slot: 0,
                generation: 1,
            },
            view: 1,
            source: 1,
        },
    ];
    image.program.jobs_per_quantum = 6;
    sim.load(&image).unwrap();
    let mut media =
        Media::with_channels(4, 2, rt_fpga_adapter::loop_media::Encoding::Float32).unwrap();
    let input = vec![
        (0..32)
            .map(|i| i * 998_777 - 16_777_217)
            .collect::<Vec<i32>>(),
        (0..32)
            .map(|i| 16_777_219 - i * 513_733)
            .collect::<Vec<i32>>(),
    ];
    command(
        &mut sim,
        Transition::Record {
            monitor_previous: false,
        },
        0,
    );
    settle(&mut sim, &mut media, 0);
    sim.run(&image, &input).unwrap();
    command(&mut sim, Transition::Stop, 32);
    settle(&mut sim, &mut media, 32);
    let state = protocol::state(&sim.command(12, 0, [0; 20]).unwrap(), 0).unwrap();
    let Reply::Export { id, .. } = media
        .export(state, Frame(32), Epoch(0), Selection::Playback)
        .unwrap()
    else {
        panic!("export")
    };
    let mut cursor = media.cursor(id).unwrap();
    let stale = cursor;
    let expected = (0..32)
        .flat_map(|i| {
            [
                Q824(input[0][i]).to_f32().to_le_bytes(),
                Q824(input[1][i]).to_f32().to_le_bytes(),
            ]
            .into_iter()
            .flatten()
        })
        .collect::<Vec<_>>();
    let mut got = vec![];
    loop {
        let mut bytes = [0; 17];
        let (count, done) = media.read(&mut cursor, &mut bytes, 13, &mut sim).unwrap();
        got.extend_from_slice(&bytes[..count]);
        if done {
            break;
        }
    }
    assert_eq!(got, expected);
    assert_eq!(
        media.read(&mut cursor, &mut [0; 17], 13, &mut sim).unwrap(),
        (0, true)
    );
    sim.run(&image, &vec![vec![0; 32]; 2]).unwrap();
    command(
        &mut sim,
        Transition::Record {
            monitor_previous: false,
        },
        64,
    );
    settle(&mut sim, &mut media, 64);
    sim.run(&image, &vec![vec![7; 32]; 2]).unwrap();
    let mut retained = media.cursor(id).unwrap();
    let mut bytes = vec![0; expected.len()];
    let (n, done) = media
        .read(&mut retained, &mut bytes, 4096, &mut sim)
        .unwrap();
    assert!(done);
    assert_eq!(n, expected.len());
    assert_eq!(bytes, expected);
    let other = Media::with_channels(4, 2, rt_fpga_adapter::loop_media::Encoding::Float32).unwrap();
    assert_eq!(
        other.read(&mut stale.clone(), &mut [0; 8], 13, &mut sim),
        Err(Error::InvalidId)
    );
    media.release(id, &mut sim).unwrap();
    assert_eq!(media.cursor(id), Err(Error::InvalidId));
    command(&mut sim, Transition::Stop, 96);
    settle(&mut sim, &mut media, 96);
    assert_eq!(
        media.read(&mut stale.clone(), &mut [0; 8], 13, &mut sim),
        Err(Error::InvalidId)
    );
}

#[shoop_test]
fn large_preset_has_metadata_for_two_retained_stereo_capacity_takes() {
    use rt_fpga_adapter::looping::{LoopDevice, MediaCommand};
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-store");
    let runtime = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&runtime, None, false).unwrap();
    sim.arena_capacity(32768).unwrap();
    let case: rt_backend_testbench::spec::Scenario =
        serde_json::from_str(include_str!("../fixtures/instant-gains.json")).unwrap();
    let mut image = rt_backend_testbench::rtl::compile(&case).unwrap();
    image.media.views = vec![
        View {
            chunks: vec![],
            frames: 0
        };
        2
    ];
    sim.load(&image).unwrap();
    let samples = vec![19; 240_000];
    let mut retained = vec![];
    for take in 0..2 {
        for view in 0..2 {
            sim.media(MediaCommand {
                opcode: 8,
                view,
                start: 0,
                length: 0,
                count: 0,
            })
            .unwrap();
            sim.append_view(view, &samples).unwrap();
        }
        eprintln!("take {take}: {:?}", sim.store_usage().unwrap());
        for view in 0..2 {
            let result = sim
                .media(MediaCommand {
                    opcode: 1,
                    view,
                    start: 0,
                    length: 240_000,
                    count: 0,
                })
                .unwrap_or_else(|error| {
                    panic!("take {take}, view {view}: {error}; {:?}", sim.last_error)
                });
            retained.push(result.view);
        }
    }
    for view in retained {
        sim.media(MediaCommand {
            opcode: 3,
            view,
            start: 0,
            length: 0,
            count: 0,
        })
        .unwrap();
    }
}
