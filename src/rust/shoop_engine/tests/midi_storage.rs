//! One-for-one translation of `unit test test_MidiStorage.cpp`.
//!
//! than behaviour derived from reading the implementation.
//!
//! variable-length elements, so its cases size the buffer as
//! `n * sizeof(Storage::Elem)` and assert on `bytes_occupied` / `bytes_free`. This
//! padding. `n * sizeof(Elem)` becomes a capacity of `n` elements, and "no bytes
//! free" becomes `is_full`.

use assert2::check;
use shoop_engine::midi_storage::{MidiStorage, MidiStorageElem};

fn msg(time: u32, bytes: &[u8]) -> MidiStorageElem {
    MidiStorageElem::new(time, bytes).expect("valid message")
}

/// Everything a cursor yields, walking from the start until it wraps.
///
fn drain(s: &MidiStorage) -> Vec<MidiStorageElem> {
    let mut out = Vec::new();
    let mut cursor = s.create_cursor();
    while cursor.valid() {
        let Some(e) = cursor.get(s).copied() else {
            break;
        };
        out.push(e);
        cursor.next(s);
        if cursor.is_at_start(s) {
            break;
        }
    }
    out
}

/// Compares by time and payload. `MidiStorageElem` pads its byte array, so the
/// unused tail is not part of the message.
fn same(got: &[MidiStorageElem], want: &[MidiStorageElem]) -> bool {
    got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(a, b)| a.time == b.time && a.data() == b.data())
}

#[test]
fn midi_storage_round_trip() {
    let input = [msg(0, &[0, 1, 2]), msg(1, &[3, 4, 5]), msg(10, &[10])];
    let mut s = MidiStorage::with_capacity_elems(input.len());

    for i in &input {
        check!(s.append(i.time, i.data(), false, None));
    }

    check!(s.is_full());
    check!(s.n_events() == 3);

    check!(same(&drain(&s), &input));
}

#[test]
fn midi_storage_prepend() {
    let input = [msg(10, &[0, 1, 2]), msg(11, &[3, 4, 5])];
    let prepend = [msg(9, &[10]), msg(8, &[10])];
    let expected = [
        msg(8, &[10]),
        msg(9, &[10]),
        msg(10, &[0, 1, 2]),
        msg(11, &[3, 4, 5]),
    ];
    let mut s = MidiStorage::with_capacity_elems(input.len() + prepend.len());

    for i in &input {
        check!(s.append(i.time, i.data(), false, None));
    }
    // Prepended newest-first, so each one goes ahead of the last.
    for i in &prepend {
        check!(s.prepend(i.time, i.data()));
    }

    check!(s.is_full());
    check!(s.n_events() == 4);

    check!(same(&drain(&s), &expected));
}

#[test]
fn midi_storage_replace_append() {
    let input = [msg(0, &[0, 1, 2]), msg(1, &[3, 4, 5]), msg(10, &[10])];
    let append = msg(11, &[4, 5, 6]);
    let expected = [msg(1, &[3, 4, 5]), msg(10, &[10]), msg(11, &[4, 5, 6])];
    let mut s = MidiStorage::with_capacity_elems(input.len());

    for i in &input {
        check!(s.append(i.time, i.data(), false, None));
    }

    check!(s.is_full());
    check!(s.n_events() == 3);

    // A full buffer refuses, unless replacing the oldest message is allowed.
    check!(!s.append(append.time, append.data(), false, None));
    check!(s.append(append.time, append.data(), true, None));

    check!(s.is_full());
    check!(s.n_events() == 3);

    check!(same(&drain(&s), &expected));
}

#[test]
fn midi_storage_wrap_around() {
    let mut s = MidiStorage::with_capacity_elems(3);

    check!(s.append(0, &[0, 0, 0], false, None));
    check!(s.append(1, &[1, 1, 1], false, None));
    check!(s.append(2, &[2, 2, 2], false, None));
    check!(s.n_events() == 3);

    // Overwrites the oldest, at time 0.
    check!(s.append(3, &[3, 3, 3], true, None));
    check!(s.n_events() == 3);

    // And again, overwriting time 1.
    check!(s.append(4, &[4, 4, 4], true, None));
    check!(s.n_events() == 3);

    let expected = [msg(2, &[2, 2, 2]), msg(3, &[3, 3, 3]), msg(4, &[4, 4, 4])];
    check!(same(&drain(&s), &expected));
}
