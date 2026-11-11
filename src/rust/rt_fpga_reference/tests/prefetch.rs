use rt_fpga_reference::{
    compiler::{compile, Graph, Node, NodeId},
    fixed::Fixed,
    jobs::{JobProcessor, JobQueue},
    resolver::{resolve, ReadIntent},
    state::{Instruction, StateProcessor, Value},
    storage::{ChunkId, MediaStore},
    Error,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn emission_resolves_only_the_read_window_in_chunk_units_including_wrap() {
    let mut media = MediaStore::new(32).unwrap();
    let view = media.create_recording().unwrap();
    let samples: Vec<_> = (0..96).map(Fixed).collect();
    media.append(view, &samples).unwrap();
    let compiled = compile(&Graph {
        quantum: 64,
        chunk_frames: 32,
        nodes: vec![
            Node::Playback {
                view,
                length: 3,
                position: 2,
                playing: true,
            },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
        ],
    })
    .unwrap();
    let mut state = StateProcessor::new(compiled.initial_state);
    let mut queue = JobQueue::new(2);
    let mut intents = Vec::new();
    let mut ids = Vec::new();
    state
        .run_with_intents(&compiled.program, &mut queue, |intent| {
            intents.push(intent);
            resolve(intent, &media, |chunk| ids.push(chunk));
        })
        .unwrap();
    assert_eq!(
        intents,
        [ReadIntent {
            view,
            position: 2,
            length: 3,
            chunks: 2
        }]
    );
    assert_eq!(ids, [ChunkId(2), ChunkId(0)]);
    let mut output = [Fixed::ZERO; 64];
    JobProcessor::new(64, 2)
        .run(&mut queue, &mut media, &[], &mut [&mut output])
        .unwrap();
    assert_eq!(
        output.as_slice(),
        [samples[64..].to_vec(), samples[..32].to_vec()].concat()
    );
    ids.clear();
    state
        .run_with_intents(&compiled.program, &mut queue, |_| {})
        .unwrap();
    assert_eq!(
        state.run_with_intents(&compiled.program, &mut queue, |intent| {
            resolve(intent, &media, |chunk| ids.push(chunk));
        }),
        Err(Error::QueueFull)
    );
    assert!(ids.is_empty());
}

#[shoop_test]
fn unresolved_early_reads_observe_chunks_published_by_preceding_capture_jobs() {
    let mut media = MediaStore::new(4).unwrap();
    let view = media.create_recording().unwrap();
    let compiled = compile(&Graph {
        quantum: 4,
        chunk_frames: 4,
        nodes: vec![
            Node::Input { port: 0 },
            Node::Capture {
                input: NodeId(0),
                view,
                recording: true,
            },
            Node::Playback {
                view,
                length: 1,
                position: 0,
                playing: true,
            },
            Node::Output {
                input: NodeId(2),
                port: 0,
            },
        ],
    })
    .unwrap();
    let Instruction::ProcessLoopPlayback {
        position, length, ..
    } = compiled.program.instructions[2]
    else {
        panic!("playback");
    };
    let mut state = StateProcessor::new(compiled.initial_state);
    let mut queue = JobQueue::new(4);
    let mut jobs = JobProcessor::new(4, 4);
    for index in 0..4 {
        state.write(position, Value::Word(index)).unwrap();
        state.write(length, Value::Word(index + 1)).unwrap();
        let mut ids = Vec::new();
        state
            .run_with_intents(&compiled.program, &mut queue, |intent| {
                resolve(intent, &media, |chunk| ids.push(chunk));
            })
            .unwrap();
        assert!(ids.is_empty());
        let input = [Fixed(index as i32 * 31 + 7); 4];
        let mut output = [Fixed::ZERO; 4];
        jobs.run(&mut queue, &mut media, &[&input], &mut [&mut output])
            .unwrap();
        assert_eq!(output, input);
    }
}

#[shoop_test]
fn resolver_ranges_share_chunks_and_skip_invalid_or_unpublished_hints() {
    let mut media = MediaStore::new(4).unwrap();
    let source = media.create_recording().unwrap();
    media.append(source, &[Fixed(91); 16]).unwrap();
    let view = media.slice_chunks(source, 1, 2).unwrap();
    media.clear(source).unwrap();
    let mut ids = Vec::new();
    resolve(
        ReadIntent {
            view,
            position: 1,
            length: 2,
            chunks: 3,
        },
        &media,
        |id| ids.push(id),
    );
    assert_eq!(ids, [ChunkId(2), ChunkId(1), ChunkId(2)]);
    for hint in [
        ReadIntent {
            view: source,
            position: 0,
            length: 1,
            chunks: 1,
        },
        ReadIntent {
            view,
            position: 2,
            length: 2,
            chunks: 1,
        },
        ReadIntent {
            view,
            position: 0,
            length: u64::MAX,
            chunks: 1,
        },
    ] {
        resolve(hint, &media, |_| panic!("invalid hint resolved"));
    }
}
