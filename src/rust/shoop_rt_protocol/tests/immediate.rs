#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use shoop_rt_protocol::{
    codec,
    immediate::{Batch, Code, Command, CommandRecord, Header, Response, TransitionMode, COMMANDS},
    ClientMessage, EngineMessage, LoopSlotId, SlotId, MAX_RECORD_BYTES,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn program_switch_descriptor_and_fenced_result_are_canonical_records() {
    let command = Command::SwitchProgram(shoop_rt_protocol::immediate::SwitchProgram {
        revision: 0x1234_5678_abcdef01,
        region: 1,
        instructions: 127,
        sync_count: 16,
        states: 64,
        jobs: 85,
        buffers: 50,
    });
    assert!(command.valid());
    let batch = Batch::new(41, 41, &[command]).unwrap();
    let mut bytes = [0; MAX_RECORD_BYTES];
    let message = ClientMessage::Immediate(batch);
    let n = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(codec::decode_client(&bytes[..n]), Ok(message));
    let record = CommandRecord::new(command).complete(Code::Applied);
    let message = EngineMessage::ImmediateResponse(
        Response::new(batch.header().complete(1024, Code::Applied), &[record]).unwrap(),
    );
    let n = codec::encode_engine(&message, &mut bytes).unwrap();
    assert_eq!(codec::decode_engine(&bytes[..n]), Ok(message));
    let mut invalid = *record.bytes();
    invalid[24] = 1;
    assert!(CommandRecord::from_bytes(invalid).is_none());
}

