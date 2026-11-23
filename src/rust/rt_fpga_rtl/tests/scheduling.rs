#[path = "../../rt_fpga_compiler/tests/support/programs.rs"]
mod programs;

use rt_fpga_compiler::{MidiOperation as M, Operation, StateProgramTarget};
use rt_fpga_rtl::target::RtlTarget;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn audio_jobs_use_the_first_sync_slot_allowed_by_their_dependencies() {
    let mut program = programs::mono_loops(32, 3);
    let (a, b, c) = (
        program.loops[0].id,
        program.loops[1].id,
        program.loops[2].id,
    );
    program.work_pass = vec![
        Operation::Input {
            port: 0,
            destination: 0,
        },
        Operation::Playback {
            loop_id: b,
            view: 4,
            destination: 2,
        },
        Operation::Playback {
            loop_id: a,
            view: 3,
            destination: 1,
        },
        Operation::Mix {
            left: 1,
            right: 2,
            destination: 3,
        },
        Operation::Capture {
            loop_id: c,
            view: 5,
            source: 3,
        },
        Operation::Playback {
            loop_id: a,
            view: 3,
            destination: 4,
        },
        Operation::Gain {
            ramp: 0,
            source: 3,
            destination: 5,
        },
        Operation::Output { source: 5, port: 0 },
        Operation::Silence { destination: 5 },
    ];
    // The last playback aliases the physical view of the earlier capture.
    let target = RtlTarget::new([64, 16, 32, 32, 16, 2, 3], 3, &[7, 8, 7]);
    let compiled = target.build(&program).unwrap();
    assert_eq!(compiled.sync_count, 3);
    let instructions: Vec<_> = compiled
        .instructions
        .iter()
        .map(|word| (word.get(0, 8), word.get(8, 16)))
        .collect();
    assert_eq!(
        instructions,
        [
            (0, 0),  // Input
            (12, 0), // Sync A
            (6, 0),  // Read A
            (12, 1), // Sync B
            (6, 1),  // Read B
            (3, 1),  // Mix A and B
            (2, 0),  // Gain
            (5, 5),  // Output
            (8, 5),  // Overwrite scratch after its last reader
            (12, 2), // Sync C
            (7, 2),  // Capture C
            (6, 0),  // Read A after the aliased capture
        ]
    );
}

#[shoop_test]
fn midi_jobs_interleave_with_sync_and_keep_snapshot_list_and_packet_dependencies() {
    let mut program = programs::mono_loops(32, 3);
    let (a, b) = (program.loops[0].id, program.loops[1].id);
    program.work_pass.clear();
    program.jobs_per_quantum = 0;
    let program = program
        .with_midi_data(&[
            M::Playback {
                loop_id: b,
                sequence: 1,
                cursor: 1,
                channels: u16::MAX,
                destination: 2,
            },
            M::Playback {
                loop_id: a,
                sequence: 0,
                cursor: 0,
                channels: u16::MAX,
                destination: 1,
            },
            M::Merge {
                left: 1,
                right: 2,
                destination: 3,
            },
            M::Output { source: 3, port: 0 },
            M::Input {
                port: 0,
                destination: 0,
            },
            M::Capture {
                loop_id: b,
                sequence: 1,
                source: 0,
            },
            M::State {
                loop_id: a,
                sequence: 0,
                input: 0,
                outputs: 1,
                channels: u16::MAX,
            },
            M::Output { source: 1, port: 1 },
        ])
        .unwrap();
    let target = RtlTarget::new([64, 16, 32, 32, 16, 2, 3], 3, &[0, 1, 2]);
    let compiled = target.build(&program).unwrap();
    let instructions: Vec<_> = compiled
        .instructions
        .iter()
        .map(|word| (word.get(0, 8), word.get(8, 16)))
        .collect();
    assert_eq!(
        instructions,
        [
            (12, 0), // Sync A
            (15, 0), // Read A
            (12, 1), // Sync B
            (20, 1), // Prepare B's capture before observing input
            (19, 0), // Restore/cleanup after refreshing the output tracker
            (15, 1), // Read B after preparing its sequence
            (17, 1), // Merge A and B
            (18, 3), // Output merged list
            (14, 0), // Observe input after capture preparation
            (16, 1), // Append to B from the input list
            (18, 1), // Output A after the earlier output
            (12, 2), // Sync C
        ]
    );
}
