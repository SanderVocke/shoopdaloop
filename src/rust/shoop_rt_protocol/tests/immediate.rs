#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use shoop_rt_protocol::{
    codec,
    immediate::{Batch, Command, COMMANDS},
    ClientMessage, LoopSlotId, SlotId, MAX_RECORD_BYTES,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn maximum_group_is_one_small_canonical_record() {
    let commands = [Command::LoopGain(LoopSlotId(SlotId::new(1, 2)), 0.5); COMMANDS];
    let message = ClientMessage::Immediate(Batch::new(3, 4, &commands).unwrap());
    let mut bytes = [0; MAX_RECORD_BYTES];
    let len = codec::encode_client(&message, &mut bytes).unwrap();
    assert_eq!(len, 436);
    assert_eq!(codec::decode_client(&bytes[..len]).unwrap(), message);
    for prefix in 0..len {
        assert!(codec::decode_client(&bytes[..prefix]).is_err());
    }
}

#[shoop_test]
fn groups_refuse_bad_values_and_conflicting_playback() {
    let id = LoopSlotId(SlotId::new(0, 1));
    assert!(Batch::new(1, 1, &[]).is_none());
    assert!(Batch::new(1, 1, &[Command::LoopGain(id, f32::NAN)]).is_none());
    assert!(Batch::new(1, 1, &[Command::LoopBalance(id, 1.1)]).is_none());
    assert!(Batch::new(1, 1, &[Command::Start(id), Command::Stop(id)]).is_none());
}
