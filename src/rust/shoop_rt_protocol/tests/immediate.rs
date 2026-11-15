#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use shoop_rt_protocol::{
    codec,
    immediate::{Batch, Code, Command, CommandRecord, Header, Response, COMMANDS},
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
    assert_eq!(&input.bytes()[12..], &[0; 4]);
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
    invalid[12..].copy_from_slice(&999u32.to_le_bytes());
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
        Command::Start(id),
        Command::Stop(id),
    ];
    let message = ClientMessage::Immediate(Batch::new(1, 1, &commands).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
}
