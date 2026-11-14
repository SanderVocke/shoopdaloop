mod support;
use rt_runtime::midi::ShortMessage;
use rt_runtime::midi_state::{BasicUpdate, MidiState, BASIC_STATE_CELLS};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn fixed_state_steps_match_linear_basic_midi_for_pedals_modes_resets_and_all_channels() {
    let mut stepped = MidiState::default();
    let mut oracle = MidiState::default();
    assert_no_heap(|| {
        for channel in 0..16 {
            for packet in [
                [0xb0 | channel, 0, 3],
                [0xb0 | channel, 32, 4],
                [0xc0 | channel, 7, 0],
                [0xb0 | channel, 0, 9],
                [0x90 | channel, 60, 100],
                [0xb0 | channel, 66, 127],
                [0x90 | channel, 61, 81],
                [0xb0 | channel, 64, 127],
                [0x80 | channel, 60, 0],
                [0xa0 | channel, 61, 0],
                [0xb0 | channel, 69, 127],
                [0xb0 | channel, 64, 0],
                [0xb0 | channel, 66, 0],
                [0xb0 | channel, 69, 0],
                [0xb0 | channel, 124, 0],
                [0xb0 | channel, 125, 0],
                [0xb0 | channel, 126, 1],
                [0xb0 | channel, 127, 0],
                [0xb0 | channel, 121, 0],
                [0xb0 | channel, 120, 0],
            ] {
                let len = if packet[0] & 0xf0 == 0xc0 { 2 } else { 3 };
                let message = ShortMessage::managed(&packet[..len]).unwrap();
                let mut update = BasicUpdate::begin(message).unwrap();
                let mut steps = 0;
                while !update.complete() {
                    update.step(&mut stepped);
                    steps += 1;
                    assert!(steps <= BASIC_STATE_CELLS + 1);
                }
                oracle.apply_packet(message.as_bytes(), 4).unwrap();
                assert_eq!(stepped, oracle.basic_projection());
            }
        }
        let mut reset = BasicUpdate::begin(ShortMessage::managed(&[0xff]).unwrap()).unwrap();
        let mut steps = 0;
        while !reset.complete() {
            reset.step(&mut stepped);
            steps += 1;
        }
        assert_eq!(steps, BASIC_STATE_CELLS + 1);
        oracle.apply_packet(&[0xff], 4).unwrap();
        assert_eq!(stepped, oracle.basic_projection());
    });
}

#[shoop_test]
fn state_copy_progresses_by_one_fixed_cell_and_excludes_parameter_state() {
    let mut source = MidiState::default();
    source.apply_packet(&[0xb3, 7, 100], 3).unwrap();
    source.apply_packet(&[0xb3, 6, 17], 3).unwrap();
    source.apply_packet(&[0x93, 62, 88], 3).unwrap();
    let mut copied = MidiState::default();
    assert_no_heap(|| {
        for cell in 0..BASIC_STATE_CELLS {
            assert_eq!(
                copied.copy_basic_cell(Some(&source), cell),
                cell + 1 == BASIC_STATE_CELLS
            );
        }
        assert_eq!(copied, source.basic_projection());
        assert_eq!(copied.channel(3).unwrap().controller(6), None);
        assert!(!copied.channel(3).unwrap().parameter_history_untracked());
    });
}
