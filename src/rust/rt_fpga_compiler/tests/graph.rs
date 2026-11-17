use rt_fpga_compiler::{graph::*, CompileError, Identity};
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
fn graph_orders_sync_independently_of_audio_and_supports_sixteen_loops() {
    for quantum in [1, 16, 32, 64, 128] {
        let description = GraphDescription {
            quantum,
            loops: (0..16)
                .rev()
                .map(|i| row(i, (i > 0).then(|| i - 1)))
                .collect(),
        };
        let program = description.compile().unwrap();
        assert_eq!(
            program.sync_pass.iter().map(|i| i.slot).collect::<Vec<_>>(),
            (0..16).collect::<Vec<_>>()
        );
        assert_eq!(program.loops[0].id.slot, 15);
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
        (0..17).map(|i| row(i, None)).collect(),
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
fn stereo_graph_accepts_quantum_sized_chunks_and_preserves_legacy_geometry() {
    let description = GraphDescription {
        quantum: 32,
        loops: (0..64).map(|i| row(i, (i > 0).then_some(0))).collect(),
    };
    let mut program = description.compile_with_geometry(2, 32).unwrap();
    assert_eq!(program.chunk_frames, 32);
    assert_eq!(program.loops.len(), 64);
    program.chunk_frames = 1;
    assert_eq!(program, description.compile_with_channels(2).unwrap());
    for chunk_frames in [0, 3, 64] {
        assert_eq!(
            description.compile_with_geometry(2, chunk_frames),
            Err(CompileError::InvalidProgram)
        );
    }
}
