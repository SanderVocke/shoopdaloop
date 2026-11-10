use rt_fpga_reference::{
    compiler::{compile, Graph, Node, NodeId},
    fixed::Fixed,
    pipeline::PipelineError,
    storage::{MediaStore, ViewId},
    transitions::*,
    Error,
};
use rt_fpga_testbench::{
    pipeline::*,
    transitions::{TransitionPlanInput, TransitionStep},
    Document, Payload, StateSnapshot,
};
use shoop_wasm_test_support::shoop_test;

fn fixture() -> PipelineCase {
    let loop_id = LoopId {
        slot: 0,
        generation: 9,
    };
    let mut media = MediaStore::new(7).unwrap();
    let view = media.create_recording().unwrap();
    let compiled = compile(&Graph {
        quantum: 16,
        nodes: vec![
            Node::LogicalPlayback { loop_id, view },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
            Node::Input { port: 0 },
            Node::LogicalCapture {
                loop_id,
                view,
                input: NodeId(2),
            },
        ],
    })
    .unwrap();
    assert_eq!(compiled.initial_state.len(), 2);
    PipelineCase {
        name: "record-play-shared-records".into(),
        plan: TransitionPlanInput {
            description: PlanDescription {
                quantum: 16,
                loops: vec![LoopSpec {
                    id: loop_id,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                    repeat_sync: false,
                    timeline: None,
                }],
            },
            limits: TransitionLimits {
                loop_slots: 1,
                pending_requests: 2,
                events: 4,
            },
        },
        program: compiled.program,
        initial_state: StateSnapshot {
            memory: compiled.initial_state,
        },
        queue_capacity: 4,
        buffers: 4,
        output_ports: 1,
        media: Some(media.snapshot()),
        steps: (0..6)
            .map(|i| PipelineStep {
                writes: vec![],
                transition: TransitionStep {
                    collect_completed: true,
                    collect: vec![],
                    cancel: vec![],
                    previews: vec![vec![], vec![]],
                    commands: if i == 0 || i == 2 {
                        vec![Command {
                            target: loop_id,
                            sequence: i + 1,
                            at: 0,
                            timing: Timing::At,
                            operation: if i == 0 {
                                Operation::Record
                            } else {
                                Operation::Play {
                                    position: 0,
                                    retrigger: true,
                                }
                            },
                        }]
                    } else {
                        vec![]
                    },
                },
                work: PipelineWork::Audio {
                    inputs: vec![vec![Fixed(1000 + i as i32); 16]],
                },
            })
            .collect(),
    }
}

#[shoop_test]
fn combined_trace_runs_all_processors_without_copied_loop_operands() {
    let case = fixture();
    let initial = case.initial_state.clone();
    let trace = run_pipeline_case(case).unwrap();
    for (i, row) in trace.results.iter().enumerate() {
        assert_eq!(row.state, initial);
        assert_eq!(row.error, None);
        assert_eq!(row.transition.next_frame, 16 * (i as u64 + 1));
        let sample = if i < 2 { 0 } else { 1000 + (i as i32 - 2) % 2 };
        assert_eq!(
            row.audio.as_ref().unwrap().outputs,
            vec![vec![Fixed(sample); 16]]
        );
    }
    let document = Document::new(Payload::PipelineTrace(trace));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[shoop_test]
fn combined_trace_stops_at_failed_state_with_committed_clock_unchanged() {
    let mut case = fixture();
    let rt_fpga_reference::state::Instruction::ProcessLogicalPlayback { loop_id, .. } =
        &mut case.program.instructions[0]
    else {
        panic!("logical playback")
    };
    loop_id.generation += 1;
    let trace = run_pipeline_case(case).unwrap();
    assert_eq!(trace.results.len(), 1);
    assert_eq!(
        trace.results[0].error,
        Some(PipelineError::State(Error::InvalidState))
    );
    assert_eq!(trace.results[0].transition.next_frame, 0);
    assert!(trace.results[0].transition.committed);
}

#[shoop_test]
fn shared_loop_pipeline_plays_a_retained_range_after_source_clear() {
    let recorded = run_pipeline_case(fixture()).unwrap();
    let snapshot = recorded
        .results
        .last()
        .unwrap()
        .audio
        .as_ref()
        .unwrap()
        .media
        .clone()
        .unwrap();
    let mut media = MediaStore::from_snapshot(snapshot).unwrap();
    let slice = media.slice(ViewId(0), 3, 16).unwrap();
    media.clear(ViewId(0)).unwrap();

    let mut case = fixture();
    case.name = "shared-loop-playback-of-retained-range".into();
    case.media = Some(media.snapshot());
    case.plan.description.loops[0].length = 16;
    case.initial_state
        .memory
        .fill(rt_fpga_reference::state::Value::Word(slice.0 as u64));
    case.steps.truncate(2);
    case.steps[0].transition.commands[0].operation = Operation::Play {
        position: 0,
        retrigger: true,
    };
    let initial = case.initial_state.clone();
    let trace = run_pipeline_case(case).unwrap();
    let expected: Vec<_> = (0..16)
        .map(|i| Fixed(if i < 13 { 1000 } else { 1001 }))
        .collect();
    for (i, row) in trace.results.iter().enumerate() {
        assert_eq!(row.error, None);
        assert_eq!(row.state, initial);
        assert_eq!(row.transition.next_frame, (i as u64 + 1) * 16);
        let audio = row.audio.as_ref().unwrap();
        assert_eq!(audio.outputs, vec![expected.clone()]);
        let snapshot = audio.media.as_ref().unwrap();
        assert_eq!(snapshot.views[0].as_ref().unwrap().frames, 0);
        assert_eq!(
            snapshot.views[slice.0]
                .as_ref()
                .unwrap()
                .range
                .unwrap()
                .offset,
            3
        );
    }
    let document = Document::new(Payload::PipelineTrace(trace));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}
