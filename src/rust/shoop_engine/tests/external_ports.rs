//! A driver's-eye view of the session: stage a cycle's input, run, take the output.
//!
//! This is the shape a real driver uses, and it is deliberately testable without one.
//! `ExternalAudioPort` and `ExternalMidiPort` take one buffer per cycle, staged before
//! the cycle runs, unlike the dummy ports whose queues span cycles.

use assert2::{check, let_assert};
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::engine::split;
use shoop_engine::external_audio_port::ExternalAudioPort;
use shoop_engine::external_midi_port::ExternalMidiPort;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi;
use shoop_engine::port::PortDirection;
use shoop_engine::session::{Port, Session};

fn audio_port(name: &str, direction: PortDirection) -> Port {
    Port::External(ExternalAudioPort::new(name, direction, 4))
}

fn midi_port(name: &str, direction: PortDirection) -> Port {
    Port::ExternalMidi(ExternalMidiPort::new(name, direction))
}

#[test]
fn a_driver_can_record_and_play_audio_through_the_session() {
    let mut s = Session::default();
    let input = s.add_port(audio_port("in", PortDirection::Input));
    let output = s.add_port(audio_port("out", PortDirection::Output));
    let l = s.create_loop();
    let_assert!(Ok(c) = s.add_audio_channel(l, 64, ChannelMode::Direct));
    let_assert!(Ok(()) = s.connect_channel_input(c, input));
    let_assert!(Ok(()) = s.connect_channel_output(c, output));
    let_assert!(Ok(()) = s.apply_graph_changes());

    // Record four frames the way a driver would: stage its input buffer, then run.
    let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
    let incoming = [0.25f32, 0.5, 0.75, 1.0];
    s.port_mut(input)
        .expect("port")
        .as_external_mut()
        .expect("external port")
        .stage_input(&incoming);
    s.process(4);

    check!(s.loop_(l).expect("loop").length() == 4);

    // Play it back and read the output the way a driver would.
    let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
    s.process(4);

    let out = s
        .port(output)
        .expect("port")
        .as_external()
        .expect("external port")
        .output(4);
    check!(out == incoming);
}

#[test]
fn an_unfed_cycle_is_silent_rather_than_a_repeat() {
    let mut s = Session::default();
    let input = s.add_port(audio_port("in", PortDirection::Input));
    let l = s.create_loop();
    let_assert!(Ok(c) = s.add_audio_channel(l, 64, ChannelMode::Direct));
    let_assert!(Ok(()) = s.connect_channel_input(c, input));
    let_assert!(Ok(()) = s.apply_graph_changes());
    let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));

    s.port_mut(input)
        .expect("port")
        .as_external_mut()
        .expect("external port")
        .stage_input(&[1.0, 1.0, 1.0, 1.0]);
    s.process(4);
    // Nothing staged this time, so what gets recorded is silence.
    s.process(4);

    let ch = s.loop_(l).expect("loop").audio_channel(0).expect("channel");
    check!(ch.data()[..4] == [1.0; 4]);
    check!(ch.data()[4..8] == [0.0; 4]);
}

#[test]
fn a_driver_can_record_and_play_midi_through_the_session() {
    let mut s = Session::default();
    let input = s.add_port(midi_port("min", PortDirection::Input));
    let output = s.add_port(midi_port("mout", PortDirection::Output));
    let l = s.create_loop();
    let_assert!(Ok(c) = s.add_midi_channel(l, 256, ChannelMode::Direct));
    let_assert!(Ok(()) = s.connect_channel_input(c, input));
    let_assert!(Ok(()) = s.connect_channel_output(c, output));
    let_assert!(Ok(()) = s.apply_graph_changes());

    let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Recording));
    {
        let p = s
            .port_mut(input)
            .expect("port")
            .as_external_midi_mut()
            .expect("midi port");
        check!(p.push_incoming(1, &midi::note_on(0, 60, 100)));
        check!(p.push_incoming(2, &midi::note_off(0, 60, 64)));
    }
    s.process(4);

    check!(
        s.loop_(l)
            .expect("loop")
            .midi_channel(0)
            .expect("channel")
            .n_events()
            == 2
    );

    // Play it back; the driver reads what the output port is holding for it.
    s.loop_mut(l).expect("loop").set_length(4);
    let_assert!(Ok(()) = s.set_loop_mode(l, LoopMode::Playing));
    s.process(4);

    let out = s
        .port(output)
        .expect("port")
        .as_external_midi()
        .expect("midi port")
        .outgoing();
    check!(out.len() == 2);
    check!(out[0].time == 1);
    check!(out[0].data() == midi::note_on(0, 60, 100).as_slice());
    check!(out[1].time == 2);
    check!(out[1].data() == midi::note_off(0, 60, 64).as_slice());
}

/// The whole boundary at once: a control command and a staged buffer meeting in one
/// cycle, which is what a driver callback does every time it runs.
#[test]
fn the_engine_drives_driver_shaped_ports() {
    let mut s = Session::default();
    let input = s.add_port(midi_port("min", PortDirection::Input));
    let output = s.add_port(midi_port("mout", PortDirection::Output));
    let l = s.create_loop();
    let_assert!(Ok(c) = s.add_midi_channel(l, 256, ChannelMode::Direct));
    let_assert!(Ok(()) = s.connect_channel_input(c, input));
    let_assert!(Ok(()) = s.connect_channel_output(c, output));
    let_assert!(Ok(()) = s.apply_graph_changes());

    let (mut engine, mut handle) = split(s, 16);

    let_assert!(
        Ok(_) = handle.send(Box::new(move |s: &mut Session| {
            let _ = s.set_loop_mode(0, LoopMode::Recording);
        }))
    );

    engine
        .session_mut()
        .port_mut(input)
        .expect("port")
        .as_external_midi_mut()
        .expect("midi port")
        .push_incoming(1, &midi::note_on(0, 60, 100));
    engine.process(4);

    check!(engine.session().loop_(l).expect("loop").mode() == LoopMode::Recording);
    check!(
        engine
            .session()
            .loop_(l)
            .expect("loop")
            .midi_channel(0)
            .expect("channel")
            .n_events()
            == 1
    );
    check!(
        engine
            .stats()
            .cycles
            .load(std::sync::atomic::Ordering::Relaxed)
            == 1
    );
}
