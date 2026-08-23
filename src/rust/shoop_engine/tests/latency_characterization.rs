use shoop_engine::audio_midi_loop::AudioMidiLoop;
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi_storage::MidiStorageElem;
use shoop_engine::port::PortDirection;
use shoop_engine::session::{Port, Session};

mod latency_support;
use latency_support::{
    identified_audio_sample, pump_callbacks, DeterministicActionHarness,
    DeterministicDelayedProcessor, DeterministicTimingConfig, IdentifiedAudioEvent,
    IdentifiedMidiEvent,
};

fn process_audio_callback(
    loop_: &mut AudioMidiLoop,
    dry_channel: usize,
    wet_channel: Option<usize>,
    processor: &mut DeterministicDelayedProcessor,
    callback_start: u64,
    callback_frames: u32,
) {
    let mut processed = 0_u32;
    while processed < callback_frames {
        let available = callback_frames - processed;
        loop_
            .audio_channel_mut(dry_channel)
            .expect("dry channel")
            .set_playback_buffer_size(available as usize);
        if let Some(wet_channel) = wet_channel {
            loop_
                .audio_channel_mut(wet_channel)
                .expect("wet channel")
                .set_recording_buffer_size(available as usize);
        }
        loop_.resync_poi();
        let frames = loop_.next_poi().map_or(available, |poi| poi.min(available));
        if frames == 0 {
            loop_.handle_poi();
            continue;
        }

        loop_
            .process::<Vec<MidiStorageElem>>(frames, &[], &mut [])
            .expect("process loop");
        let hit_poi = loop_.next_poi() == Some(0);
        if hit_poi {
            loop_.handle_poi();
        }

        let mut dry = vec![0.0; frames as usize];
        loop_
            .audio_channel_mut(dry_channel)
            .expect("dry channel")
            .finalize_process(&[], &mut dry);
        let (wet, midi) = processor.process(callback_start + u64::from(processed), &dry, &[]);
        assert!(midi.is_empty());
        if let Some(wet_channel) = wet_channel {
            loop_
                .audio_channel_mut(wet_channel)
                .expect("wet channel")
                .finalize_process(&wet, &mut []);
        }
        processed += frames;
    }
}

#[shoop_wasm_test_support::shoop_test]
fn deterministic_fixture_tracks_all_frame_domain_components() {
    let config = DeterministicTimingConfig {
        loop_length: 23,
        callback_size: 7,
        input_delay: 3,
        processor_delay: 5,
        cue_output_delay: 11,
        backend_hop_delay: 2,
        manual_trim: -1,
        performance_reference_offset: 11,
    };
    let event_frame = 6_u64;
    assert_eq!(config.direct_raw_frame(event_frame), 20);
    assert_eq!(config.wet_raw_frame(event_frame), 27);
    assert_eq!(config.direct_capture_advance(true), Some(13));
    assert_eq!(config.direct_capture_advance(false), Some(2));
    assert_eq!(config.wet_capture_advance(true), Some(20));
    assert_eq!(config.render_advance(), Some(6));

    let audio_id = 0x3f80_0001;
    let midi_data = [0x90, 73, 101];
    let mut harness = DeterministicActionHarness::new(
        config,
        &[IdentifiedAudioEvent {
            logical_frame: event_frame,
            id: audio_id,
        }],
        &[IdentifiedMidiEvent {
            frame: event_frame,
            data: midi_data.to_vec(),
        }],
    );
    harness.pump(u64::from(config.loop_length) * 2);

    let observations = harness.observations();
    assert_eq!(observations.audio_logical, vec![(audio_id, 6)]);
    assert_eq!(observations.audio_raw, vec![(audio_id, 20)]);
    assert_eq!(observations.audio_dispatch, vec![(audio_id, 20)]);
    assert_eq!(observations.audio_output, vec![(audio_id, 27)]);
    assert_eq!(observations.midi_logical, vec![(midi_data.to_vec(), 6)]);
    assert_eq!(observations.midi_raw, vec![(midi_data.to_vec(), 20)]);
    assert_eq!(observations.midi_dispatch, vec![(midi_data.to_vec(), 20)]);
    assert_eq!(observations.midi_output, vec![(midi_data.to_vec(), 27)]);
}

#[shoop_wasm_test_support::shoop_test]
fn current_monitoring_is_sample_identical_across_callback_sizes() {
    let mut session = Session::default();
    let input = session.add_port(Port::Dummy(DummyAudioPort::new(
        PortId(1),
        "monitor-input",
        PortDirection::Input,
        5,
    )));
    let output = session.add_port(Port::Dummy(DummyAudioPort::new(
        PortId(2),
        "monitor-output",
        PortDirection::Output,
        5,
    )));
    session.connect_ports_internal(input, output).unwrap();
    session.apply_graph_changes().unwrap();

    let source: Vec<f32> = (1..=12).map(|frame| frame as f32).collect();
    let mut offset = 0;
    for frames in [3, 5, 4] {
        let block = &source[offset..offset + frames];
        session
            .port_mut(input)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(block);
        session
            .port_mut(output)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(frames);
        session.process(frames);
        assert_eq!(
            session
                .port_mut(output)
                .unwrap()
                .as_dummy_mut()
                .unwrap()
                .dequeue_data(frames)
                .unwrap(),
            block
        );
        offset += frames;
    }
}

