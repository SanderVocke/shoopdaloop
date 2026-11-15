#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use shoop_rt_protocol::{
    codec,
    immediate::{Batch, Code, Command, CommandRecord, Header, Response, TransitionMode, COMMANDS},
    ClientMessage, EngineMessage, MAX_RECORD_BYTES,
};
use shoop_wasm_test_support::shoop_test;

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
        Command::Transition(id, TransitionMode::Playing),
        Command::Transition(id, TransitionMode::Stopped),
        Command::Transition(id, TransitionMode::Recording),
    ];
    let message = ClientMessage::Immediate(Batch::new(1, 1, &commands).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
}

#[shoop_test]
fn recording_and_capture_failure_use_the_existing_result_layout() {
    let command = Command::Transition(12, TransitionMode::Recording);
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
        let command = Command::Transition(9, mode);
        let record = CommandRecord::new(command).complete(Code::Applied);
        assert_eq!(&record.bytes()[..4], &2u32.to_le_bytes());
        assert_eq!(&record.bytes()[8..12], &raw.to_le_bytes());
        assert_eq!(CommandRecord::from_bytes(*record.bytes()), Some(record));
        let message = ClientMessage::Immediate(Batch::new(7, 8, &[command]).unwrap());
        let len = codec::encode_client(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);

        for invalid_mode in [3u32, u32::MAX] {
            let mut invalid = *record.bytes();
            invalid[8..12].copy_from_slice(&invalid_mode.to_le_bytes());
            assert!(CommandRecord::from_bytes(invalid).is_none());
            bytes[len - 4..len].copy_from_slice(&invalid_mode.to_le_bytes());
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
        *CommandRecord::new(Command::Transition(9, TransitionMode::Stopped)).bytes();
    noncanonical[12] = 1;
    assert!(CommandRecord::from_bytes(noncanonical).is_none());
}
