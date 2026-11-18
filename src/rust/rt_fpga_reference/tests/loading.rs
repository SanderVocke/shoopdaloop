use rt_fpga_compiler::{
    graph::*, loading::Allocator, FixedProgram, Identity, LogicalStateProgram, Operation,
    ProgramLimits,
};
use rt_fpga_reference::{
    compiler::{ProgramImage, RustTarget},
    fixed::Fixed,
    jobs::JobProcessor,
    pipeline::Pipeline,
    state::{ModePlan, ModeStep},
    storage::MediaStore,
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
    MonoGraphFixture {
        quantum: 16,
        loops: ids
            .iter()
            .map(|&slot| LoopDescription {
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
            .collect(),
    }
    .compile()
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
    let logical = FixedProgram::SyncSmoothing { quantum: 16 }.emit().unwrap();
    let mut modified = logical.clone();
    for op in &mut modified.audio_pass {
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