#[shoop_wasm_test_support::shoop_test]
fn current_grab_adopts_raw_history_across_callback_boundaries() {
    let mut session = Session::default();
    let input = session.add_port(Port::Dummy(DummyAudioPort::new(
        PortId(3),
        "grab-input",
        PortDirection::Input,
        4,
    )));
    let loop_ = session.create_loop();
    let channel = session
        .add_audio_channel_with_bounded_capacity(loop_, 4, 32, ChannelMode::Direct)
        .unwrap();
    session.connect_channel_input(channel, input).unwrap();
    session.apply_graph_changes().unwrap();

    let source: Vec<f32> = (1..=12).map(|frame| frame as f32).collect();
    let mut offset = 0;
    for frames in [3, 5, 4] {
        session
            .port_mut(input)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&source[offset..offset + frames]);
        session.process(frames);
        offset += frames;
    }
    session
        .adopt_audio_ringbuffers_for_loop(loop_, None, None, None, LoopMode::Playing)
        .unwrap();

    let grabbed = session.loop_(loop_).unwrap();
    assert_eq!(grabbed.mode(), LoopMode::Playing);
    assert_eq!(grabbed.length(), source.len() as u32);
    assert_eq!(grabbed.audio_channel(0).unwrap().data(), source);
}

#[shoop_wasm_test_support::shoop_test]
fn current_dry_through_wet_dispatches_without_render_ahead() {
    const LOOP_LENGTH: u32 = 13;
    const CALLBACK_SIZE: u32 = 5;
    const EVENT_FRAME: usize = 4;
    const PROCESSOR_DELAY: u32 = 3;
    let audio_id = 0x3f80_0002;

    let mut loop_ = AudioMidiLoop::default();
    let dry = loop_.add_audio_channel(4, ChannelMode::Dry);
    let mut data = vec![0.0; LOOP_LENGTH as usize];
    data[EVENT_FRAME] = identified_audio_sample(audio_id);
    loop_
        .audio_channel_mut(dry)
        .expect("dry channel")
        .load_data(&data);
    loop_.set_length(LOOP_LENGTH);
    loop_.set_mode(LoopMode::PlayingDryThroughWet);

    let mut processor = DeterministicDelayedProcessor::new(PROCESSOR_DELAY, 0);
    pump_callbacks(u64::from(LOOP_LENGTH), CALLBACK_SIZE, |start, frames| {
        process_audio_callback(&mut loop_, dry, None, &mut processor, start, frames);
    });

    let observations = processor.observations();
    assert_eq!(
        observations.audio_dispatch,
        vec![(audio_id, EVENT_FRAME as u64)]
    );
    assert_eq!(
        observations.audio_output,
        vec![(audio_id, EVENT_FRAME as u64 + u64::from(PROCESSOR_DELAY))]
    );
}

#[shoop_wasm_test_support::shoop_test]
fn current_dry_into_wet_records_the_uncompensated_delayed_return() {
    const LOOP_LENGTH: u32 = 13;
    const CALLBACK_SIZE: u32 = 5;
    const EVENT_FRAME: usize = 4;
    const PROCESSOR_DELAY: u32 = 3;
    let audio_id = 0x3f80_0003;
    let sample = identified_audio_sample(audio_id);

    let mut loop_ = AudioMidiLoop::default();
    let dry = loop_.add_audio_channel(4, ChannelMode::Dry);
    let wet = loop_.add_audio_channel(4, ChannelMode::Wet);
    let mut dry_data = vec![0.0; LOOP_LENGTH as usize];
    dry_data[EVENT_FRAME] = sample;
    loop_
        .audio_channel_mut(dry)
        .expect("dry channel")
        .load_data(&dry_data);
    loop_
        .audio_channel_mut(wet)
        .expect("wet channel")
        .load_data(&vec![0.0; LOOP_LENGTH as usize]);
    loop_.set_length(LOOP_LENGTH);
    loop_.set_mode(LoopMode::RecordingDryIntoWet);

    let mut processor = DeterministicDelayedProcessor::new(PROCESSOR_DELAY, 0);
    pump_callbacks(u64::from(LOOP_LENGTH), CALLBACK_SIZE, |start, frames| {
        process_audio_callback(&mut loop_, dry, Some(wet), &mut processor, start, frames);
    });

    let wet_data = loop_.audio_channel(wet).expect("wet channel").data();
    let recorded_frame = EVENT_FRAME + PROCESSOR_DELAY as usize;
    assert_eq!(wet_data[recorded_frame], sample);
    assert!(wet_data
        .iter()
        .enumerate()
        .all(|(frame, value)| frame == recorded_frame || *value == 0.0));
    assert_eq!(
        processor.observations().audio_dispatch,
        vec![(audio_id, EVENT_FRAME as u64)]
    );
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
