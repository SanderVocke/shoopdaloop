#[path = "../../rt_fpga_compiler/tests/support/mono.rs"]
mod mono;
use rt_fpga_compiler::{compile, CompileContext};
use rt_fpga_compiler::{Identity, StateProgramTarget};
use rt_fpga_reference::{
    fixed::Fixed, jobs::JobProcessor, pipeline::Pipeline, state::ModePlan, storage::MediaStore,
    target::RustTarget, transitions::Mode,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn compiled_graph_records_routes_and_sums_exact_samples() {
    for quantum in [1, 16, 128] {
        for count in [1, 2, 16] {
            let description = mono::session(
                &((0..count)
                    .map(|i| mono::MonoLoop {
                        id: Identity {
                            slot: i,
                            generation: 2,
                        },
                        source: None,
                        input: i % 2 == 0,
                        output: i % 3 != 2,
                        gain: 0.5,
                        repeat_sync: false,
                    })
                    .collect::<Vec<_>>()),
            );
            let logical = compile(
                &description,
                CompileContext {
                    quantum,
                    limits: Default::default(),
                },
            )
            .unwrap();
            let mut media = MediaStore::new(1).unwrap();
            let views = (0..count)
                .map(|_| media.create_recording().unwrap().0 as u64)
                .collect::<Vec<_>>();
            let compiled = RustTarget::new(&views).build(&logical).unwrap();
            let r = compiled.resources;
            let ids = compiled.bindings.iter().map(|b| b.id).collect::<Vec<_>>();
            let mut pipeline = Pipeline::prepare(
                compiled.plan,
                compiled.program,
                compiled.memory,
                r.queue_capacity as usize,
            )
            .unwrap();
            let mut jobs = JobProcessor::with_history_slots(
                quantum as usize,
                r.buffers as usize,
                r.history_slots as usize,
            );
            let mut left = vec![Fixed::ZERO; quantum as usize];
            let mut right = left.clone();
            for &id in &ids {
                pipeline
                    .write_mode_plan(
                        id,
                        ModePlan {
                            now: Some(Mode::Recording),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            for amplitude in [0.25, 0.5] {
                let input = vec![Fixed::from_f32(amplitude); quantum as usize];
                pipeline
                    .process(
                        &mut jobs,
                        &mut media,
                        &[&input],
                        &mut [&mut left, &mut right],
                    )
                    .unwrap();
                assert!(left.iter().chain(&right).all(|&x| x == Fixed::ZERO));
            }
            for &id in &ids {
                pipeline
                    .write_mode_plan(
                        id,
                        ModePlan {
                            now: Some(Mode::Playing),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            for amplitude in [0.25, 0.5, 0.25, 0.5] {
                let input = vec![Fixed::ZERO; quantum as usize];
                pipeline
                    .process(
                        &mut jobs,
                        &mut media,
                        &[&input],
                        &mut [&mut left, &mut right],
                    )
                    .unwrap();
                let contributors = (0..count).filter(|i| i % 2 == 0 && i % 3 != 2).count();
                let expected = Fixed::from_f32(amplitude * 0.5 * contributors as f32);
                assert!(left.iter().chain(&right).all(|&x| x == expected), "Q={quantum} loops={count} amplitude={amplitude} expected={expected:?} actual={left:?}");
            }
        }
    }
}

#[shoop_test]
fn authored_recording_publishes_the_take_before_play_and_preserves_the_old_take_on_refusal() {
    use rt_fpga_compiler::shoop_graph::{CompositeKind, CompositeSpan, CompositeView, LoopMode};
    use rt_fpga_reference::{
        state::PlannedTransition,
        transitions::{LoopId, Operation},
    };

    let child = Identity {
        slot: 0,
        generation: 1,
    };
    let mut graph = mono::session(&[mono::MonoLoop {
        id: child,
        source: None,
        input: true,
        output: true,
        gain: 1.0,
        repeat_sync: false,
    }]);
    let mut parent = graph.loops[0].clone();
    parent.id.slot = 1;
    parent.strip = None;
    parent.composite = Some(CompositeView {
        kind: CompositeKind::Explicit,
        version: 1,
        unit_frames: 16,
        iterations: 3,
        sample_rate: 48000,
        quantum: 16,
        spans: vec![
            CompositeSpan {
                target: child,
                start: 0,
                end: 2,
                occurrence_start: 0,
                ordinal: 0,
                mode: LoopMode::Recording,
            },
            CompositeSpan {
                target: child,
                start: 2,
                end: 3,
                occurrence_start: 2,
                ordinal: 1,
                mode: LoopMode::Playing,
            },
        ]
        .into(),
    });
    graph.loops.push(parent);
    let logical = rt_fpga_compiler::compile_composites(
        &graph,
        CompileContext {
            quantum: 16,
            limits: Default::default(),
        },
        16,
    )
    .unwrap();

    let old = Fixed::from_f32(0.125);
    let first = Fixed::from_f32(0.25);
    let second = Fixed::from_f32(0.5);
    for (capacity, prior_take, denied, expected) in
        [(1, true, 1, old), (1, false, 1, first), (2, true, 0, first)]
    {
        let mut media = MediaStore::with_chunk_limit(16, capacity).unwrap();
        let view = if prior_take {
            media.insert(&[old; 16]).unwrap()
        } else {
            media.create_recording().unwrap()
        };
        let mut logical = logical.clone();
        if prior_take {
            logical.loops[0].length = 16;
        }
        let compiled = RustTarget::new(&[view.0 as u64]).build(&logical).unwrap();
        let resources = compiled.resources;
        let mut jobs = JobProcessor::new(16, resources.buffers as usize);
        let mut pipeline = Pipeline::prepare(
            compiled.plan,
            compiled.program,
            compiled.memory,
            resources.queue_capacity as usize,
        )
        .unwrap();
        pipeline
            .write_plans(
                LoopId {
                    slot: 1,
                    generation: 1,
                },
                [
                    Some(PlannedTransition {
                        sequence: 1,
                        operation: Operation::Play {
                            position: 0,
                            retrigger: true,
                        },
                        effective: 0,
                        on_pulse: false,
                        remaining_ticks: 0,
                    }),
                    None,
                ],
            )
            .unwrap();
        let mut left = [Fixed::ZERO; 16];
        let mut right = left;
        for (tick, input) in [first, second, Fixed::ZERO].into_iter().enumerate() {
            pipeline
                .process(
                    &mut jobs,
                    &mut media,
                    &[&[input; 16]],
                    &mut [&mut left, &mut right],
                )
                .unwrap();
            assert_eq!(left, [if tick == 2 { expected } else { Fixed::ZERO }; 16]);
            assert_eq!(right, left);
        }
        assert_eq!(pipeline.state().authored_capture_refusals(), denied);
        assert_eq!(
            media.frames(view).unwrap(),
            if denied == 0 { 32 } else { 16 }
        );
    }
}
