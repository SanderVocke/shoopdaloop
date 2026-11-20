#[path = "support/programs.rs"]
mod programs;
use rt_fpga_compiler::*;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn raw_state_programs_enforce_prepared_resource_limits() {
    for quantum in [1, 16, 32, 64, 128] {
        for program in [
            programs::mono_loops(quantum, 1),
            programs::mono_loops(quantum, 2),
            programs::sync_smoothing(quantum),
        ] {
            let exact = ProgramLimits {
                instructions: (program.sync_pass.len() + program.work_pass.len()) as u32,
                state_values: program.initial_state.len() as u32,
                loop_slots: program.loops.len() as u32,
                jobs: program.resources.queue_capacity,
                buffers: program.resources.buffers,
                history_slots: program.resources.history_slots,
                input_ports: program.resources.input_ports,
                output_ports: program.resources.output_ports,
                quantum,
            };
            program.validate(exact).unwrap();
            for resource in [
                Resource::Instructions,
                Resource::State,
                Resource::Loops,
                Resource::Jobs,
                Resource::Buffers,
                Resource::History,
                Resource::Inputs,
                Resource::Outputs,
                Resource::Quantum,
            ] {
                let mut insufficient = exact;
                match resource {
                    Resource::Instructions => insufficient.instructions -= 1,
                    Resource::State => insufficient.state_values -= 1,
                    Resource::Loops => insufficient.loop_slots -= 1,
                    Resource::Jobs => insufficient.jobs -= 1,
                    Resource::Buffers => insufficient.buffers -= 1,
                    Resource::History => insufficient.history_slots -= 1,
                    Resource::Inputs => insufficient.input_ports -= 1,
                    Resource::Outputs => insufficient.output_ports -= 1,
                    Resource::Quantum => insufficient.quantum -= 1,
                }
                assert_eq!(
                    program.validate(insufficient),
                    Err(CompileError::Capacity(resource))
                );
            }
        }
    }
}

#[shoop_test]
fn malformed_program_parameters_and_references_are_rejected() {
    assert_eq!(
        programs::mono_loops(3, 1).validate(ProgramLimits::default()),
        Err(CompileError::InvalidProgram)
    );
    let program = programs::mono_loops(16, 2);
    for mutation in 0..5 {
        let mut invalid = program.clone();
        match mutation {
            0 => invalid.sync_pass.reverse(),
            1 => invalid.bindings[0].gain = Some(2),
            2 => {
                invalid.work_pass[0] = Operation::Input {
                    port: 1,
                    destination: 0,
                }
            }
            3 => invalid.bindings[0].meter = Some(100),
            _ => invalid.loops[0].id.generation = 0,
        }
        assert_eq!(
            invalid.validate(ProgramLimits::default()),
            Err(CompileError::InvalidProgram)
        );
    }
}

#[shoop_test]
fn mixed_programs_validate_phase_order_stable_contexts_and_queue_bounds() {
    let audio = programs::mono_loops(32, 2);
    let mixed = audio
        .clone()
        .with_midi(&[
            MidiBinding {
                loop_id: audio.loops[1].id,
                context: 7,
            },
            MidiBinding {
                loop_id: audio.loops[0].id,
                context: 3,
            },
        ])
        .unwrap();
    mixed.validate(ProgramLimits::default()).unwrap();
    assert_eq!(mixed.jobs_per_quantum, audio.jobs_per_quantum + 4 + 8 * 2);
    for change in 0..5 {
        let mut bad = mixed.clone();
        let start = audio.work_pass.len();
        match change {
            0 => bad.work_pass.swap(start, start + 1),
            1 => bad.work_pass.push(bad.work_pass[start]),
            2 => {
                bad.work_pass[start + 1] = Operation::Midi {
                    phase: MidiPhase::Capture,
                    loop_id: Some(audio.loops[0].id),
                    context: 7,
                }
            }
            3 => {
                bad.work_pass.remove(start + 1);
            }
            _ => {
                bad.jobs_per_quantum -= 1;
            }
        };
        assert_eq!(
            bad.validate(ProgramLimits::default()),
            Err(CompileError::InvalidProgram)
        );
    }
    assert_eq!(
        audio.with_midi(&[
            MidiBinding {
                loop_id: mixed.loops[0].id,
                context: 3
            },
            MidiBinding {
                loop_id: mixed.loops[1].id,
                context: 3
            },
        ]),
        Err(CompileError::InvalidProgram)
    );
}

