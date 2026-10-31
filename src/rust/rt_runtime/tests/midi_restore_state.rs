mod support;
use rt_runtime::midi_state::{MidiState, ParameterFamily};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
#[shoop_test]
fn selector_order_relative_commands_and_opposed_modes_have_restorable_metadata() {
    let mut state = MidiState::default();
    assert_no_heap(|| {
        assert_eq!(state.channel(0).unwrap().parameter_family(), None);
        for packet in [
            [0xb0, 101, 1],
            [0xb0, 100, 2],
            [0xb0, 99, 3],
            [0xb0, 98, 4],
            [0xb0, 6, 5],
            [0xb0, 38, 6],
            [0xb0, 96, 0],
            [0xb0, 97, 127],
        ] {
            state.apply_packet(&packet, 3).unwrap();
        }
        let channel = state.channel(0).unwrap();
        assert_eq!(channel.parameter_family(), Some(ParameterFamily::Nrpn));
        assert!(channel.parameter_history_untracked());
        assert_eq!(channel.controller(6), Some(5));
        assert_eq!(channel.controller(38), Some(6));
        assert_eq!(channel.controller(96), None);
        assert_eq!(channel.controller(97), None);
        state.apply_packet(&[0xb0, 100, 7], 3).unwrap();
        assert_eq!(
            state.channel(0).unwrap().parameter_family(),
            Some(ParameterFamily::Rpn)
        );
        for (first, second) in [(124, 125), (125, 124), (126, 127), (127, 126)] {
            state.apply_packet(&[0xb0, first, 0], 3).unwrap();
            state.apply_packet(&[0xb0, second, 1], 3).unwrap();
            assert_eq!(state.channel(0).unwrap().controller(first), None);
            assert_eq!(state.channel(0).unwrap().controller(second), Some(1));
        }
        state.apply_packet(&[0xb0, 121, 0], 3).unwrap();
        assert_eq!(
            state.channel(0).unwrap().parameter_family(),
            Some(ParameterFamily::Rpn)
        );
        for cc in 98..=101 {
            assert_eq!(state.channel(0).unwrap().controller(cc), Some(127));
        }
        assert!(state.channel(0).unwrap().parameter_history_untracked());
        state.apply_packet(&[0xff], 3).unwrap();
        assert!(!state.channel(0).unwrap().parameter_history_untracked());
        assert_eq!(state.channel(0).unwrap().parameter_family(), None);
    });
}
#[shoop_test]
fn program_bank_snapshot_is_distinct_from_pending_bank_selectors() {
    let mut state = MidiState::default();
    assert_no_heap(|| {
        state.apply_packet(&[0xc0, 9], 3).unwrap();
        assert_eq!(state.channel(0).unwrap().program_bank(), [None, None]);
        state.apply_packet(&[0xb0, 0, 1], 3).unwrap();
        state.apply_packet(&[0xb0, 32, 2], 3).unwrap();
        state.apply_packet(&[0xc0, 10], 3).unwrap();
        let snapshot = state;
        state.apply_packet(&[0xb0, 0, 3], 3).unwrap();
        state.apply_packet(&[0xb0, 32, 4], 3).unwrap();
        assert_eq!(state.channel(0).unwrap().program_bank(), [Some(1), Some(2)]);
        assert_eq!(
            snapshot.channel(0).unwrap().program_bank(),
            [Some(1), Some(2)]
        );
        state.apply_packet(&[0xb0, 121, 0], 3).unwrap();
        assert_eq!(state.channel(0).unwrap().program_bank(), [Some(1), Some(2)]);
        state.apply_packet(&[0xc0, 11], 3).unwrap();
        assert_eq!(state.channel(0).unwrap().program_bank(), [Some(3), Some(4)]);
        state.apply_packet(&[0xff], 3).unwrap();
        assert_eq!(state.channel(0).unwrap().program_bank(), [None, None]);
    });
}
