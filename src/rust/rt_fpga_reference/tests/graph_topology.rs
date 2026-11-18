#[path = "../../rt_fpga_compiler/tests/support/mod.rs"]
mod support;
use rt_fpga_compiler::{
    graph::{compile, CompileContext},
    loading::Allocator,
    ProgramLimits, StateProgramTarget,
};
use rt_fpga_reference::{
    compiler::RustTarget,
    fixed::{Fixed, Ramp},
    jobs::JobProcessor,
    pipeline::Pipeline,
    state::{ModePlan, Value},
    storage::MediaStore,
    transitions::{LoopId, Mode},
};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[shoop_test]
fn compiled_multichannel_strips_and_bus_chains_match_independent_sample_calculations() {
    for channels in [1, 2, 4] {
        for quantum in [1, 16, 128] {
            let graph = session(channels);
            let logical = compile(
                &graph,
                CompileContext {
                    quantum,
                    limits: ProgramLimits::default(),
                },
            )
            .unwrap();
            let mut media = MediaStore::new(1).unwrap();
            let views = (0..3 * channels)
                .map(|_| media.create_recording().unwrap().0 as u64)
                .collect::<Vec<_>>();
            let compiled = RustTarget::new(&views).build(&logical).unwrap();
            let ids = compiled.bindings.iter().map(|b| b.id).collect::<Vec<_>>();
            let gain = compiled.bindings[0].gain.unwrap();
            let r = compiled.resources;
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
            let mut recorded = vec![];
            for tick in 0..8 {
                if tick == 0 || tick == 2 {
                    for &id in &ids {
                        pipeline
                            .write_mode_plan(
                                id,
                                ModePlan {
                                    now: Some(if tick == 0 {
                                        Mode::Recording
                                    } else {
                                        Mode::Playing
                                    }),
                                    ..Default::default()
                                },
                            )
                            .unwrap();
                    }
                }
                if tick == 6 {
                    pipeline
                        .write_state(gain, Value::Ramp(Ramp::constant(Fixed::from_f32(0.25))))
                        .unwrap();
                }
                let input = (0..channels)
                    .map(|ch| {
                        (0..quantum)
                            .map(|frame| {
                                Fixed::from_f32((1 + ch * 8 + frame % 7 + tick) as f32 / 128.0)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                if tick < 2 {
                    recorded.push(input.clone());
                }
                let refs = input.iter().map(Vec::as_slice).collect::<Vec<_>>();
                let mut output = vec![vec![Fixed::ZERO; quantum as usize]; channels as usize];
                let mut outs = output.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();
                pipeline
                    .process(&mut jobs, &mut media, &refs, &mut outs)
                    .unwrap();
                for ch in 0..channels as usize {
                    for frame in 0..quantum as usize {
                        let expected = if tick < 2 {
                            input[ch][frame]
                        } else {
                            let stored = recorded[((tick - 2) % 2) as usize][ch][frame];
                            let gain = if tick >= 6 { 0.25 } else { 0.5 };
                            stored
                                .multiply(Fixed::from_f32(gain))
                                .saturating_add(stored.multiply(Fixed::from_f32(0.25)))
                                .saturating_add(stored)
                                .saturating_add(input[ch][frame])
                        };
                        assert_eq!(
                            output[ch][frame], expected,
                            "channels={channels} Q={quantum} tick={tick} ch={ch} frame={frame}"
                        );
                    }
                }
            }
        }
    }
}
#[shoop_test]
fn live_graph_changes_preserve_each_channel_recording_and_only_initialize_new_configuration() {
    let limits = ProgramLimits {
        loop_slots: 8,
        state_values: 128,
        instructions: 512,
        jobs: 512,
        buffers: 256,
        history_slots: 32,
        ..Default::default()
    };
    let mut graph = session(2);
    graph.strips[1].controls.input_monitoring = false;
    let first = compile(
        &graph,
        CompileContext {
            quantum: 16,
            limits,
        },
    )
    .unwrap();
    let mut allocator = Allocator::new(limits);
    let first_layout = allocator.stage(&first).unwrap();
    let mut media = MediaStore::new(1).unwrap();
    for _ in 0..limits.state_values {
        media.create_recording().unwrap();
    }
    let mut target = RustTarget::new(&[]);
    target.limits = limits;
    let views = first_layout.media_views(&first);
    target.media_views = &views;
    let image = target.build_image(&first, first_layout.clone()).unwrap();
    allocator.publish(first_layout.revision).unwrap();
    allocator.complete(first_layout.revision).unwrap();
    let mut pipeline = Pipeline::from_image(image, limits).unwrap();
    let ids = graph
        .loops
        .iter()
        .map(|l| {
            let id = first_layout.loop_id(l.id).unwrap();
            LoopId {
                slot: id.slot,
                generation: id.generation,
            }
        })
        .collect::<Vec<_>>();
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
    let mut jobs = JobProcessor::with_history_slots(16, 256, 32);
    let a = vec![Fixed::from_f32(0.25); 16];
    let b = vec![Fixed::from_f32(0.5); 16];
    let mut out_a = vec![Fixed::ZERO; 16];
    let mut out_b = out_a.clone();
    for _ in 0..2 {
        pipeline
            .process(
                &mut jobs,
                &mut media,
                &[&a, &b],
                &mut [&mut out_a, &mut out_b],
            )
            .unwrap();
    }
    graph.buses[0].gain_db = -6.0206;
    graph.strips[0].controls.input_gain_db = -6.;
    graph.strips.reverse();
    graph.ports.reverse();
    let next = compile(
        &graph,
        CompileContext {
            quantum: 16,
            limits,
        },
    )
    .unwrap();
    let next_layout = allocator.stage(&next).unwrap();
    assert_eq!(next_layout.new_media_views().count(), 0);
    assert_eq!(next_layout.media_views(&next), views);
    assert_eq!(next_layout.new_loops().count(), 0);
    assert_eq!(next_layout.new_values().count(), 4);
    for binding in &next.bindings {
        let previous = first.bindings.iter().find(|b| b.id == binding.id).unwrap();
        assert_eq!(
            next_layout.state_address(binding.gain.unwrap()),
            first_layout.state_address(previous.gain.unwrap())
        );
    }
    let next_views = next_layout.media_views(&next);
    target.media_views = &next_views;
    let mut image = target.build_image(&next, next_layout.clone()).unwrap();
    let old_addresses = first
        .state_bindings
        .iter()
        .map(|b| first_layout.state_address(b.address))
        .collect::<Vec<_>>();
    assert!(image
        .initializers
        .iter()
        .all(|(a, _)| !old_addresses.contains(&(a.0 as u32))));
    pipeline.switch_image(&mut image).unwrap();
    allocator.publish(next_layout.revision).unwrap();
    allocator.complete(next_layout.revision).unwrap();
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
    let zero = vec![Fixed::ZERO; 16];
    for _ in 0..6 {
        pipeline
            .process(
                &mut jobs,
                &mut media,
                &[&zero, &zero],
                &mut [&mut out_a, &mut out_b],
            )
            .unwrap();
        for (output, amplitude) in [(&out_a, 0.25), (&out_b, 0.5)] {
            let expected = Fixed::from_f32(amplitude * 1.375);
            assert!(
                output.iter().all(|s| (s.0 - expected.0).abs() <= 2),
                "{output:?}"
            );
        }
    }
    assert!(matches!(
        allocator.stage(
            &compile(
                &session(3),
                CompileContext {
                    quantum: 16,
                    limits
                }
            )
            .unwrap()
        ),
        Err(rt_fpga_compiler::loading::LoadError::IncompatibleState)
    ));
}
