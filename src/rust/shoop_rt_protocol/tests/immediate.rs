#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use shoop_rt_protocol::{
    codec,
    immediate::{Batch, Command, COMMANDS},
    ClientMessage, MAX_RECORD_BYTES,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn maximum_group_is_one_small_canonical_record() {
    let commands = [Command::LoopGain(1, 0.5); COMMANDS];
    let message = ClientMessage::Immediate(Batch::new(3, 4, &commands).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(len, 308);
    assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
    for prefix in 0..len {
        assert!(codec::decode_client(&bytes[..prefix]).is_err());
    }
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
