//! The MIDI half of `unit test test_JackPorts.cpp`, re-pointed at the
//! shared port core.
//!
//! Those cases open a JACK port against a fake JACK API (`JackTestApi`) and inject
//! messages into its `internal_port.midi_buffer`, one buffer per cycle. There is no
//! JACK driver here yet, but what they actually exercise -- mute, event counters,
//! note tracking, output ordering -- is the `MidiPort` core that a JACK port and the
//! dummy port would both delegate to, and `MidiPort::process` takes a cycle's events
//! directly, which is exactly the shape of the JACK mock's buffer.
//!
//! Their input and output variants collapse into one case each here, because there is
//! a single core rather than a port class per direction.
//!
//! Not covered, and still owed once a driver exists: the JACK plumbing itself -- port
//! registration, reading and writing its buffers, and the direction-dependent access
//! flags, which are a property of the port type rather than of this core. (The dummy
//! port is no substitute for that: it hard-codes all four access flags to true, as
//! being a fresh per-cycle buffer.)
//!
//! The audio half of that file duplicates `tests/dummy_ports.rs` almost exactly, so
//! it is not repeated here.

use assert2::check;
use shoop_engine::midi;
use shoop_engine::midi_port::MidiPort;
use shoop_engine::midi_sorting_buffer::MidiSortingBuffer;
use shoop_engine::midi_state::TrackWhat;
use shoop_engine::midi_storage::MidiStorageElem;

fn port() -> MidiPort {
    MidiPort::new(TrackWhat::ALL)
}

fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
    MidiStorageElem::new(time, data).expect("valid message")
}

fn pairs(msgs: &[MidiStorageElem]) -> Vec<(u32, Vec<u8>)> {
    msgs.iter().map(|m| (m.time, m.data().to_vec())).collect()
}

/// One cycle: hand the port this cycle's events and collect what it passes on.
fn cycle(p: &mut MidiPort, n_frames: u32, input: &[MidiStorageElem]) -> Vec<MidiStorageElem> {
    let mut out = MidiSortingBuffer::default();
    p.process(n_frames, Some(input), Some(&mut out));
    out.sort();
    out.events().expect("sorted").to_vec()
}

#[test]
fn midi_port_receive() {
    let mut p = port();

    // An empty cycle passes nothing on.
    check!(cycle(&mut p, 100, &[]).is_empty());

    let input = [ev(0, &[0, 1, 2]), ev(0, &[0, 1, 2])];
    let got = cycle(&mut p, 100, &input);

    check!(got.len() == 2);
    check!(pairs(&got) == pairs(&input));
}

#[test]
fn midi_port_mute_stops_output() {
    let mut p = port();
    p.set_muted(true);

    let input = [ev(0, &[0, 1, 2]), ev(0, &[0, 1, 2])];
    check!(cycle(&mut p, 100, &input).is_empty());

    // Muting only gates the output; the messages still arrived, so they are still
    // counted and still tracked.
    check!(p.n_input_events() == 2);
    check!(p.n_output_events() == 0);
}

#[test]
fn midi_port_message_counters() {
    let mut p = port();

    let input = [ev(0, &[0, 1, 2]), ev(0, &[0, 1, 2])];
    check!(cycle(&mut p, 100, &input).len() == 2);
    check!(p.n_input_events() == 2);
    check!(p.n_output_events() == 2);

    p.reset_n_input_events();
    p.reset_n_output_events();

    // A cycle with nothing in it counts nothing.
    check!(cycle(&mut p, 100, &[]).is_empty());
    check!(p.n_input_events() == 0);
    check!(p.n_output_events() == 0);
}

#[test]
fn midi_port_note_tracker() {
    let mut p = port();
    check!(p.n_notes_active() == 0);

    cycle(&mut p, 1, &[ev(0, &midi::note_on(0, 100, 127))]);
    check!(p.n_notes_active() == 1);

    cycle(&mut p, 1, &[ev(0, &midi::note_on(0, 110, 127))]);
    check!(p.n_notes_active() == 2);

    // A note-on for a note already sounding is not counted twice.
    cycle(&mut p, 1, &[ev(0, &midi::note_on(0, 100, 127))]);
    check!(p.n_notes_active() == 2);

    cycle(&mut p, 1, &[ev(0, &midi::note_off(0, 100, 127))]);
    check!(p.n_notes_active() == 1);

    // Nor does a second note-off take the count below what is sounding.
    cycle(&mut p, 1, &[ev(0, &midi::note_off(0, 100, 127))]);
    check!(p.n_notes_active() == 1);

    cycle(&mut p, 1, &[ev(0, &midi::note_off(0, 110, 127))]);
    check!(p.n_notes_active() == 0);
}

#[test]
fn midi_port_receives_a_run_of_messages_in_order() {
    let mut p = port();
    let input = [
        ev(0, &midi::note_on(0, 100, 127)),
        ev(1, &midi::note_on(0, 110, 127)),
        ev(2, &midi::note_off(0, 100, 127)),
        ev(3, &midi::note_off(0, 110, 127)),
    ];

    let got = cycle(&mut p, 100, &input);
    check!(got.len() == 4);
    check!(pairs(&got) == pairs(&input));
}

/// within a cycle, so what they write has to come out ordered by time.
#[test]
fn midi_port_output_is_sorted_by_time() {
    let mut out = MidiSortingBuffer::default();
    out.write(1, &[0, 1, 2]);
    out.write(0, &[0, 1, 2]);
    out.write(10, &[0, 1, 2]);
    out.sort();

    let events = out.events().expect("sorted");
    check!(events.iter().map(|m| m.time).collect::<Vec<_>>() == vec![0, 1, 10]);
}
