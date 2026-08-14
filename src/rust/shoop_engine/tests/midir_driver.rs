//! Real MIDI, end to end: sent over a virtual port, captured, recorded by a loop.
//!
//! Virtual ports are a Unix feature of `midir`, so this only compiles there. Where the
//! platform has the API but no usable port, the test fails rather than quietly passing --
//! set `SHOOP_ALLOW_MISSING_BACKENDS=1` to downgrade that to a skip.

#![cfg(all(feature = "midir", unix))]

use assert2::{check, let_assert};
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::external_midi_port::ExternalMidiPort;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi;
use shoop_engine::midir_driver::{create_virtual_input, open_output};
use shoop_engine::port::PortDirection;
use shoop_engine::session::{Port, Session};

mod backend_availability;
use backend_availability::require_backend;

/// A distinct virtual port name per test, so two running at once cannot capture each
/// other's messages.
fn virtual_name(suffix: &str) -> String {
    format!("shoop-test-{suffix}")
}

#[tracy_nextest_capture::tracy_capture_test]
fn a_message_sent_over_a_virtual_port_is_captured() {
    let name = virtual_name("capture");
    let Ok((mut capture, _conn)) = create_virtual_input("shoop-test-in", &name) else {
        require_backend("virtual MIDI", "no virtual MIDI port available");
        return;
    };

    let Ok(mut playback) = open_output("shoop-test-out", "to-virtual", &name) else {
        require_backend("virtual MIDI", "virtual port not visible to a sender");
        return;
    };

    // Sent through a real MIDI connection, not injected.
    let mut out_port = ExternalMidiPort::new("out", PortDirection::Output);
    out_port.prepare(64);
    out_port.write_event(
        shoop_engine::midi_storage::MidiStorageElem::new(0, &midi::note_on(0, 60, 100))
            .expect("valid"),
    );
    out_port.process(64);
    check!(playback.send_from(&out_port) == 1);

    // Wait for it to come back round through CoreMIDI.
    let mut in_port = ExternalMidiPort::new("in", PortDirection::Input);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut staged = 0;
    while std::time::Instant::now() < deadline && staged == 0 {
        staged = capture.drain_into(&mut in_port);
        if staged == 0 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    check!(staged == 1, "the message never arrived");
    in_port.prepare(64);
    in_port.process(64);
    let got = in_port.visible_events();
    check!(got.len() == 1);
    check!(got[0].data() == midi::note_on(0, 60, 100).as_slice());
    // Everything is staged at frame 0, which is the one-buffer imprecision of this path.
    check!(got[0].time == 0);
    check!(capture.n_refused() == 0);
    check!(capture.n_dropped() == 0);
}

#[tracy_nextest_capture::tracy_capture_test]
fn captured_midi_is_recorded_by_a_loop() {
    let name = virtual_name("record");
    let Ok((mut capture, _conn)) = create_virtual_input("shoop-rec-in", &name) else {
        require_backend("virtual MIDI", "no virtual MIDI port available");
        return;
    };
    let Ok(mut playback) = open_output("shoop-rec-out", "to-virtual", &name) else {
        require_backend("virtual MIDI", "virtual port not visible to a sender");
        return;
    };

    let mut s = Session::default();
    let input = s.add_port(Port::ExternalMidi(ExternalMidiPort::new(
        "min",
        PortDirection::Input,
    )));
    let l = s.create_loop();
    let_assert!(Ok(c) = s.add_midi_channel(l, 256, ChannelMode::Direct));
    let_assert!(Ok(()) = s.connect_channel_input(c, input));
    let_assert!(Ok(()) = s.apply_graph_changes());
    let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));

    // Send two messages over the real connection.
    let mut sender = ExternalMidiPort::new("sender", PortDirection::Output);
    sender.prepare(64);
    for m in [midi::note_on(0, 62, 90), midi::note_off(0, 62, 64)] {
        sender.write_event(shoop_engine::midi_storage::MidiStorageElem::new(0, &m).expect("valid"));
    }
    sender.process(64);
    check!(playback.send_from(&sender) == 2);

    // Drain into the session's port and run cycles until the loop has both.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if let Some(p) = s.port_mut(input).and_then(Port::as_external_midi_mut) {
            capture.drain_into(p);
        }
        s.process(64);
        let n = s
            .loop_(l)
            .and_then(|l| l.midi_channel(0))
            .map(|c| c.n_events())
            .unwrap_or(0);
        if n >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let_assert!(Some(ch) = s.loop_(l).and_then(|l| l.midi_channel(0)));
    check!(ch.n_events() == 2, "the loop did not record both messages");
    let contents = ch.contents();
    check!(contents[0].data() == midi::note_on(0, 62, 90).as_slice());
    check!(contents[1].data() == midi::note_off(0, 62, 64).as_slice());
}

#[tracy_nextest_capture::tracy_capture_test]
fn an_oversized_message_is_refused_rather_than_truncated() {
    let name = virtual_name("sysex");
    let Ok((mut capture, _conn)) = create_virtual_input("shoop-sx-in", &name) else {
        require_backend("virtual MIDI", "no virtual MIDI port available");
        return;
    };
    let Ok(out) = midir::MidiOutput::new("shoop-sx-out") else {
        require_backend("MIDI output", "no MIDI output available");
        return;
    };
    let Some(port) = out
        .ports()
        .into_iter()
        .find(|p| out.port_name(p).map(|n| n.contains(&name)).unwrap_or(false))
    else {
        require_backend("virtual MIDI", "virtual port not visible");
        return;
    };
    let Ok(mut conn) = out.connect(&port, "to-virtual") else {
        require_backend("virtual MIDI", "could not connect to the virtual port");
        return;
    };

    // A sysex message, which is longer than a storage element can hold.
    let _ = conn.send(&[0xF0, 0x7D, 0x01, 0x02, 0x03, 0x04, 0xF7]);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline && capture.n_refused() == 0 {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let mut port_in = ExternalMidiPort::new("in", PortDirection::Input);
    check!(capture.drain_into(&mut port_in) == 0);
    check!(
        capture.n_refused() >= 1,
        "the oversized message was not refused"
    );
}
