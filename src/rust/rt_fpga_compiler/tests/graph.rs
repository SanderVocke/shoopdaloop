use rt_fpga_compiler::{graph::*, CompileError, Identity, ProgramLimits, Resource};
use shoop_wasm_test_support::shoop_test;

fn row(slot: u32, source: Option<u32>) -> LoopDescription {
    LoopDescription {
        id: Identity {
            slot,
            generation: 3,
        },
        source,
        input: true,
        output: true,
        gain: 0.5,
        repeat_sync: false,
    }
}

#[shoop_test]
fn graph_orders_sync_independently_of_audio_without_a_profile_loop_limit() {
    for quantum in [1, 16, 32, 64, 128] {
        let description = GraphDescription {
            quantum,
            loops: (0..64)
                .rev()
                .map(|i| row(i, (i > 0).then(|| i - 1)))
                .collect(),
        };
        let program = description.compile().unwrap();
        assert_eq!(
            program.sync_pass.iter().map(|i| i.slot).collect::<Vec<_>>(),
            (0..64).collect::<Vec<_>>()
        );
        assert_eq!(program.loops[0].id.slot, 63);
    }
    GraphDescription {
        quantum: 16,
        loops: vec![row(0, Some(0))],
    }
    .compile()
    .unwrap();
}

#[shoop_test]
fn invalid_graphs_fail_before_target_preparation() {
    for loops in [
        vec![],
        vec![row(0, Some(1))],
        vec![row(0, Some(1)), row(1, Some(0))],
        vec![row(0, None), row(0, None)],
        vec![LoopDescription {
            gain: f32::NAN,
            ..row(0, None)
        }],
        vec![LoopDescription {
            id: Identity {
                slot: 0,
                generation: 0,
            },
            ..row(0, None)
        }],
    ] {
        assert_eq!(
            GraphDescription { quantum: 16, loops }.compile(),
            Err(CompileError::InvalidProgram)
        );
    }
    assert_eq!(
        GraphDescription {
            quantum: 3,
            loops: vec![row(0, None)]
        }
        .compile(),
        Err(CompileError::InvalidProgram)
    );
}

#[shoop_test]
fn compiler_checks_each_target_resource_and_accepts_the_exact_fit() {
    let graph = GraphDescription {
        quantum: 32,
        loops: (0..40).map(|i| row(i + 1000, None)).collect(),
    };
    let program = graph.compile().unwrap();
    let exact = ProgramLimits {
        instructions: (program.sync_pass.len() + program.audio_pass.len()) as u32,
        state_values: program.initial_state.len() as u32,
        loop_slots: 40,
        jobs: program.resources.queue_capacity,
        buffers: program.resources.buffers,
        history_slots: 0,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    graph.compile_with_limits(exact).unwrap();
    for (resource, limits) in [
        (
            Resource::Instructions,
            ProgramLimits {
                instructions: exact.instructions - 1,
                ..exact
            },
        ),
        (
            Resource::State,
            ProgramLimits {
                state_values: exact.state_values - 1,
                ..exact
            },
        ),
        (
            Resource::Loops,
            ProgramLimits {
                loop_slots: 39,
                ..exact
            },
        ),
        (
            Resource::Jobs,
            ProgramLimits {
                jobs: exact.jobs - 1,
                ..exact
            },
        ),
        (
            Resource::Buffers,
            ProgramLimits {
                buffers: exact.buffers - 1,
                ..exact
            },
        ),
        (
            Resource::Inputs,
            ProgramLimits {
                input_ports: 0,
                ..exact
            },
        ),
        (
            Resource::Outputs,
            ProgramLimits {
                output_ports: 1,
                ..exact
            },
        ),
        (
            Resource::Quantum,
            ProgramLimits {
                quantum: 16,
                ..exact
            },
        ),
    ] {
        assert_eq!(
            graph.compile_with_limits(limits),
            Err(CompileError::Capacity(resource))
        );
    }
    assert_eq!(program.resources.queue_capacity, 204);
    assert_eq!(program.resources.buffers, 122);
    assert_eq!(program.resources.history_slots, 0);
}
