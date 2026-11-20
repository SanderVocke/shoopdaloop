use rt_fpga_reference::midi::{Error, Event, Interpreter, Message, MidiState, Take, TakeStatus};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;

fn event(offset: u32, bytes: &[u8]) -> Event {
    Event {
        offset,
        message: Message::managed(bytes).unwrap(),
    }
}

#[shoop_test]
fn round_trip_preserves_bytes_equal_frame_order_and_seed_without_allocations() {
    let mut state = Interpreter::default();
    state
        .apply(Message::managed(&[0xb0, 64, 127]).unwrap())
        .unwrap();
    let mut take = Take::prepare(5).unwrap();
    let input = [
        event(0, &[0x90, 60, 50]),
        event(0, &[0x90, 60, 70]),
        event(15, &[0x90, 60, 0]),
    ];
    assert_no_heap(|| {
        take.begin(state.state().unwrap()).unwrap();
        take.append_window(16, &input).unwrap();
        take.append_window(16, &[event(0, &[0xb0, 64, 0])]).unwrap();
        take.finish().unwrap();
        let mut cursor = take.start_playback().unwrap();
        assert_eq!(take.begin(&MidiState::default()), Err(Error::Busy));
        assert_eq!(take.initial(), state.state().unwrap());
        for (i, (frame, msg)) in [
            (0, input[0].message),
            (0, input[1].message),
            (15, input[2].message),
            (16, Message::managed(&[0xb0, 64, 0]).unwrap()),
        ]
        .into_iter()
        .enumerate()
        {
            let record = cursor.peek(&take).unwrap().unwrap();
            assert_eq!(
                (record.frame, record.order, record.message),
                (frame, i as u32, msg)
            );
            cursor.acknowledge(&take).unwrap();
        }
        assert_eq!(cursor.peek(&take).unwrap(), None);
        cursor.rewind(&take).unwrap();
        assert_eq!(
            cursor.peek(&take).unwrap().unwrap().message,
            input[0].message
        );
        take.end_playback(cursor).unwrap();
        take.begin(&MidiState::default()).unwrap();
    });
}

#[shoop_test]
fn invalid_windows_are_atomic_and_boundary_events_belong_to_next_window() {
    let mut take = Take::prepare(3).unwrap();
    take.begin(&MidiState::default()).unwrap();
    for input in [
        [event(16, &[0x90, 60, 1]), event(16, &[0x90, 60, 2])],
        [event(15, &[0x90, 60, 1]), event(0, &[0x90, 60, 2])],
    ] {
        assert_eq!(take.append_window(16, &input), Err(Error::InvalidEvent));
        assert!(take.records().is_empty());
        assert_eq!(take.frames(), 0);
    }
    take.append_window(16, &[]).unwrap();
    take.append_window(16, &[event(0, &[0x90, 60, 1])]).unwrap();
    assert_eq!(take.records()[0].frame, 16);
}

#[shoop_test]
fn exhaustion_preserves_prefix_and_cannot_be_finalized_as_success() {
    let mut take = Take::prepare(1).unwrap();
    take.begin(&MidiState::default()).unwrap();
    let input = [event(0, &[0x90, 60, 50]), event(1, &[0x80, 60, 0])];
    assert_no_heap(|| {
        assert_eq!(take.append_window(16, &input), Err(Error::Capacity));
        assert_eq!(take.status(), TakeStatus::Incomplete);
        assert_eq!(take.records()[0].message, input[0].message);
        assert_eq!(take.frames(), 0);
        assert_eq!(take.finish(), Err(Error::Incomplete));
        assert!(matches!(take.start_playback(), Err(Error::Incomplete)));
    });
}

#[shoop_test]
fn a_silent_take_has_duration_and_can_play() {
    let mut take = Take::prepare(1).unwrap();
    take.begin(&MidiState::default()).unwrap();
    assert_eq!(take.begin(&MidiState::default()), Err(Error::Busy));
    take.append_window(128, &[]).unwrap();
    take.finish().unwrap();
    assert_eq!(take.frames(), 128);
    let cursor = take.start_playback().unwrap();
    assert_eq!(cursor.peek(&take).unwrap(), None);
    take.end_playback(cursor).unwrap();
}
