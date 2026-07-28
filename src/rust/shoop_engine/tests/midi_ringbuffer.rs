//! Translation of `legacy C++ backend unit test test_MidiRingbuffer.cpp`.
//!
//! `MidiRingbuffer(sizeof(Storage::Elem) * 3)` becomes a capacity of 3 elements, for
//! the same reason as in `midi_storage.rs`: this storage counts fixed-size elements
//! rather than bytes.

use assert2::check;
use shoop_engine::midi_ringbuffer::MidiRingbuffer;
use shoop_engine::midi_storage::MidiStorage;

/// Messages as (time, payload), oldest first.
fn msgs(s: &MidiStorage) -> Vec<(u32, Vec<u8>)> {
    s.iter().map(|m| (m.time, m.data().to_vec())).collect()
}

fn contents(b: &MidiRingbuffer) -> Vec<(u32, Vec<u8>)> {
    msgs(b.storage())
}

#[test]
fn midi_ringbuffer_put_and_increment() {
    let mut b = MidiRingbuffer::with_capacity_elems(200);
    b.set_n_samples(50);
    b.next_buffer(10, None);

    check!(b.put(0, &[0, 0, 0], None));
    check!(b.put(1, &[1, 1, 1], None));
    b.next_buffer(10, None);
    check!(b.put(2, &[2, 2, 2], None));
    b.next_buffer(10, None);
    check!(b.put(3, &[3, 3, 3], None));

    check!(b.n_events() == 4);
    // Times are stored against the ringbuffer's own timeline, so each buffer's
    // offsets are shifted by where that buffer started.
    check!(
        contents(&b)
            == vec![
                (0, vec![0, 0, 0]),
                (1, vec![1, 1, 1]),
                (12, vec![2, 2, 2]),
                (23, vec![3, 3, 3]),
            ]
    );
}

#[test]
fn midi_ringbuffer_put_and_truncate() {
    // A 17-sample window, so advancing past that drops what fell out of it.
    let mut b = MidiRingbuffer::with_capacity_elems(200);
    b.set_n_samples(17);
    b.next_buffer(10, None);

    check!(b.put(0, &[0, 0, 0], None));
    check!(b.put(1, &[1, 1, 1], None));
    b.next_buffer(10, None);
    check!(b.put(2, &[2, 2, 2], None));
    check!(b.put(3, &[3, 3, 3], None));
    b.next_buffer(10, None);
    check!(b.put(3, &[4, 4, 4], None));

    // The window now ends at 30 and so starts at 13.
    check!(b.n_events() == 2);
    check!(contents(&b) == vec![(13, vec![3, 3, 3]), (23, vec![4, 4, 4])]);
}

#[test]
fn midi_ringbuffer_put_and_wrap() {
    // Room for exactly three messages, and a window long enough that nothing is
    // dropped for being old: only capacity forces anything out.
    let mut b = MidiRingbuffer::with_capacity_elems(3);
    b.set_n_samples(10000);
    b.next_buffer(10, None);

    check!(b.put(0, &[0, 0, 0], None));
    check!(b.put(1, &[1, 1, 1], None));
    check!(b.put(2, &[2, 2, 2], None));
    // Each further message overwrites the oldest.
    check!(b.put(3, &[3, 3, 3], None));
    check!(b.put(4, &[4, 4, 4], None));
    b.next_buffer(10, None);

    check!(b.n_events() == 3);
    check!(contents(&b) == vec![(2, vec![2, 2, 2]), (3, vec![3, 3, 3]), (4, vec![4, 4, 4])]);
}

#[test]
fn midi_ringbuffer_put_and_wrap_then_truncate() {
    let mut b = MidiRingbuffer::with_capacity_elems(3);
    b.set_n_samples(17);
    b.next_buffer(10, None);

    check!(b.put(0, &[0, 0, 0], None));
    check!(b.put(1, &[1, 1, 1], None));
    check!(b.put(2, &[2, 2, 2], None));
    check!(b.put(3, &[3, 3, 3], None));
    check!(b.put(4, &[4, 4, 4], None));
    b.next_buffer(10, None);

    // Capacity dropped two, then the 17-sample window starting at 3 drops one more.
    check!(b.n_events() == 2);
    check!(contents(&b) == vec![(3, vec![3, 3, 3]), (4, vec![4, 4, 4])]);
}

#[test]
fn midi_ringbuffer_put_then_overflow_then_snapshot() {
    let mut b = MidiRingbuffer::with_capacity_elems(3);
    b.set_n_samples(17);

    // Advance to just short of overflowing the 32-bit time, so the next buffer
    // forces a rebase.
    //
    // In one step, not in 512-frame steps. The C++ case looks like it steps, but its
    // `std::min(512, (int)(target - end))` casts a value near 2^32 to a negative
    // `int`, so the first call consumes the whole distance and the current buffer
    // still starts at 0. Its expected values depend on that, so stepping properly
    // here would leave the messages at a different base and compare against nothing
    // meaningful.
    let target = u32::MAX - 2;
    b.next_buffer(target, None);
    check!(b.current_end_time() == target);
    check!(b.current_buffer_start_time() == 0);

    check!(b.put(0, &[0, 0, 0], None));
    check!(b.put(2, &[1, 1, 1], None));
    check!(b.put(5, &[2, 2, 2], None));

    b.next_buffer(10, None);

    let mut copy = MidiStorage::with_capacity_elems(b.storage().capacity_elems());
    b.snapshot(&mut copy, Some(8));

    // Rebased so the window starts at 0 again, keeping the intervals intact.
    check!(b.n_events() == 3);
    check!(
        contents(&b)
            == vec![
                (10, vec![0, 0, 0]),
                (12, vec![1, 1, 1]),
                (15, vec![2, 2, 2])
            ]
    );

    check!(copy.n_events() == 3);
    check!(msgs(&copy) == vec![(1, vec![0, 0, 0]), (3, vec![1, 1, 1]), (6, vec![2, 2, 2])]);
}

#[test]
fn midi_ringbuffer_put_then_truncated_snapshot() {
    let mut b = MidiRingbuffer::with_capacity_elems(3);
    b.set_n_samples(20);
    b.next_buffer(10, None);

    check!(b.put(0, &[0, 0, 0], None));
    check!(b.put(2, &[1, 1, 1], None));
    check!(b.put(5, &[2, 2, 2], None));

    b.next_buffer(10, None);

    // A snapshot of only the last 17 samples, which begins at time 3.
    let mut copy = MidiStorage::with_capacity_elems(b.storage().capacity_elems());
    b.snapshot(&mut copy, Some(17));

    check!(b.n_events() == 3);
    check!(contents(&b) == vec![(0, vec![0, 0, 0]), (2, vec![1, 1, 1]), (5, vec![2, 2, 2])]);

    check!(copy.n_events() == 1);
    check!(msgs(&copy) == vec![(2, vec![2, 2, 2])]);
}
