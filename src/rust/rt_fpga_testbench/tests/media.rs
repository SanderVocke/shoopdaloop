use rt_fpga_reference::{
    compiler::{compile, Binding, Graph, Node, NodeId},
    fixed::Fixed,
    state::Value,
    storage::{ChunkId, MediaStore, ViewId},
    Error,
};
use rt_fpga_testbench::{
    run_audio_case, AudioCase, AudioStep, Document, ExecutionStage, ExecutionStatus, MediaAction,
    Payload, StateSnapshot, StateWrite,
};
use shoop_wasm_test_support::shoop_test;

fn case(limit: usize) -> AudioCase {
    let mut media = MediaStore::with_chunk_limit(3, limit).unwrap();
    let view = media.create_recording().unwrap();
    let compiled = compile(&Graph {
        quantum: 4,
        nodes: vec![
            Node::Input { port: 0 },
            Node::Capture {
                input: NodeId(0),
                view,
                recording: true,
            },
            Node::Playback {
                view,
                length: 4,
                position: 0,
                playing: false,
            },
            Node::Output {
                input: NodeId(2),
                port: 0,
            },
        ],
    })
    .unwrap();
    let Binding::Capture { enabled: recording } = compiled.bindings[1] else {
        panic!("capture")
    };
    let Binding::Playback {
        enabled: playing, ..
    } = compiled.bindings[2]
    else {
        panic!("playback")
    };
    let writes = |record, play| {
        vec![
            StateWrite {
                address: recording,
                value: Value::Word(record),
            },
            StateWrite {
                address: playing,
                value: Value::Word(play),
            },
        ]
    };
    AudioCase {
        name: "elastic media record/play/clear/reuse/remove".into(),
        queue_capacity: compiled.program.jobs_per_quantum as u32,
        program: compiled.program,
        initial_state: StateSnapshot {
            memory: compiled.initial_state,
        },
        buffers: 4,
        output_ports: 1,
        media: Some(media.snapshot()),
        steps: vec![
            AudioStep {
                writes: vec![],
                inputs: vec![vec![Fixed(11); 4]],
                media_actions: None,
            },
            AudioStep {
                writes: writes(0, 1),
                inputs: vec![vec![Fixed::ZERO; 4]],
                media_actions: None,
            },
            AudioStep {
                writes: writes(1, 0),
                inputs: vec![vec![Fixed(22); 4]],
                media_actions: Some(vec![MediaAction::Clear { view }]),
            },
            AudioStep {
                writes: writes(0, 1),
                inputs: vec![vec![Fixed::ZERO; 4]],
                media_actions: None,
            },
            AudioStep {
                writes: writes(0, 0),
                inputs: vec![vec![Fixed::ZERO; 4]],
                media_actions: Some(vec![MediaAction::Remove { view }]),
            },
        ],
    }
}

fn shared_case() -> AudioCase {
    let document = Document::from_json(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../scripts/fpga_testbench/examples/shared_ranges.json"
    )))
    .unwrap();
    let Payload::AudioCase(case) = document.payload else {
        panic!("audio case")
    };
    case
}

