use rt_fpga_compiler::{graph::*, Identity, StateProgramTarget};
use rt_fpga_reference::{
    compiler::RustTarget, fixed::Fixed, jobs::JobProcessor, pipeline::Pipeline, state::ModePlan,
    storage::MediaStore, transitions::Mode,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn compiled_graph_records_routes_and_sums_exact_samples() {
    for quantum in [1, 16, 128] {
        for count in [1, 2, 16] {
            let description = GraphDescription {
                quantum,
                loops: (0..count)
                    .map(|i| LoopDescription {
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
                    .collect(),
            };
            let logical = description.compile().unwrap();
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
