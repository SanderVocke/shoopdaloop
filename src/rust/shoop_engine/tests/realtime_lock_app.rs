#![cfg(feature = "app_backend")]

use shoop_engine::app_backend::{
    AudioDriver, AudioDriverSettings, AudioPort, BackendSession, DummyAudioDriverSettings, MidiPort,
};
use shoop_engine::realtime_lock_guard;
use shoop_engine::{AudioDriverType, ChannelMode, LoopMode, MidiEvent, PortDirection};

struct DisableGuard;

impl Drop for DisableGuard {
    fn drop(&mut self) {
        realtime_lock_guard::set_enabled(false);
    }
}

#[test]
fn dummy_app_processing_uses_only_explicit_realtime_lock_permissions() {
    const BUFFER: u32 = 64;

    let driver = AudioDriver::new(AudioDriverType::Dummy, None).expect("driver");
    driver
        .start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
            client_name: "realtime-lock-app-test".to_string(),
            sample_rate: 48_000,
            buffer_size: BUFFER,
        }))
        .expect("start driver");
    let session = BackendSession::new().expect("session");
    session.set_audio_driver(&driver).expect("attach driver");
    driver.dummy_enter_controlled_mode();

    realtime_lock_guard::set_enabled(true);
    let _disable = DisableGuard;

    let loop_ = session.create_loop().expect("loop");
    let channel = loop_
        .add_audio_channel(ChannelMode::Direct)
        .expect("channel");
    let midi_channel = loop_
        .add_midi_channel(ChannelMode::Direct)
        .expect("MIDI channel");
    let output = AudioPort::new_driver_port(
        &session,
        &driver,
        "guarded-output",
        &PortDirection::Output,
        BUFFER,
    )
    .expect("output port");
    let midi_output = MidiPort::new_driver_port(
        &session,
        &driver,
        "guarded-MIDI-output",
        &PortDirection::Output,
        BUFFER,
    )
    .expect("MIDI output port");
    channel.connect_output(&output).expect("connect output");
    midi_channel
        .connect_output(&midi_output)
        .expect("connect MIDI output");
    midi_channel
        .load_all_midi_data(&[MidiEvent::new(0, vec![0x90, 60, 100])])
        .expect("load MIDI");
    loop_.set_length(BUFFER * 2).expect("length");
    loop_
        .transition(LoopMode::Playing, -1, -1)
        .expect("playing");
    output.dummy_request_data(BUFFER).expect("request capture");
    midi_output
        .dummy_request_data(BUFFER)
        .expect("request MIDI capture");

    driver.wait_process();
    driver.dummy_request_controlled_frames(BUFFER);
    driver.dummy_run_requested_frames();

    realtime_lock_guard::set_enabled(false);
    assert_eq!(loop_.get_state().expect("state").position, BUFFER);
}