#[shoop_test]
fn prepared_graph_records_roundtrip_and_reject_reserved_bits() {
    use shoop_rt_protocol::immediate::{GraphLoop, IterationTransition};
    let commands = [
        Command::BeginGraph {
            quantum: 32,
            loops: 1,
        },
        Command::ConfigureLoop(GraphLoop {
            slot: 0,
            generation: 4,
            source: Some(0),
            input: Some(0),
            gain: 0.25,
            output: true,
            repeat_sync: true,
        }),
        Command::ActivateGraph,
        Command::LoopIteration(
            LoopSlotId(SlotId::new(9, 4)),
            IterationTransition {
                iteration: 0x1234_5678_90ab_cdef,
                retrigger: true,
                seek: true,
            },
        ),
        Command::LoopRepeatSync(LoopSlotId(SlotId::new(9, 4)), true),
    ];
    let message = ClientMessage::Immediate(Batch::new(0, 0, &commands).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let length = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(codec::decode_client(&bytes[..length]), Ok(message));
    for command in commands {
        assert!(command.valid());
        let record = CommandRecord::new(command);
        assert_eq!(CommandRecord::from_bytes(*record.bytes()), Some(record));
        let record = record.complete(Code::Applied);
        let response = EngineMessage::ImmediateResponse(
            Response::new(
                shoop_rt_protocol::immediate::Header::new(0, 0, None).complete(0, Code::Applied),
                &[record],
            )
            .unwrap(),
        );
        let length = codec::encode_engine(&response, &mut bytes).unwrap();
        assert_eq!(codec::decode_engine(&bytes[..length]), Ok(response));
    }
    let mut invalid = *CommandRecord::new(commands[1]).bytes();
    invalid[24] |= 4;
    assert!(CommandRecord::from_bytes(invalid).is_none());
    let mut invalid = *CommandRecord::new(Command::ActivateGraph).bytes();
    invalid[4] = 1;
    assert!(CommandRecord::from_bytes(invalid).is_none());
    let mut invalid = *CommandRecord::new(commands[3]).bytes();
    invalid[16] |= 4;
    assert!(CommandRecord::from_bytes(invalid).is_none());
    let mut invalid = *CommandRecord::new(commands[4]).bytes();
    invalid[12] = 2;
    assert!(CommandRecord::from_bytes(invalid).is_none());
    assert!(!Command::BeginGraph {
        quantum: 3,
        loops: 1
    }
    .valid());
}

#[shoop_test]
fn maximum_group_is_one_small_canonical_record() {
    let commands = [Command::LoopGain(1, 0.5); COMMANDS];
    let message = ClientMessage::Immediate(Batch::new(3, 4, &commands).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(len, 309);
    assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
    for prefix in 0..len {
        assert!(codec::decode_client(&bytes[..prefix]).is_err());
    }
}

#[shoop_test]
fn deadlines_and_fixed_result_records_roundtrip_without_semantic_preflight() {
    let batch = Batch::new(7, 8, &[Command::LoopGain(9, 0.25); COMMANDS]).unwrap();
    assert_eq!(batch.deadline, None);
    let deadline = batch.with_deadline(100);
    let message = ClientMessage::Immediate(deadline);
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(len, 317);
    assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
    let input = CommandRecord::new(Command::LoopGain(9, 0.25));
    assert_eq!(input.code(), Code::Unprocessed);
    assert_eq!(&input.bytes()[28..], &[0; 4]);
    assert_eq!(&input.bytes()[4..8], &9u32.to_le_bytes());
    let applied = input.complete(Code::Applied);
    assert_eq!(applied.command(), input.command());
    let header = deadline.header().complete(100, Code::Applied);
    assert_eq!(header.deadline(), Some(100));
    assert_eq!(header.frame(), 100);
    assert_eq!(Header::from_bytes(*header.bytes()), Some(header));
    let response = Response::new(header, &[applied; COMMANDS]).unwrap();
    assert!(Response::new(deadline.header(), &[applied]).is_none());
    assert!(Response::new(header, &[input]).is_none());
    let response = EngineMessage::ImmediateResponse(response);
    let len = codec::encode_engine(&response, &mut bytes).unwrap();
    assert!(len < MAX_RECORD_BYTES);
    assert_eq!(codec::decode_engine(&bytes[..len]).unwrap(), response);
    let mut invalid = *applied.bytes();
    invalid[28..].copy_from_slice(&999u32.to_le_bytes());
    assert!(CommandRecord::from_bytes(invalid).is_none());
    let mut invalid = *header.bytes();
    invalid[44] = 1;
    assert!(Header::from_bytes(invalid).is_none());
}

#[shoop_test]
fn groups_preserve_commands_without_semantic_preflight() {
    let id = 0;
    assert!(Batch::new(1, 1, &[]).is_none());
    assert!(Batch::new(1, 1, &[Command::LoopGain(id, f32::NAN)]).is_some());
    let commands = [
        Command::LoopBalance(id, 1.1),
        Command::Transition(id, TransitionMode::Playing.into()),
        Command::Transition(id, TransitionMode::Stopped.into()),
        Command::Transition(id, TransitionMode::Recording.into()),
    ];
    let message = ClientMessage::Immediate(Batch::new(1, 1, &commands).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
}

#[shoop_test]
fn recording_and_capture_failure_use_the_existing_result_layout() {
    let command = Command::Transition(12, TransitionMode::Recording.into());
    let record = CommandRecord::new(command).complete(Code::CaptureUnavailable);
    assert_eq!(record.bytes().len(), 32);
    assert_eq!(CommandRecord::from_bytes(*record.bytes()), Some(record));
    assert_eq!(record.command(), command);
    let header = Header::new(7, 8, None).complete(400, Code::Applied);
    let message = EngineMessage::ImmediateResponse(Response::new(header, &[record]).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_engine(&message, &mut bytes).unwrap();
    assert_eq!(codec::decode_engine(&bytes[..len]).unwrap(), message);
}

#[shoop_test]
fn transitions_share_one_opcode_and_reject_unknown_modes() {
    let mut bytes = [0; MAX_RECORD_BYTES];
    for (mode, raw) in [
        (TransitionMode::Stopped, 0u32),
        (TransitionMode::Playing, 1),
        (TransitionMode::Recording, 2),
    ] {
        let command = Command::Transition(9, mode.into());
        let record = CommandRecord::new(command).complete(Code::Applied);
        assert_eq!(&record.bytes()[..4], &2u32.to_le_bytes());
        assert_eq!(&record.bytes()[8..12], &raw.to_le_bytes());
        assert_eq!(CommandRecord::from_bytes(*record.bytes()), Some(record));
        let message = ClientMessage::Immediate(Batch::new(7, 8, &[command]).unwrap());
        let len = codec::encode_client(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);

        for invalid_mode in [3u32, 4] {
            let mut invalid = *record.bytes();
            invalid[8..12].copy_from_slice(&invalid_mode.to_le_bytes());
            assert!(CommandRecord::from_bytes(invalid).is_none());
            bytes[len - 20..len - 16].copy_from_slice(&invalid_mode.to_le_bytes());
            assert!(codec::decode_client(&bytes[..len]).is_err());
        }
        for removed_opcode in [3u32, 12] {
            let mut invalid = *record.bytes();
            invalid[..4].copy_from_slice(&removed_opcode.to_le_bytes());
            assert!(CommandRecord::from_bytes(invalid).is_none());
        }
    }
}

#[shoop_test]
fn maximum_grab_groups_and_responses_fit_one_transport_record() {
    use shoop_rt_protocol::immediate::{Grab, GrabGesture};
    let values = [
        Command::Grab(
            9,
            Grab {
                sync: Some(2),
                reverse_start_cycle: Some(0),
                cycles_length: Some(4),
                go_to_cycle: Some(2),
                mode: TransitionMode::Recording,
            },
        ),
        Command::Grab(
            9,
            Grab {
                sync: None,
                reverse_start_cycle: None,
                cycles_length: None,
                go_to_cycle: None,
                mode: TransitionMode::Stopped,
            },
        ),
        Command::GrabGesture(
            9,
            GrabGesture {
                synced: true,
                sync: 2,
                target: Some(4),
                cycles: 3,
                play_after_record: true,
            },
        ),
    ];
    let mut bytes = [0; MAX_RECORD_BYTES];
    for command in values {
        let input = ClientMessage::Immediate(Batch::new(7, 8, &[command; COMMANDS]).unwrap());
        let len = codec::encode_client(&input, &mut bytes).unwrap();
        assert!(len <= MAX_RECORD_BYTES);
        assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), input);
        let record = CommandRecord::new(command).complete(Code::Applied);
        assert_eq!(CommandRecord::from_bytes(*record.bytes()), Some(record));
        let header = Header::new(7, 8, None).complete(400, Code::Applied);
        let response =
            EngineMessage::ImmediateResponse(Response::new(header, &[record; COMMANDS]).unwrap());
        let len = codec::encode_engine(&response, &mut bytes).unwrap();
        assert!(len <= MAX_RECORD_BYTES);
        assert_eq!(codec::decode_engine(&bytes[..len]).unwrap(), response);
        for prefix in 0..len {
            assert!(codec::decode_engine(&bytes[..prefix]).is_err());
        }
    }
    let mut noncanonical =
        *CommandRecord::new(Command::Transition(9, TransitionMode::Stopped.into())).bytes();
    noncanonical[12] = 1;
    assert!(CommandRecord::from_bytes(noncanonical).is_none());
}

#[shoop_test]
fn complete_plans_clearing_and_observation_keep_the_value_envelope() {
    use shoop_rt_protocol::immediate::{ClearPlanned, PlannedTransition, TransitionPlan};
    let plan = TransitionPlan {
        now: None,
        first: Some(PlannedTransition {
            mode: TransitionMode::Recording,
            skip: 1,
        }),
        second: Some(PlannedTransition {
            mode: TransitionMode::Playing,
            skip: 5,
        }),
    };
    let mut bytes = [0; MAX_RECORD_BYTES];
    for command in [
        Command::Transition(8, plan),
        Command::Transition(8, TransitionPlan::default()),
        Command::ClearPlannedTransitions(8, ClearPlanned::First),
        Command::ClearPlannedTransitions(8, ClearPlanned::Second),
        Command::ClearPlannedTransitions(8, ClearPlanned::Both),
    ] {
        let message = ClientMessage::Immediate(Batch::new(7, 8, &[command; COMMANDS]).unwrap());
        let len = codec::encode_client(&message, &mut bytes).unwrap();
        assert!(len <= MAX_RECORD_BYTES);
        assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
        let record = CommandRecord::new(command).complete(Code::Scheduled);
        assert_eq!(CommandRecord::from_bytes(*record.bytes()), Some(record));
    }
    let record = shoop_rt_protocol::snapshot::Record::LoopPlan {
        id: shoop_rt_protocol::LoopSlotId(shoop_rt_protocol::SlotId::new(8, 3)),
        plan,
        failures: 4,
    };
    let len = codec::encode_snapshot(&record, &mut bytes).unwrap();
    assert_eq!(codec::decode_snapshot(&bytes[..len]).unwrap(), record);
}

#[shoop_test]
fn graph_resource_refusals_survive_the_wire_without_a_profile_size_limit() {
    let command = Command::BeginGraph {
        quantum: 32,
        loops: 1000,
    };
    assert!(command.valid());
    let batch = Batch::new(4, 4, &[command]).unwrap();
    for code in [
        Code::InstructionCapacity,
        Code::StateCapacity,
        Code::LoopCapacity,
        Code::JobCapacity,
        Code::BufferCapacity,
        Code::HistoryCapacity,
        Code::InputCapacity,
        Code::OutputCapacity,
        Code::QuantumCapacity,
    ] {
        let message = EngineMessage::ImmediateResponse(
            Response::new(
                batch.header().complete(64, code),
                &[CommandRecord::new(command).complete(code)],
            )
            .unwrap(),
        );
        let mut bytes = [0; MAX_RECORD_BYTES];
        let len = codec::encode_engine(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode_engine(&bytes[..len]), Ok(message));
    }
    assert!(
        Command::ConfigureLoop(shoop_rt_protocol::immediate::GraphLoop {
            slot: 1000,
            generation: 1,
            source: Some(2000),
            input: None,
            gain: 1.,
            output: true,
            repeat_sync: false,
        })
        .valid()
    );
}

#[shoop_test]
fn full_graph_transfer_records_and_detailed_refusals_have_canonical_wire_forms() {
    for command in [
        Command::BeginGraphImage {
            quantum: 32,
            bytes: 1000,
        },
        Command::GraphImageData {
            len: 20,
            bytes: [255; 20],
        },
    ] {
        assert!(command.valid());
        let message = ClientMessage::Immediate(Batch::new(4, 4, &[command]).unwrap());
        let mut bytes = [0; MAX_RECORD_BYTES];
        let len = codec::encode_client(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode_client(&bytes[..len]), Ok(message));
        let batch = Batch::new(4, 4, &[command]).unwrap();
        let message = EngineMessage::ImmediateResponse(
            Response::new(
                batch.header().complete(0, Code::GraphMidiUnsupported),
                &[CommandRecord::new(command).complete(Code::GraphMidiUnsupported)],
            )
            .unwrap(),
        );
        let len = codec::encode_engine(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode_engine(&bytes[..len]), Ok(message));
    }
    assert!(!Command::GraphImageData {
        len: 21,
        bytes: [0; 20]
    }
    .valid());
    assert!(!Command::GraphImageData {
        len: 1,
        bytes: [1; 20]
    }
    .valid());
}