#[shoop_test]
fn explicit_midi_data_jobs_validate_resources_and_reject_conflicting_writers() {
    let mut audio = programs::mono_loops(32, 2);
    audio.resources.buffers = 8;
    let operations = [
        MidiOperation::Input {
            port: 0,
            destination: 0,
        },
        MidiOperation::Capture {
            loop_id: audio.loops[0].id,
            sequence: 0,
            source: 0,
        },
        MidiOperation::Playback {
            loop_id: audio.loops[0].id,
            sequence: 0,
            cursor: 0,
            channels: 1,
            destination: 1,
        },
        MidiOperation::Output { source: 1, port: 0 },
    ];
    let program = audio.clone().with_midi_data(&operations).unwrap();
    assert_eq!(program.jobs_per_quantum, audio.jobs_per_quantum + 6);
    program.validate(ProgramLimits::default()).unwrap();
    for change in 0..5 {
        let mut invalid = program.clone();
        match change {
            0 => invalid.jobs_per_quantum -= 1,
            1 => invalid.work_pass.push(Operation::MidiData(operations[1])),
            2 => invalid.work_pass.push(Operation::MidiData(operations[2])),
            3 => invalid
                .work_pass
                .push(Operation::MidiData(MidiOperation::Merge {
                    left: 0,
                    right: 1,
                    destination: 0,
                })),
            _ => invalid
                .work_pass
                .push(Operation::MidiData(MidiOperation::Playback {
                    loop_id: audio.loops[1].id,
                    sequence: 1,
                    cursor: 1,
                    channels: 0,
                    destination: 2,
                })),
        }
        invalid.resources.queue_capacity += 4;
        assert_eq!(
            invalid.validate(ProgramLimits::default()),
            Err(CompileError::InvalidProgram)
        );
    }
}

#[shoop_test]
fn midi_buffers_and_ports_are_independent_of_audio_resources() {
    let audio = programs::mono_loops(32, 1);
    let operations = [
        MidiOperation::Input {
            port: 7,
            destination: 200,
        },
        MidiOperation::Capture {
            loop_id: audio.loops[0].id,
            sequence: 0,
            source: 200,
        },
        MidiOperation::Playback {
            loop_id: audio.loops[0].id,
            sequence: 0,
            cursor: 0,
            channels: 1,
            destination: 201,
        },
        MidiOperation::Merge {
            left: 200,
            right: 201,
            destination: 202,
        },
        MidiOperation::Output {
            source: 202,
            port: 7,
        },
    ];
    audio
        .clone()
        .with_midi_data(&operations)
        .unwrap()
        .validate(ProgramLimits::default())
        .unwrap();
    let mut midi_only = audio.clone();
    midi_only.work_pass.clear();
    midi_only.jobs_per_quantum = 0;
    midi_only.resources.queue_capacity = 0;
    midi_only.resources.buffers = 0;
    midi_only.resources.input_ports = 0;
    midi_only.resources.output_ports = 0;
    for binding in &mut midi_only.bindings {
        binding.meter = None;
    }
    midi_only
        .with_midi_data(&operations)
        .unwrap()
        .validate(ProgramLimits::default())
        .unwrap();
    let mut invalid_audio = audio;
    invalid_audio.work_pass[0] = Operation::Input {
        port: 7,
        destination: 0,
    };
    assert_eq!(
        invalid_audio.validate(ProgramLimits::default()),
        Err(CompileError::InvalidProgram)
    );
}