#[shoop_test]
fn shared_range_traces_preserve_samples_through_source_clear_and_reuse() {
    let trace = run_audio_case(shared_case()).unwrap();
    assert_eq!(trace.results.len(), 4);
    assert!(trace
        .results
        .iter()
        .all(|result| result.status == ExecutionStatus::Completed));
    assert_eq!(
        trace.results[0].outputs[0],
        [Fixed(4), Fixed(5), Fixed(6), Fixed(7)]
    );
    assert_eq!(
        trace.results[1].outputs[0],
        [Fixed(8), Fixed(4), Fixed(5), Fixed(6)]
    );
    let first = trace.results[0].media.as_ref().unwrap();
    let slice = first.views[1].as_ref().unwrap();
    assert_eq!(slice.chunks, [ChunkId(0), ChunkId(1)]);
    assert_eq!(slice.range.unwrap().offset, 3);
    let reused = trace.results[1].media.as_ref().unwrap();
    assert_eq!(reused.views[0].as_ref().unwrap().chunks, [ChunkId(2)]);
    assert_eq!(reused.samples.len(), first.samples.len());
    let released = trace.results[2].media.as_ref().unwrap();
    assert_eq!(released.free_chunks, [ChunkId(0)]);
    assert!(released.views[1].as_ref().unwrap().chunks.is_empty());
    let final_media = trace.results[3].media.as_ref().unwrap();
    assert_eq!(
        final_media.views[0].as_ref().unwrap().chunks,
        [ChunkId(2), ChunkId(1), ChunkId(0)]
    );
    assert!(final_media.free_chunks.is_empty());
    for result in &trace.results {
        let media = result.media.as_ref().unwrap();
        assert_eq!(
            MediaStore::from_snapshot(media.clone()).unwrap().snapshot(),
            *media
        );
    }
    let document = Document::new(Payload::AudioTrace(trace));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[shoop_test]
fn shared_range_control_and_capture_failures_preserve_media() {
    let mut input = shared_case();
    input.steps[0].media_actions = Some(vec![MediaAction::Slice {
        source: ViewId(0),
        start: 13,
        frames: 0,
    }]);
    let initial = input.media.clone().unwrap();
    let trace = run_audio_case(input).unwrap();
    assert_eq!(trace.results.len(), 1);
    assert_eq!(trace.results[0].stage, ExecutionStage::Media);
    assert_eq!(
        trace.results[0].status,
        ExecutionStatus::Failed {
            error: Error::InvalidView
        }
    );
    assert_eq!(trace.results[0].media.as_ref(), Some(&initial));
    let mut input = shared_case();
    // The capture instruction uses state word zero for its destination view.
    input.steps[0].writes.extend([
        StateWrite {
            address: rt_fpga_reference::state::Address(0),
            value: Value::Word(1),
        },
        StateWrite {
            address: rt_fpga_reference::state::Address(1),
            value: Value::Word(1),
        },
    ]);
    let mut expected = MediaStore::from_snapshot(input.media.clone().unwrap()).unwrap();
    expected.slice(ViewId(0), 3, 5).unwrap();
    let trace = run_audio_case(input).unwrap();
    assert_eq!(trace.results.len(), 1);
    assert_eq!(trace.results[0].stage, ExecutionStage::Jobs);
    assert_eq!(
        trace.results[0].status,
        ExecutionStatus::Failed {
            error: Error::InvalidState
        }
    );
    assert_eq!(trace.results[0].media, Some(expected.snapshot()));
}

#[shoop_test]
fn audio_traces_capture_recycled_slots_and_restore_allocator_state_exactly() {
    let trace = run_audio_case(case(2)).unwrap();
    assert_eq!(trace.results.len(), 5);
    assert!(trace
        .results
        .iter()
        .all(|result| result.status == ExecutionStatus::Completed));
    assert_eq!(trace.results[1].outputs[0], vec![Fixed(11); 4]);
    assert_eq!(trace.results[3].outputs[0], vec![Fixed(22); 4]);
    let first = trace.results[0].media.as_ref().unwrap();
    assert_eq!(
        first.views[0].as_ref().unwrap().chunks,
        [ChunkId(0), ChunkId(1)]
    );
    let reused = trace.results[2].media.as_ref().unwrap();
    assert_eq!(
        reused.views[0].as_ref().unwrap().chunks,
        [ChunkId(1), ChunkId(0)]
    );
    assert_eq!(reused.samples.len(), 6);
    let released = trace.results[4].media.as_ref().unwrap();
    assert_eq!(released.views, [None]);
    assert_eq!(released.free_chunks, [ChunkId(1), ChunkId(0)]);
    let mut restored = MediaStore::from_snapshot(released.clone()).unwrap();
    let id = restored.insert(&[Fixed(33); 4]).unwrap();
    assert_eq!(id, ViewId(1));
    assert_eq!(restored.chunk_ids(id).unwrap(), [ChunkId(0), ChunkId(1)]);
    for payload in [Payload::Media(released.clone()), Payload::AudioTrace(trace)] {
        let document = Document::new(payload);
        assert_eq!(
            Document::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );
    }
}

#[shoop_test]
fn capture_exhaustion_preserves_media_and_reports_unconsumed_jobs() {
    let input = case(1);
    let initial = input.media.clone().unwrap();
    let trace = run_audio_case(input).unwrap();
    assert_eq!(trace.results.len(), 1);
    let result = &trace.results[0];
    assert_eq!(result.stage, ExecutionStage::Jobs);
    assert_eq!(
        result.status,
        ExecutionStatus::Failed {
            error: Error::MemoryFull
        }
    );
    assert_eq!(result.media.as_ref(), Some(&initial));
    assert_eq!(result.remaining_jobs.len(), 2);
    assert_eq!(result.buffers[0], Some(vec![Fixed(11); 4]));
    assert_eq!(result.buffers[2], None);
}

#[shoop_test]
fn media_control_failures_precede_state_execution_and_bad_initial_snapshots_are_refused() {
    let mut input = case(2);
    input.steps[0].media_actions = Some(vec![MediaAction::Remove { view: ViewId(99) }]);
    let trace = run_audio_case(input).unwrap();
    assert_eq!(trace.results.len(), 1);
    assert_eq!(trace.results[0].stage, ExecutionStage::Media);
    assert_eq!(
        trace.results[0].status,
        ExecutionStatus::Failed {
            error: Error::InvalidView
        }
    );
    assert!(trace.results[0].jobs.is_empty());
    assert_eq!(trace.results[0].state, trace.case.initial_state);
    let mut invalid = case(2);
    invalid.media.as_mut().unwrap().samples.push(Fixed(1));
    assert_eq!(run_audio_case(invalid), Err(Error::InvalidShape));
}
