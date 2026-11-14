mod support;
use rt_runtime::midi::{
    EventRecord, MessageDisposition, MessagePolicy, PolicyRejection, ShortMessage,
};
use rt_runtime::{midi_state::MidiState, Error, Frame};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn short_policy_accepts_only_complete_basic_messages_on_every_channel() {
    assert_no_heap(|| {
        for status in 0u8..=255 {
            let expected = match status {
                0x80..=0xbf | 0xe0..=0xef => Some(3),
                0xc0..=0xdf => Some(2),
                0xff => Some(1),
                _ => None,
            };
            let packet = [status, 7, 23, 0, 0];
            for length in 0..=5 {
                assert_eq!(
                    ShortMessage::managed(&packet[..length]).is_ok(),
                    expected == Some(length),
                    "status {status:02x}, length {length}"
                );
            }
        }
        assert_eq!(
            ShortMessage::managed(&[0xf0, 0xf7]),
            Err(PolicyRejection::SysEx)
        );
        assert_eq!(ShortMessage::managed(&[0xf7]), Err(PolicyRejection::SysEx));
        assert_eq!(
            ShortMessage::managed(&[0x90, 60, 128]),
            Err(PolicyRejection::Malformed)
        );
    });
}

#[shoop_test]
fn parameter_forwarding_never_makes_a_record_or_changes_managed_state() {
    assert_no_heap(|| {
        let mut state = MidiState::default();
        ShortMessage::managed(&[0xb0, 7, 81])
            .unwrap()
            .apply(&mut state)
            .unwrap();
        let before = state;
        for channel in 0..16 {
            for controller in 0..128 {
                let packet = [0xb0 | channel, controller, 42];
                let excluded = matches!(controller, 6 | 38 | 96..=101);
                let disposition = MessagePolicy::LiveOnlyParameters.classify(&packet).unwrap();
                assert_eq!(
                    matches!(disposition, MessageDisposition::LiveOnly(_)),
                    excluded
                );
                assert_eq!(EventRecord::new(Frame(4), 0, &packet).is_err(), excluded);
                if excluded {
                    assert_eq!(
                        disposition.message().apply(&mut state),
                        Err(Error::InvalidParameter)
                    );
                    assert_eq!(state, before);
                }
            }
        }
        assert_eq!(state.channel(0).unwrap().controller(7), Some(81));
    });
}

#[shoop_test]
fn fixed_record_has_a_canonical_rtl_encoding_and_checked_order() {
    assert_no_heap(|| {
        let event = EventRecord::new(Frame(0x0807_0605_0403_0201), 0x1234, &[0x90, 60, 0]).unwrap();
        let expected = [1, 2, 3, 4, 5, 6, 7, 8, 0x90, 60, 0, 0, 3, 0, 0x34, 0x12];
        assert_eq!(event.encode(), expected);
        assert_eq!(EventRecord::decode(expected), Ok(event));
        for index in [11, 13] {
            let mut malformed = expected;
            malformed[index] = 1;
            assert_eq!(EventRecord::decode(malformed), Err(Error::InvalidEvent));
        }
        assert_eq!(
            EventRecord::new(Frame(0), 65536, &[0xff]),
            Err(Error::Exhausted)
        );
        let mut state = MidiState::default();
        event.message().apply(&mut state).unwrap();
        assert!(!state.channel(0).unwrap().note(60).unwrap().held());
    });
}
