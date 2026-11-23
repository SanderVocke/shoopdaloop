#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;
use rt_fpga_compiler::{compile, CompileContext};
#[path = "../../rt_fpga_compiler/tests/support/programs.rs"]
mod programs;
use rt_fpga_compiler::{
    loading::Allocator, Identity, LogicalStateProgram, Operation, ProgramLimits,
};
use rt_fpga_reference::{
    fixed::Fixed,
    jobs::JobProcessor,
    pipeline::Pipeline,
    state::{ModePlan, ModeStep},
    storage::MediaStore,
    target::{ProgramImage, RustTarget},
    transitions::{LoopId, Mode},
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn limits() -> ProgramLimits {
    ProgramLimits {
        loop_slots: 8,
        state_values: 32,
        buffers: 32,
        history_slots: 32,
        jobs: 128,
        instructions: 128,
        ..Default::default()
    }
}
fn graph(ids: &[u32]) -> LogicalStateProgram {
    compile(
        &mono::session(
            &(ids
                .iter()
                .map(|&slot| mono::MonoLoop {
                    id: Identity {
                        slot,
                        generation: 1,
                    },
                    source: (slot != 0).then_some(0),
                    input: true,
                    output: true,
                    gain: 1.0,
                    repeat_sync: false,
                })
                .collect::<Vec<_>>()),
        ),
        CompileContext {
            quantum: 16,
            limits: Default::default(),
        },
    )
    .unwrap()
}
fn image(allocator: &mut Allocator, logical: &LogicalStateProgram, views: &[u64]) -> ProgramImage {
    let layout = allocator.stage(logical).unwrap();
    let mut target = RustTarget::new(views);
    target.limits = limits();
    target.build_image(logical, layout).unwrap()
}
fn start(image: ProgramImage) -> Pipeline {
    Pipeline::from_image(image, limits()).unwrap()
}
fn id(layout: &rt_fpga_compiler::loading::Layout, slot: u32) -> LoopId {
    let id = layout
        .loop_id(Identity {
            slot,
            generation: 1,
        })
        .unwrap();
    LoopId {
        slot: id.slot,
        generation: id.generation,
    }
}
fn quantum(
    pipeline: &mut Pipeline,
    jobs: &mut JobProcessor,
    media: &mut MediaStore,
    value: f32,
) -> Vec<Fixed> {
    let input = vec![Fixed::from_f32(value); 16];
    let mut left = vec![Fixed::ZERO; 16];
    let mut right = left.clone();
    pipeline
        .process(jobs, media, &[&input], &mut [&mut left, &mut right])
        .unwrap();
    assert_eq!(left, right);
    left
}

#[shoop_test]
fn staging_never_reinitializes_live_recording_and_switch_keeps_plans_and_frame() {
    let mut allocator = Allocator::new(limits());
    let mut media = MediaStore::new(1).unwrap();
    let views = (0..8)
        .map(|_| media.create_recording().unwrap().0 as u64)
        .collect::<Vec<_>>();
    let a = image(&mut allocator, &graph(&[0, 1]), &views[..2]);
    let layout = a.layout.clone();
    allocator.publish(layout.revision).unwrap();
    allocator.complete(layout.revision).unwrap();
    let mut pipeline = start(a);
    let mut jobs = JobProcessor::with_history_slots(16, 32, 32);
    let root = id(&layout, 0);
    let voice = id(&layout, 1);
    for loop_id in [root, voice] {
        pipeline
            .write_mode_plan(
                loop_id,
                ModePlan {
                    now: Some(Mode::Recording),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    for _ in 0..4 {
        quantum(&mut pipeline, &mut jobs, &mut media, 0.25);
    }
    pipeline
        .write_mode_plan(
            root,
            ModePlan {
                now: Some(Mode::Playing),
                ..Default::default()
            },
        )
        .unwrap();
    pipeline
        .write_mode_plan(
            voice,
            ModePlan {
                steps: [
                    Some(ModeStep {
                        mode: Mode::Stopped,
                        skip: 2,
                    }),
                    None,
                ],
                ..Default::default()
            },
        )
        .unwrap();
    let bgraph = graph(&[1, 2, 0]);
    let b_layout = allocator.stage(&bgraph).unwrap();
    let bviews = b_layout
        .loops
        .iter()
        .map(|l| views[l.physical.slot as usize])
        .collect::<Vec<_>>();
    let mut target = RustTarget::new(&bviews);
    target.limits = limits();
    let mut b = target.build_image(&bgraph, b_layout.clone()).unwrap();
    for _ in 0..3 {
        quantum(&mut pipeline, &mut jobs, &mut media, 0.5);
    }
    let before = pipeline.state().loop_state(voice).unwrap().clone();
    let frame = pipeline.state().frame();
    let gain = layout.state_address(1);
    let value = pipeline
        .state()
        .read(rt_fpga_reference::state::Address(gain as usize))
        .unwrap();
    assert_no_heap(|| pipeline.switch_image(&mut b).unwrap());
    assert_eq!(pipeline.state().loop_state(voice).unwrap(), &before);
    assert_eq!(pipeline.state().frame(), frame);
    assert_eq!(
        pipeline
            .state()
            .read(rt_fpga_reference::state::Address(gain as usize))
            .unwrap(),
        value
    );
    assert_eq!(
        pipeline
            .state()
            .loop_state(id(&b_layout, 2))
            .unwrap()
            .state
            .capture_frames,
        0
    );
    allocator.publish(b_layout.revision).unwrap();
    allocator.complete(b_layout.revision).unwrap();
    for _ in 0..16 {
        quantum(&mut pipeline, &mut jobs, &mut media, 0.75);
    }
    assert_eq!(
        pipeline.state().loop_state(voice).unwrap().state.mode,
        Mode::Stopped
    );
    assert!(
        pipeline
            .state()
            .loop_state(voice)
            .unwrap()
            .state
            .content_length
            > before.state.capture_frames
    );
    assert_eq!(pipeline.state().frame(), frame + 16 * 16);
}

#[shoop_test]
fn smoothing_history_and_gain_ramp_survive_changed_scratch_assignments() {
    let logical = programs::sync_smoothing(16);
    let mut modified = logical.clone();
    for op in &mut modified.work_pass {
        match op {
            Operation::Input { destination, .. } => *destination += 3,
            Operation::Gain {
                source,
                destination,
                ..
            } => {
                *source += 3;
                *destination += 3;
            }
            Operation::Smoothing { source, .. } => *source += 3,
            Operation::Output { source, .. } => *source += 3,
            _ => unreachable!(),
        }
    }
    if let rt_fpga_compiler::InitialValue::Smoothing { destination, .. } =
        &mut modified.initial_state[0]
    {
        *destination += 3;
    }
    let mut a_alloc = Allocator::new(limits());
    let mut b_alloc = Allocator::new(limits());
    let a = image(&mut a_alloc, &logical, &[]);
    let a_layout = a.layout.clone();
    a_alloc.publish(a_layout.revision).unwrap();
    a_alloc.complete(a_layout.revision).unwrap();
    let baseline = image(&mut b_alloc, &logical, &[]);
    let physical = a_layout.loops[0].physical;
    let loop_id = LoopId {
        slot: physical.slot,
        generation: physical.generation,
    };
    let mut a = start(a);
    let mut baseline = start(baseline);
    let mut a_jobs = JobProcessor::with_history_slots(16, 32, 32);
    let mut b_jobs = JobProcessor::with_history_slots(16, 32, 32);
    let mut a_media = MediaStore::new(1).unwrap();
    let mut b_media = MediaStore::new(1).unwrap();
    for p in [&mut a, &mut baseline] {
        p.write_mode_plan(
            loop_id,
            ModePlan {
                now: Some(Mode::Playing),
                ..Default::default()
            },
        )
        .unwrap();
    }
    for tick in 0..12 {
        if tick == 3 {
            let mut next = image(&mut a_alloc, &modified, &[]);
            assert_eq!(next.initializers.len(), 0);
            a.switch_image(&mut next).unwrap();
            a_alloc.publish(next.layout.revision).unwrap();
            a_alloc.complete(next.layout.revision).unwrap();
        }
        let input = vec![Fixed::from_f32(if tick % 2 == 0 { 0.375 } else { -0.25 }); 16];
        let mut out = vec![Fixed::ZERO; 16];
        let mut expected = out.clone();
        a.process(&mut a_jobs, &mut a_media, &[&input], &mut [&mut out])
            .unwrap();
        baseline
            .process(&mut b_jobs, &mut b_media, &[&input], &mut [&mut expected])
            .unwrap();
        assert_eq!(out, expected, "tick {tick}");
    }
}

#[shoop_test]
fn explicit_midi_capture_survives_image_switch_and_refuses_writer_rebinding() {
    use rt_fpga_compiler::MidiOperation as M;
    use rt_fpga_reference::midi::{
        BufferLimits, InputEvent, Message, MidiMemory, SequenceId, StoreLimits,
    };
    let logical = |ids: &[u32], sequence| {
        let mut p = graph(ids);
        p.resources.buffers = p.resources.buffers.max(2);
        p.clone()
            .with_midi_data(&[
                M::Input {
                    port: 0,
                    destination: 0,
                },
                M::Capture {
                    loop_id: p.loops[0].id,
                    sequence,
                    source: 0,
                },
                M::Playback {
                    loop_id: p.loops[0].id,
                    sequence,
                    cursor: 0,
                    channels: u16::MAX,
                    destination: 1,
                },
                M::Output { source: 1, port: 0 },
            ])
            .unwrap()
    };
    let mut media = MediaStore::new(1).unwrap();
    let views: Vec<_> = (0..8)
        .map(|_| media.create_recording().unwrap().0 as u64)
        .collect();
    let mut allocator = Allocator::new(limits());
    let first = image(&mut allocator, &logical(&[0], 0), &views);
    let revision = first.layout.revision;
    let loop_id = id(&first.layout, 0);
    let mut pipeline = Pipeline::from_image(first, limits()).unwrap();
    allocator.publish(revision).unwrap();
    allocator.complete(revision).unwrap();
    let mut jobs = JobProcessor::with_history_slots(16, 32, 32);
    jobs.prepare_midi(BufferLimits {
        buffers: 2,
        cursors: 1,
        scratch_events: 1024,
        input_ports: 1,
        output_ports: 1,
    })
    .unwrap();
    let mut midi = MidiMemory::prepare(
        StoreLimits {
            block_events: 8,
            blocks: 64,
            max_events: 128,
            views: 16,
            retirements: 16,
        },
        2,
    )
    .unwrap();
    pipeline
        .write_mode_plan(
            loop_id,
            ModePlan {
                now: Some(Mode::Recording),
                ..Default::default()
            },
        )
        .unwrap();
    let mut output = Vec::new();
    let mut tick = |pipeline: &mut Pipeline,
                    jobs: &mut JobProcessor,
                    media: &mut MediaStore,
                    midi: &mut MidiMemory,
                    events: &[InputEvent]| {
        output.clear();
        pipeline
            .process_with_midi(
                jobs,
                media,
                &[&[Fixed::ZERO; 16]],
                &mut [&mut [Fixed::ZERO; 16], &mut [Fixed::ZERO; 16]],
                16,
                midi,
                events,
                &mut |e| {
                    output.push(e);
                    Ok(())
                },
            )
            .unwrap();
        output.clone()
    };
    let note = Message::parse(&[0x90, 60, 80]).unwrap();
    tick(
        &mut pipeline,
        &mut jobs,
        &mut media,
        &mut midi,
        &[InputEvent {
            input: 0,
            offset: 2,
            message: note,
        }],
    );
    let mut invalid = image(&mut allocator, &logical(&[0, 1], 1), &views);
    assert_eq!(
        pipeline.switch_image(&mut invalid),
        Err(rt_fpga_reference::pipeline::PipelineError::State(
            rt_fpga_reference::Error::InvalidState
        ))
    );
    allocator.cancel(invalid.layout.revision).unwrap();
    let mut next = image(&mut allocator, &logical(&[0, 1], 0), &views);
    pipeline.switch_image(&mut next).unwrap();
    allocator.publish(next.layout.revision).unwrap();
    allocator.complete(next.layout.revision).unwrap();
    let cc = Message::parse(&[0xb0, 1, 40]).unwrap();
    tick(
        &mut pipeline,
        &mut jobs,
        &mut media,
        &mut midi,
        &[InputEvent {
            input: 0,
            offset: 7,
            message: cc,
        }],
    );
    let view = midi.view(SequenceId(0)).unwrap().unwrap();
    assert_eq!(midi.store.describe(view).unwrap().events, 2);
    pipeline
        .write_mode_plan(
            loop_id,
            ModePlan {
                now: Some(Mode::Playing),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        tick(&mut pipeline, &mut jobs, &mut media, &mut midi, &[])
            .iter()
            .map(|e| (e.offset, e.message))
            .collect::<Vec<_>>(),
        vec![(2, note)]
    );
    assert_eq!(
        tick(&mut pipeline, &mut jobs, &mut media, &mut midi, &[])
            .iter()
            .map(|e| (e.offset, e.message))
            .collect::<Vec<_>>(),
        vec![(7, cc)]
    );
}

#[path = "../../rt_fpga_compiler/tests/support/composite.rs"]
mod composite_fixtures;

#[shoop_test]
fn composite_image_switch_waits_for_a_compatible_restart() {
    use rt_fpga_reference::{state::PlannedTransition, transitions::Operation as Transition};
    let context = CompileContext {
        quantum: 16,
        limits: limits(),
    };
    let mut graph = composite_fixtures::arbitration_graph();
    let before = rt_fpga_compiler::compile_composites(&graph, context, 16).unwrap();
    let mut allocator = Allocator::new(limits());
    let initial = image(&mut allocator, &before, &[]);
    let parent = id(&initial.layout, 1);
    allocator.publish(initial.layout.revision).unwrap();
    allocator.complete(initial.layout.revision).unwrap();
    let mut pipeline = start(initial);
    pipeline
        .write_plans(
            parent,
            [
                Some(PlannedTransition {
                    sequence: 1,
                    operation: Transition::Play {
                        position: 0,
                        retrigger: true,
                    },
                    on_pulse: false,
                    remaining_ticks: 0,
                    effective: 0,
                }),
                None,
            ],
        )
        .unwrap();
    let mut media = MediaStore::new(16).unwrap();
    let mut jobs = JobProcessor::new(16, 32);
    pipeline
        .process(&mut jobs, &mut media, &[], &mut [])
        .unwrap();

    let plan = graph.loops[1].composite.as_mut().unwrap();
    plan.version += 1;
    plan.spans.to_mut()[0].start = 1;
    plan.spans.to_mut()[0].occurrence_start = 1;
    let next = rt_fpga_compiler::compile_composites(&graph, context, 16).unwrap();
    let mut candidate = image(&mut allocator, &next, &[]);
    let revision = candidate.layout.revision;
    assert_eq!(candidate.layout.composite_cuts().len(), 1);
    assert!(candidate.layout.composite_cuts()[0].allow_restart);
    assert_eq!(id(&candidate.layout, 1), parent);
    assert!(pipeline.switch_image(&mut candidate).is_err());
    for _ in 0..3 {
        pipeline
            .process(&mut jobs, &mut media, &[], &mut [])
            .unwrap();
    }
    assert_eq!(pipeline.state().frame(), 64);
    assert_no_heap(|| pipeline.switch_image(&mut candidate)).unwrap();
    allocator.publish(revision).unwrap();
    allocator.complete(revision).unwrap();
    pipeline
        .process(&mut jobs, &mut media, &[], &mut [])
        .unwrap();
    assert_eq!(pipeline.revision(), revision);
    let state = pipeline.state().loop_state(parent).unwrap();
    assert!(state.pulse_this_quantum);
    assert_eq!(state.render.position, 0);
    assert_eq!(state.state.cycles, 1);

    let plan = graph.loops[1].composite.as_mut().unwrap();
    plan.version += 1;
    plan.spans.to_mut()[0].end = 3;
    let future = rt_fpga_compiler::compile_composites(&graph, context, 16).unwrap();
    let mut candidate = image(&mut allocator, &future, &[]);
    assert!(!candidate.layout.composite_cuts()[0].allow_restart);
    assert_eq!(candidate.layout.composite_cuts()[0].future_before, Some(32));
    assert_eq!(
        pipeline.state().loop_state(parent).unwrap().state.position,
        16
    );
    assert_no_heap(|| pipeline.switch_image(&mut candidate)).unwrap();
    pipeline
        .process(&mut jobs, &mut media, &[], &mut [])
        .unwrap();
    assert_eq!(
        pipeline.state().composite_status(parent).unwrap().version,
        3
    );
    assert_eq!(
        pipeline.state().loop_state(parent).unwrap().render.position,
        16
    );
}
