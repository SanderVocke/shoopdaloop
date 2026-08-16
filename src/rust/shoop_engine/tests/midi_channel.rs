//! Translation of `unit test test_MidiChannel.cpp`.

use assert2::check;
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::midi_channel::MidiChannel;
use shoop_engine::midi_storage::MidiStorageElem;

#[shoop_wasm_test_support::shoop_test]
fn midi_channel_set_contents_indefinite_size() {
    // Capacity for one message, then handed three: setting contents has to grow the
    // storage rather than drop what does not fit.
    let mut c = MidiChannel::with_capacity_elems(1, ChannelMode::Direct);

    let data = [
        MidiStorageElem::new(0, &[0, 1, 2]).expect("valid"),
        MidiStorageElem::new(1, &[3, 4, 5]).expect("valid"),
        MidiStorageElem::new(10, &[10]).expect("valid"),
    ];

    c.set_contents(&data, 1000, None);

    let got: Vec<(u32, Vec<u8>)> = c
        .contents()
        .iter()
        .map(|m| (m.time, m.data().to_vec()))
        .collect();
    let want: Vec<(u32, Vec<u8>)> = data.iter().map(|m| (m.time, m.data().to_vec())).collect();
    check!(got == want);
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
