use shoop_engine::audio_channel::AudioChannel;
use shoop_engine::audio_midi_loop::AudioMidiLoop;
use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::dummy_midi_port::DummyMidiPort;
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::latency_runtime::{
    RetainedLatencySelection, RuntimeLatencyObservation, RuntimeLatencyRecipe,
};
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi;
use shoop_engine::midi_channel::MidiChannel;
use shoop_engine::midi_storage::MidiStorageElem;
use shoop_engine::port::PortDirection;
use shoop_engine::session::{
    AudioRingbufferAdoption, GrabLatencyPolicy, LatencyAwareAudioRingbufferAdoption,
    LatencyAwareMidiRingbufferAdoption, Port, PreparedMidiLatencyGrabChannel, Session,
    SessionError,
};

mod latency_support;
use latency_support::{
    identified_audio_sample, pump_callbacks, DeterministicActionHarness,
    DeterministicDelayedProcessor, DeterministicTimingConfig, IdentifiedAudioEvent,
    IdentifiedMidiEvent,
};

fn runtime_render_recipe(
    operation: shoop_latency::LatencyOperationKind,
    observed_frames: u32,
    selected_frames: u32,
    revision: u64,
) -> RuntimeLatencyRecipe {
    let observation = shoop_latency::LatencyObservation::new(
        Some(shoop_latency::LatencyRangeFrames::new(observed_frames, observed_frames).unwrap()),
        shoop_latency::LatencyCertainty::Exact,
        48_000,
        revision,
        shoop_latency::SourceIdentity::new(format!("matrix-processor-{revision}")).unwrap(),
        Some(
            shoop_latency::LatencyIntervalIdentity::new(format!(
                "matrix-processor-interval-{revision}"
            ))
            .unwrap(),
        ),
    )
    .unwrap();
    let resolved = shoop_latency::resolve_latency_recipe(
        operation,
        shoop_latency::RecordingReference::ExternalWorld,
        &[shoop_latency::LatencyComponentInput {
            kind: shoop_latency::LatencyComponentKind::Processor,
            observation,
            policy: shoop_latency::LatencyComponentPolicy {
                enabled: true,
                value_mode: shoop_latency::LatencyValueMode::Manual(selected_frames),
                range_selection: shoop_latency::LatencyRangeSelection::Maximum,
            },
        }],
    )
    .unwrap();
    RuntimeLatencyRecipe::from_resolved(&resolved, revision)
}

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

fn process_midi_callback(
    loop_: &mut AudioMidiLoop,
    dry_channel: usize,
    processor: &mut DeterministicDelayedProcessor,
    callback_start: u64,
    callback_frames: u32,
) {
    let mut processed = 0_u32;
    while processed < callback_frames {
        let available = callback_frames - processed;
        loop_
            .midi_channel_mut(dry_channel)
            .expect("dry MIDI channel")
            .set_playback_buffer(available);
        loop_.resync_poi();
        let frames = loop_.next_poi().map_or(available, |poi| poi.min(available));
        if frames == 0 {
            loop_.handle_poi();
            continue;
        }
        let mut outputs = [Vec::with_capacity(32)];
        loop_
            .process(frames, &[&[][..]], &mut outputs)
            .expect("process MIDI loop");
        let hit_poi = loop_.next_poi() == Some(0);
        if hit_poi {
            loop_.handle_poi();
        }
        let silence = vec![0.0; frames as usize];
        processor.process(callback_start + u64::from(processed), &silence, &outputs[0]);
        processed += frames;
    }
}

fn dry_through_wet_audio_oracle(
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_processor_delay: u32,
    selected_render_advance: u32,
    take_capture_alignment: u32,
    cycles: u32,
) -> Vec<u64> {
    let audio_id = 0x3f80_0062;
    let sample = identified_audio_sample(audio_id);
    let mut loop_ = AudioMidiLoop::default();
    let dry = loop_.add_audio_channel(callback_size.max(1) as usize, ChannelMode::Dry);
    let mut data = vec![0.0; (loop_length + take_capture_alignment) as usize];
    data[(event_frame + take_capture_alignment) as usize] = sample;
    loop_.audio_channel_mut(dry).unwrap().load_data(&data);
    loop_
        .audio_channel_mut(dry)
        .unwrap()
        .set_capture_alignment_frames(take_capture_alignment as i32)
        .unwrap();
    loop_.set_length(loop_length);
    loop_.set_pending_latency_recipe(Some(runtime_render_recipe(
        shoop_latency::LatencyOperationKind::DryThroughWet,
        actual_processor_delay,
        selected_render_advance,
        1,
    )));
    loop_.latch_latency_recipes(0);
    loop_.set_pending_latency_recipe(None);
    loop_.set_mode(LoopMode::PlayingDryThroughWet);
    assert_eq!(
        loop_.audio_channel(dry).unwrap().render_advance_frames(),
        selected_render_advance
    );
    let mut processor = DeterministicDelayedProcessor::new(actual_processor_delay, 0);
    pump_callbacks(
        u64::from(loop_length) * u64::from(cycles),
        callback_size,
        |start, frames| {
            assert_eq!(
                loop_.audio_channel(dry).unwrap().render_advance_frames(),
                selected_render_advance
            );
            process_audio_callback(&mut loop_, dry, None, &mut processor, start, frames);
        },
    );
    processor
        .observations()
        .audio_output
        .iter()
        .filter(|(id, frame)| *id == audio_id && *frame >= u64::from(selected_render_advance))
        .map(|(_, frame)| *frame)
        .collect()
}

fn dry_through_wet_midi_oracle(
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_processor_delay: u32,
    selected_render_advance: u32,
    cycles: u32,
) -> Vec<u64> {
    let note = 71;
    let mut loop_ = AudioMidiLoop::default();
    let dry = loop_.add_midi_channel(128, ChannelMode::Dry);
    loop_.midi_channel_mut(dry).unwrap().set_contents(
        &[
            MidiStorageElem::new(event_frame, &midi::note_on(0, note, 100)).unwrap(),
            MidiStorageElem::new(event_frame, &midi::note_off(0, note, 0)).unwrap(),
        ],
        loop_length,
        Some(&[midi::cc(0, 64, 127).to_vec()]),
    );
    loop_.set_length(loop_length);
    loop_.set_pending_latency_recipe(Some(runtime_render_recipe(
        shoop_latency::LatencyOperationKind::DryThroughWet,
        actual_processor_delay,
        selected_render_advance,
        2,
    )));
    loop_.latch_latency_recipes(0);
    loop_.set_pending_latency_recipe(None);
    loop_.set_mode(LoopMode::PlayingDryThroughWet);
    assert_eq!(
        loop_.midi_channel(dry).unwrap().render_advance_frames(),
        selected_render_advance
    );
    let mut processor = DeterministicDelayedProcessor::new(actual_processor_delay, 0);
    pump_callbacks(
        u64::from(loop_length) * u64::from(cycles),
        callback_size,
        |start, frames| {
            assert_eq!(
                loop_.midi_channel(dry).unwrap().render_advance_frames(),
                selected_render_advance
            );
            process_midi_callback(&mut loop_, dry, &mut processor, start, frames);
        },
    );
    processor
        .observations()
        .midi_output
        .iter()
        .filter(|(data, frame)| {
            midi::is_note_on(data)
                && data[1] == note
                && *frame >= u64::from(selected_render_advance)
        })
        .map(|(_, frame)| *frame)
        .collect()
}

fn process_midi_synth_callback(
    loop_: &mut AudioMidiLoop,
    dry_midi: usize,
    wet_audio: usize,
    processor: &mut DeterministicDelayedProcessor,
    callback_start: u64,
    callback_frames: u32,
    synth_sample: f32,
) {
    let mut processed = 0_u32;
    while processed < callback_frames {
        let available = callback_frames - processed;
        loop_
            .midi_channel_mut(dry_midi)
            .unwrap()
            .set_playback_buffer(available);
        loop_
            .audio_channel_mut(wet_audio)
            .unwrap()
            .set_recording_buffer_size(available as usize);
        loop_.resync_poi();
        let frames = loop_.next_poi().map_or(available, |poi| poi.min(available));
        if frames == 0 {
            loop_.handle_poi();
            continue;
        }
        let mut midi_outputs = [Vec::with_capacity(32)];
        loop_
            .process(frames, &[&[][..]], &mut midi_outputs)
            .unwrap();
        if loop_.next_poi() == Some(0) {
            loop_.handle_poi();
        }
        let silence = vec![0.0; frames as usize];
        let (_, processed_midi) = processor.process(
            callback_start + u64::from(processed),
            &silence,
            &midi_outputs[0],
        );
        let mut wet = vec![0.0; frames as usize];
        for event in processed_midi {
            if midi::is_note_on(event.data()) {
                wet[event.time as usize] = synth_sample;
            }
        }
        loop_
            .audio_channel_mut(wet_audio)
            .unwrap()
            .finalize_process(&wet, &mut []);
        processed += frames;
    }
}

fn dry_midi_into_wet_audio_oracle(
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_processor_delay: u32,
    selected_render_advance: u32,
) -> (Vec<usize>, Vec<(Vec<u8>, u64)>) {
    let sample = identified_audio_sample(0x3f80_0064);
    let note = 72;
    let mut loop_ = AudioMidiLoop::default();
    let wet = loop_.add_audio_channel(callback_size.max(1) as usize, ChannelMode::Wet);
    let dry = loop_.add_midi_channel(256, ChannelMode::Dry);
    loop_
        .audio_channel_mut(wet)
        .unwrap()
        .load_data(&vec![0.0; loop_length as usize]);
    loop_.midi_channel_mut(dry).unwrap().set_contents(
        &[
            MidiStorageElem::new(event_frame, &midi::cc(0, 64, 127)).unwrap(),
            MidiStorageElem::new(event_frame, &midi::note_on(0, note, 100)).unwrap(),
            MidiStorageElem::new(event_frame, &midi::note_off(0, note, 0)).unwrap(),
        ],
        loop_length,
        Some(&[midi::program_change(0, 4).to_vec()]),
    );
    loop_.set_length(loop_length);
    loop_.set_pending_latency_recipe(Some(runtime_render_recipe(
        shoop_latency::LatencyOperationKind::RecordDryIntoWet,
        actual_processor_delay,
        selected_render_advance,
        4,
    )));
    loop_.set_mode(LoopMode::RecordingDryIntoWet);
    let mut processor = DeterministicDelayedProcessor::new(actual_processor_delay, 0);
    pump_callbacks(
        u64::from(loop_length) * 3 + u64::from(selected_render_advance),
        callback_size,
        |start, frames| {
            process_midi_synth_callback(
                &mut loop_,
                dry,
                wet,
                &mut processor,
                start,
                frames,
                sample,
            );
        },
    );
    let frames = loop_
        .audio_channel(wet)
        .unwrap()
        .data()
        .iter()
        .enumerate()
        .filter_map(|(frame, value)| (*value == sample).then_some(frame))
        .collect();
    (frames, processor.observations().midi_output.clone())
}

fn dry_into_wet_audio_oracle(
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_processor_delay: u32,
    selected_render_advance: u32,
) -> (Vec<usize>, bool, i32) {
    let audio_id = 0x3f80_0063;
    let sample = identified_audio_sample(audio_id);
    let mut loop_ = AudioMidiLoop::default();
    let dry = loop_.add_audio_channel(callback_size.max(1) as usize, ChannelMode::Dry);
    let wet = loop_.add_audio_channel(callback_size.max(1) as usize, ChannelMode::Wet);
    let mut dry_data = vec![0.0; loop_length as usize];
    dry_data[event_frame as usize] = sample;
    loop_.audio_channel_mut(dry).unwrap().load_data(&dry_data);
    loop_
        .audio_channel_mut(wet)
        .unwrap()
        .load_data(&vec![0.0; loop_length as usize]);
    loop_.set_length(loop_length);
    loop_.set_pending_latency_recipe(Some(runtime_render_recipe(
        shoop_latency::LatencyOperationKind::RecordDryIntoWet,
        actual_processor_delay,
        selected_render_advance,
        3,
    )));
    loop_.set_mode(LoopMode::RecordingDryIntoWet);
    let mut processor = DeterministicDelayedProcessor::new(actual_processor_delay, 0);
    pump_callbacks(
        u64::from(loop_length) * 3 + u64::from(selected_render_advance),
        callback_size,
        |start, frames| {
            process_audio_callback(&mut loop_, dry, Some(wet), &mut processor, start, frames);
        },
    );
    let wet_channel = loop_.audio_channel(wet).unwrap();
    let frames = wet_channel
        .data()
        .iter()
        .enumerate()
        .filter_map(|(frame, value)| (*value == sample).then_some(frame))
        .collect();
    let applied = wet_channel.latched_latency_recipe().is_some_and(|latched| {
        latched
            .recipe
            .components()
            .all(|component| component.applied_during_render)
    });
    (frames, applied, wet_channel.capture_alignment_frames())
}

fn latency_grab_fixture(
    role: ChannelMode,
    policy: GrabLatencyPolicy,
    variable: bool,
) -> (Vec<f32>, RetainedLatencySelection) {
    let mut session = Session::default();
    session.set_buffer_size(4);
    let input = session.add_port(Port::Dummy(DummyAudioPort::new(
        PortId(30),
        "latency-grab-input",
        PortDirection::Input,
        4,
    )));
    let output = session.add_port(Port::Dummy(DummyAudioPort::new(
        PortId(31),
        "latency-grab-output",
        PortDirection::Output,
        4,
    )));
    session
        .port_mut(input)
        .unwrap()
        .audio_mut()
        .unwrap()
        .set_ringbuffer_n_samples(16);
    let loop_ = session.create_loop();
    let channel = session
        .add_audio_channel_with_bounded_capacity(loop_, 4, 16, role)
        .unwrap();
    session.connect_channel_input(channel, input).unwrap();
    session.connect_channel_output(channel, output).unwrap();
    session.apply_graph_changes().unwrap();

    let mut source = vec![0.0; 20];
    source[14] = 1.0;
    for start in [0, 4, 8, 12, 16] {
        let observation = if variable && start < 16 {
            RuntimeLatencyObservation::exact(2, 48_000, 1).unwrap()
        } else {
            RuntimeLatencyObservation::exact(3, 48_000, 2).unwrap()
        };
        session
            .port_mut(input)
            .unwrap()
            .audio()
            .unwrap()
            .publish_capture_latency(observation);
        session
            .port_mut(input)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .queue_data(&source[start..start + 4]);
        session.process(4);
    }
    session.loop_mut(loop_).unwrap().set_sync_source(Some(
        shoop_engine::basic_loop::SyncSourceState {
            mode: LoopMode::Playing,
            triggering_now: false,
            next_trigger_eta: Some(8),
            position: 0,
            length: 8,
        },
    ));
    session
        .adopt_audio_ringbuffers_with_latency(&[LatencyAwareAudioRingbufferAdoption {
            request: AudioRingbufferAdoption {
                loop_idx: loop_,
                reverse_start_cycle: None,
                cycles_length: Some(1),
                go_to_cycle: Some(0),
                go_to_mode: playback_mode(role),
            },
            latency_policy: policy,
        }])
        .unwrap();
    let selection = session
        .loop_(loop_)
        .unwrap()
        .audio_channel(0)
        .unwrap()
        .grab_latency_selection();
    session
        .port_mut(output)
        .unwrap()
        .as_dummy_mut()
        .unwrap()
        .request_data(8);
    session.process(8);
    let rendered = session
        .port_mut(output)
        .unwrap()
        .as_dummy_mut()
        .unwrap()
        .dequeue_data(8)
        .unwrap();
    (rendered, selection)
}

#[shoop_wasm_test_support::shoop_test]
fn stable_latency_grab_applies_policy_without_mutating_raw_identity() {
    for role in [ChannelMode::Direct, ChannelMode::Dry, ChannelMode::Wet] {
        let (automatic, selection) =
            latency_grab_fixture(role, GrabLatencyPolicy::Automatic, false);
        assert_eq!(automatic, vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(matches!(selection, RetainedLatencySelection::Stable(_)));

        let (disabled, _) = latency_grab_fixture(role, GrabLatencyPolicy::Disabled, false);
        assert_eq!(disabled, vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let (manual, _) = latency_grab_fixture(role, GrabLatencyPolicy::Manual(1), false);
        assert_eq!(manual, vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
    }
}

#[shoop_wasm_test_support::shoop_test]
fn variable_latency_grab_uses_newest_revision_and_keeps_warning() {
    let (rendered, selection) =
        latency_grab_fixture(ChannelMode::Direct, GrabLatencyPolicy::Automatic, true);
    assert_eq!(rendered, vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    assert!(matches!(
        selection,
        RetainedLatencySelection::Variable { revisions: 2, .. }
    ));
}

fn midi_latency_grab_fixture(
    role: ChannelMode,
    policy: GrabLatencyPolicy,
    variable: bool,
) -> (Vec<(u32, u8)>, RetainedLatencySelection) {
    let mut session = Session::default();
    session.set_buffer_size(4);
    let input = session.add_port(Port::DummyMidi(DummyMidiPort::new(
        PortId(33),
        "midi-grab-input",
        PortDirection::Input,
    )));
    let output = session.add_port(Port::DummyMidi(DummyMidiPort::new(
        PortId(34),
        "midi-grab-output",
        PortDirection::Output,
    )));
    session
        .port_mut(input)
        .unwrap()
        .midi_mut()
        .unwrap()
        .set_ringbuffer_n_samples(16);
    let loop_ = session.create_loop();
    let channel = session.add_midi_channel(loop_, 64, role).unwrap();
    session.connect_channel_input(channel, input).unwrap();
    session.connect_channel_output(channel, output).unwrap();
    session.apply_graph_changes().unwrap();
    for (time, note) in [(2, 65), (14, 68)] {
        session
            .port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(time, &midi::note_on(0, note, 100));
        session
            .port_mut(input)
            .unwrap()
            .as_dummy_midi_mut()
            .unwrap()
            .queue_msg(time, &midi::note_off(0, note, 0));
    }
    for start in [0, 4, 8, 12, 16] {
        let observation = if variable && start < 16 {
            RuntimeLatencyObservation::exact(2, 48_000, 1).unwrap()
        } else {
            RuntimeLatencyObservation::exact(3, 48_000, 2).unwrap()
        };
        session
            .port_mut(input)
            .unwrap()
            .midi()
            .unwrap()
            .publish_capture_latency(observation);
        session.process(4);
    }
    session.loop_mut(loop_).unwrap().set_sync_source(Some(
        shoop_engine::basic_loop::SyncSourceState {
            mode: LoopMode::Playing,
            triggering_now: false,
            next_trigger_eta: Some(8),
            position: 0,
            length: 8,
        },
    ));
    let mut prepared = [PreparedMidiLatencyGrabChannel::new(loop_, 0, 64)];
    session
        .adopt_midi_ringbuffers_with_latency(
            &[LatencyAwareMidiRingbufferAdoption {
                request: AudioRingbufferAdoption {
                    loop_idx: loop_,
                    reverse_start_cycle: None,
                    cycles_length: Some(1),
                    go_to_cycle: Some(0),
                    go_to_mode: playback_mode(role),
                },
                latency_policy: policy,
            }],
            &mut prepared,
        )
        .unwrap();
    let selection = session
        .loop_(loop_)
        .unwrap()
        .midi_channel(0)
        .unwrap()
        .grab_latency_selection();
    session
        .port_mut(output)
        .unwrap()
        .as_dummy_midi_mut()
        .unwrap()
        .request_data(8)
        .unwrap();
    session.process(8);
    let pair = session
        .port_mut(output)
        .unwrap()
        .as_dummy_midi_mut()
        .unwrap()
        .take_written_requested_msgs()
        .iter()
        .filter(|event| event.data().get(1) == Some(&68))
        .map(|event| (event.time, event.data()[0] & 0xf0))
        .collect::<Vec<_>>();
    (pair, selection)
}

#[shoop_wasm_test_support::shoop_test]
fn stable_midi_latency_grab_preserves_equal_frame_order_and_alignment() {
    for role in [ChannelMode::Direct, ChannelMode::Dry] {
        let (pair, selection) =
            midi_latency_grab_fixture(role, GrabLatencyPolicy::Automatic, false);
        assert!(matches!(selection, RetainedLatencySelection::Stable(_)));
        assert_eq!(pair, vec![(2, 0x90), (2, 0x80)]);
        let (disabled, _) = midi_latency_grab_fixture(role, GrabLatencyPolicy::Disabled, false);
        assert_eq!(disabled, vec![(5, 0x90), (5, 0x80)]);
        let (manual, _) = midi_latency_grab_fixture(role, GrabLatencyPolicy::Manual(1), false);
        assert_eq!(manual, vec![(4, 0x90), (4, 0x80)]);
        let (variable, selection) =
            midi_latency_grab_fixture(role, GrabLatencyPolicy::Automatic, true);
        assert_eq!(variable, vec![(2, 0x90), (2, 0x80)]);
        assert!(matches!(
            selection,
            RetainedLatencySelection::Variable { revisions: 2, .. }
        ));
    }
}

#[shoop_wasm_test_support::shoop_test]
fn insufficient_latency_grab_margin_fails_before_target_mutation() {
    let mut session = Session::default();
    let input = session.add_port(Port::Dummy(DummyAudioPort::new(
        PortId(32),
        "short-grab-input",
        PortDirection::Input,
        4,
    )));
    session
        .port_mut(input)
        .unwrap()
        .audio_mut()
        .unwrap()
        .set_ringbuffer_n_samples(12);
    let loop_ = session.create_loop();
    let channel = session
        .add_audio_channel_with_bounded_capacity(loop_, 4, 16, ChannelMode::Direct)
        .unwrap();
    session.connect_channel_input(channel, input).unwrap();
    session.apply_graph_changes().unwrap();
    session
        .port_mut(input)
        .unwrap()
        .audio()
        .unwrap()
        .publish_capture_latency(RuntimeLatencyObservation::exact(7, 48_000, 1).unwrap());
    session
        .port_mut(input)
        .unwrap()
        .as_dummy_mut()
        .unwrap()
        .queue_data(&[0.0; 12]);
    session.process(12);
    let target = session.loop_mut(loop_).unwrap();
    target.set_length(1);
    target.audio_channel_mut(0).unwrap().load_data(&[9.0]);
    target.set_sync_source(Some(shoop_engine::basic_loop::SyncSourceState {
        mode: LoopMode::Playing,
        triggering_now: false,
        next_trigger_eta: Some(8),
        position: 0,
        length: 8,
    }));

    let result =
        session.adopt_audio_ringbuffers_with_latency(&[LatencyAwareAudioRingbufferAdoption {
            request: AudioRingbufferAdoption {
                loop_idx: loop_,
                reverse_start_cycle: None,
                cycles_length: Some(1),
                go_to_cycle: None,
                go_to_mode: LoopMode::Playing,
            },
            latency_policy: GrabLatencyPolicy::Automatic,
        }]);
    assert_eq!(result, Err(SessionError::LatencyGrabHistoryUnavailable));
    assert_eq!(session.loop_(loop_).unwrap().length(), 1);
    assert_eq!(
        session
            .loop_(loop_)
            .unwrap()
            .audio_channel(0)
            .unwrap()
            .data(),
        vec![9.0]
    );
}

#[shoop_wasm_test_support::shoop_test]
fn planned_render_matrix_dispatches_exactly_before_public_transition() {
    const LOOP_LENGTH: u32 = 11;
    for advance in [0_u32, 1, 10, 11, 12, 25] {
        let delay_cycles = advance.saturating_sub(1) / LOOP_LENGTH;
        let target = (delay_cycles + 1) * LOOP_LENGTH;
        let audio_id = 0x3f80_0080 + advance;
        let mut loop_ = AudioMidiLoop::default();
        let dry = loop_.add_audio_channel(4, ChannelMode::Dry);
        let mut data = vec![0.0; LOOP_LENGTH as usize];
        data[0] = identified_audio_sample(audio_id);
        loop_.audio_channel_mut(dry).unwrap().load_data(&data);
        loop_.set_length(LOOP_LENGTH);
        loop_.set_pending_latency_recipe(Some(runtime_render_recipe(
            shoop_latency::LatencyOperationKind::DryThroughWet,
            advance,
            advance,
            100 + u64::from(advance),
        )));
        loop_.set_sync_source(Some(shoop_engine::basic_loop::SyncSourceState {
            mode: LoopMode::Playing,
            triggering_now: false,
            next_trigger_eta: Some(LOOP_LENGTH),
            position: 0,
            length: LOOP_LENGTH,
        }));
        loop_.plan_transition(LoopMode::PlayingDryThroughWet, Some(delay_cycles), None);
        let mut processor = DeterministicDelayedProcessor::new(advance, 0);
        let mut modes_at_frame = Vec::new();
        for global in 0..target + LOOP_LENGTH {
            let position = global % LOOP_LENGTH;
            loop_.set_sync_source(Some(shoop_engine::basic_loop::SyncSourceState {
                mode: LoopMode::Playing,
                triggering_now: false,
                next_trigger_eta: Some(LOOP_LENGTH - position),
                position,
                length: LOOP_LENGTH,
            }));
            modes_at_frame.push(loop_.mode());
            process_audio_callback(&mut loop_, dry, None, &mut processor, u64::from(global), 1);
            if (global + 1) % LOOP_LENGTH == 0 {
                loop_.set_sync_source(Some(shoop_engine::basic_loop::SyncSourceState {
                    mode: LoopMode::Playing,
                    triggering_now: true,
                    next_trigger_eta: Some(LOOP_LENGTH),
                    position: 0,
                    length: LOOP_LENGTH,
                }));
                loop_.handle_sync();
            }
        }
        let dispatch = processor
            .observations()
            .audio_dispatch
            .iter()
            .find(|(id, _)| *id == audio_id)
            .unwrap()
            .1;
        let output = processor
            .observations()
            .audio_output
            .iter()
            .find(|(id, frame)| *id == audio_id && *frame >= u64::from(target))
            .unwrap()
            .1;
        assert_eq!(dispatch, u64::from(target - advance));
        assert_eq!(output, u64::from(target));
        if advance > 0 {
            assert_eq!(
                modes_at_frame[(target - advance) as usize],
                LoopMode::Stopped
            );
        }
        assert_eq!(loop_.mode(), LoopMode::PlayingDryThroughWet);
    }
}

#[shoop_wasm_test_support::shoop_test]
fn dry_through_wet_component_matrix_matches_audio_midi_processor_oracles() {
    for callback_size in [1_u32, 7, 64, 127] {
        let loop_length = callback_size.saturating_add(11).max(12);
        let event_frames = [0, 1, loop_length / 2, loop_length - 1];
        for actual in [1, 3, callback_size + 1, loop_length + 1] {
            let mut selected_values = vec![0, actual, actual.saturating_add(2)];
            selected_values.push(actual.saturating_sub(1));
            selected_values.sort_unstable();
            selected_values.dedup();
            for event_frame in event_frames {
                for selected in selected_values.iter().copied() {
                    let cycles = 3;
                    let total = u64::from(loop_length) * u64::from(cycles);
                    let expected = (0..=cycles)
                        .map(|cycle| {
                            u64::from(event_frame)
                                + u64::from(actual)
                                + u64::from(cycle) * u64::from(loop_length)
                        })
                        .filter(|frame| *frame >= u64::from(selected) && *frame < total)
                        .collect::<Vec<_>>();
                    assert_eq!(
                        dry_through_wet_audio_oracle(
                            callback_size,
                            loop_length,
                            event_frame,
                            actual,
                            selected,
                            0,
                            cycles,
                        ),
                        expected,
                        "audio B={callback_size} L={loop_length} E={event_frame} actual={actual} selected={selected}",
                    );
                    assert_eq!(
                        dry_through_wet_midi_oracle(
                            callback_size,
                            loop_length,
                            event_frame,
                            actual,
                            selected,
                            cycles,
                        ),
                        expected,
                        "MIDI B={callback_size} L={loop_length} E={event_frame} actual={actual} selected={selected}",
                    );
                }
            }
        }
    }
}

#[shoop_wasm_test_support::shoop_test]
fn dry_into_wet_component_and_boundary_matrix_writes_one_canonical_event() {
    for callback_size in [1_u32, 7, 64, 127] {
        let loop_length = callback_size.saturating_add(9).max(10);
        for event_frame in [0, 1, callback_size.min(loop_length - 1), loop_length - 1] {
            for actual in [1, 3, callback_size + 1, loop_length + 1] {
                let mut selected_values = vec![0, actual, actual.saturating_add(1)];
                selected_values.push(actual.saturating_sub(1));
                selected_values.sort_unstable();
                selected_values.dedup();
                for selected in selected_values {
                    let expected = (i64::from(event_frame) + i64::from(actual)
                        - i64::from(selected))
                    .rem_euclid(i64::from(loop_length)) as usize;
                    let (frames, applied_during_render, remaining_alignment) =
                        dry_into_wet_audio_oracle(
                            callback_size,
                            loop_length,
                            event_frame,
                            actual,
                            selected,
                        );
                    assert_eq!(
                        frames,
                        vec![expected],
                        "B={callback_size} L={loop_length} E={event_frame} actual={actual} selected={selected}",
                    );
                    assert!(applied_during_render);
                    assert_eq!(remaining_alignment, 0);
                }
            }
        }
    }
}

#[shoop_wasm_test_support::shoop_test]
fn dry_midi_into_wet_audio_preserves_state_order_and_canonical_timing() {
    for callback_size in [1_u32, 7, 64] {
        let loop_length = callback_size.saturating_add(9).max(10);
        for event_frame in [0, 1, loop_length - 1] {
            for actual in [1, 3, callback_size + 1] {
                for selected in [0, actual, actual.saturating_add(1)] {
                    let expected = (i64::from(event_frame) + i64::from(actual)
                        - i64::from(selected))
                    .rem_euclid(i64::from(loop_length)) as usize;
                    let (frames, midi_output) = dry_midi_into_wet_audio_oracle(
                        callback_size,
                        loop_length,
                        event_frame,
                        actual,
                        selected,
                    );
                    assert_eq!(frames, vec![expected]);
                    let processor_frame = u64::from(event_frame + actual) % u64::from(loop_length);
                    let same_frame = midi_output
                        .iter()
                        .filter(|(_, frame)| *frame % u64::from(loop_length) == processor_frame)
                        .map(|(data, _)| data[0] & 0xf0)
                        .collect::<Vec<_>>();
                    assert!(same_frame
                        .windows(3)
                        .any(|window| window == [0xB0, 0x90, 0x80]));
                }
            }
        }
    }
}

#[shoop_wasm_test_support::shoop_test]
fn dry_through_wet_start_steady_wrap_stop_restart_and_parallel_loops_are_exact() {
    const LOOP_LENGTH: u32 = 13;
    const CALLBACK: u32 = 5;
    const DELAY: u32 = 3;
    let mut loops = [AudioMidiLoop::default(), AudioMidiLoop::default()];
    let mut processors = [
        DeterministicDelayedProcessor::new(DELAY, 0),
        DeterministicDelayedProcessor::new(DELAY, 0),
    ];
    let mut channels = [0_usize; 2];
    for (index, loop_) in loops.iter_mut().enumerate() {
        let channel = loop_.add_audio_channel(4, ChannelMode::Dry);
        channels[index] = channel;
        let mut data = vec![0.0; LOOP_LENGTH as usize];
        data[4 + index] = identified_audio_sample(0x3f80_0070 + index as u32);
        loop_.audio_channel_mut(channel).unwrap().load_data(&data);
        loop_.set_length(LOOP_LENGTH);
        loop_.set_pending_latency_recipe(Some(runtime_render_recipe(
            shoop_latency::LatencyOperationKind::DryThroughWet,
            DELAY,
            DELAY,
            10 + index as u64,
        )));
        loop_.latch_latency_recipes(0);
        loop_.set_pending_latency_recipe(None);
        loop_.set_mode(LoopMode::PlayingDryThroughWet);
    }
    let mut pump_cycle = |cycle: u32, loops: &mut [AudioMidiLoop; 2]| {
        pump_callbacks(u64::from(LOOP_LENGTH), CALLBACK, |start, frames| {
            for index in 0..2 {
                process_audio_callback(
                    &mut loops[index],
                    channels[index],
                    None,
                    &mut processors[index],
                    u64::from(cycle * LOOP_LENGTH) + start,
                    frames,
                );
            }
        });
    };
    pump_cycle(0, &mut loops);
    loops[0].set_mode(LoopMode::Stopped);
    pump_cycle(1, &mut loops);
    loops[0].set_pending_latency_recipe(Some(runtime_render_recipe(
        shoop_latency::LatencyOperationKind::DryThroughWet,
        DELAY,
        DELAY,
        20,
    )));
    loops[0].latch_latency_recipes(u64::from(2 * LOOP_LENGTH));
    loops[0].set_pending_latency_recipe(None);
    loops[0].set_mode(LoopMode::PlayingDryThroughWet);
    pump_cycle(2, &mut loops);

    assert_eq!(
        processors[0]
            .observations()
            .audio_output
            .iter()
            .map(|(_, frame)| *frame)
            .collect::<Vec<_>>(),
        vec![7, 2 * u64::from(LOOP_LENGTH) + 7]
    );
    assert_eq!(
        processors[1]
            .observations()
            .audio_output
            .iter()
            .map(|(_, frame)| *frame)
            .collect::<Vec<_>>(),
        vec![
            8,
            u64::from(LOOP_LENGTH) + 8,
            2 * u64::from(LOOP_LENGTH) + 8
        ]
    );
}

#[shoop_wasm_test_support::shoop_test]
fn frozen_take_has_identical_logical_times_before_current_render_advance() {
    for callback_size in [1_u32, 7, 64] {
        let loop_length = callback_size + 11;
        let event_frame = loop_length - 2;
        let capture_alignment = callback_size + 1;
        let processor_delay = callback_size + 3;
        let ordinary = render_audio_oracle(
            ChannelMode::Direct,
            callback_size,
            loop_length,
            event_frame,
            capture_alignment as i32,
            capture_alignment as i32,
        );
        let through_wet = dry_through_wet_audio_oracle(
            callback_size,
            loop_length,
            event_frame,
            processor_delay,
            processor_delay,
            capture_alignment,
            3,
        );
        let relative_to_transition = through_wet
            .into_iter()
            .map(|frame| frame - u64::from(processor_delay))
            .take(ordinary.len())
            .collect::<Vec<_>>();
        assert_eq!(
            relative_to_transition,
            ordinary.into_iter().map(u64::from).collect::<Vec<_>>()
        );
    }
}

#[shoop_wasm_test_support::shoop_test]
fn media_lead_in_preplay_boundary_then_take_alignment_are_independent() {
    let mut channel = AudioChannel::with_chunk_size(4, ChannelMode::Direct);
    channel.load_data(&(0..16).map(|frame| frame as f32).collect::<Vec<_>>());
    channel.set_start_offset(5);
    channel.set_pre_play_samples(3);
    channel.set_capture_alignment_frames(2).unwrap();

    channel.set_playback_buffer_size(1);
    channel
        .process(
            LoopMode::Stopped,
            LoopMode::Playing,
            Some(0),
            Some(3),
            1,
            0,
            8,
        )
        .unwrap();
    let mut boundary = [0.0];
    channel.finalize_process(&[], &mut boundary);
    assert_eq!(boundary, [4.0]);
    channel.set_playback_buffer_size(1);
    channel
        .process(LoopMode::Playing, LoopMode::Unknown, None, None, 1, 0, 8)
        .unwrap();
    let mut logical_start = [0.0];
    channel.finalize_process(&[], &mut logical_start);
    assert_eq!(logical_start, [7.0]);
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

fn playback_mode(role: ChannelMode) -> LoopMode {
    match role {
        ChannelMode::Dry => LoopMode::PlayingDryThroughWet,
        ChannelMode::Direct | ChannelMode::Wet => LoopMode::Playing,
        ChannelMode::Disabled => unreachable!("disabled is not a media role"),
    }
}

fn repeated_raw_frames(
    lead: i32,
    logical_event: i32,
    actual_advance: i32,
    loop_length: i32,
) -> Vec<u32> {
    (-4..=4)
        .filter_map(|cycle| {
            u32::try_from(lead + logical_event + actual_advance + cycle * loop_length).ok()
        })
        .collect()
}

fn render_audio_oracle(
    role: ChannelMode,
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_advance: i32,
    selected_advance: i32,
) -> Vec<u32> {
    let lead = (loop_length * 5) as i32;
    let raw_frames =
        repeated_raw_frames(lead, event_frame as i32, actual_advance, loop_length as i32);
    let data_length = (lead as u32 + loop_length * 3) as usize;
    let sample = identified_audio_sample(0x3f80_0042);
    let mut data = vec![0.0; data_length];
    for frame in raw_frames {
        if let Some(slot) = data.get_mut(frame as usize) {
            *slot = sample;
        }
    }
    let mut channel = AudioChannel::with_chunk_size(callback_size.max(1) as usize, role);
    channel.load_data(&data);
    channel.set_start_offset(lead);
    channel
        .set_capture_alignment_frames(selected_advance)
        .unwrap();

    let mut observed = Vec::new();
    for cycle in 0..2 {
        pump_callbacks(u64::from(loop_length), callback_size, |start, frames| {
            channel.set_recording_buffer_size(frames as usize);
            channel.set_playback_buffer_size(frames as usize);
            channel
                .process(
                    playback_mode(role),
                    LoopMode::Unknown,
                    None,
                    None,
                    frames as usize,
                    start as i32,
                    loop_length as usize,
                )
                .unwrap();
            let mut output = vec![0.0; frames as usize];
            channel.finalize_process(&[], &mut output);
            for (offset, value) in output.into_iter().enumerate() {
                if value == sample {
                    observed.push(cycle * loop_length + start as u32 + offset as u32);
                }
            }
        });
    }
    observed
}

fn render_midi_oracle(
    role: ChannelMode,
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_advance: i32,
    selected_advance: i32,
) -> Vec<(u32, u8)> {
    let lead = (loop_length * 5) as i32;
    let mut events = Vec::new();
    for frame in repeated_raw_frames(lead, event_frame as i32, actual_advance, loop_length as i32) {
        events.push(MidiStorageElem::new(frame, &midi::note_on(0, 66, 100)).unwrap());
        events.push(MidiStorageElem::new(frame, &midi::note_off(0, 66, 0)).unwrap());
    }
    let mut channel = MidiChannel::with_capacity_elems(128, role);
    channel.set_contents(&events, lead as u32 + loop_length * 3, Some(&[]));
    channel.set_start_offset(lead);
    channel
        .set_capture_alignment_frames(selected_advance)
        .unwrap();

    let mut observed = Vec::new();
    for cycle in 0..2 {
        pump_callbacks(u64::from(loop_length), callback_size, |start, frames| {
            channel.set_recording_buffer(frames);
            channel.set_playback_buffer(frames);
            let mut output = Vec::with_capacity(32);
            channel
                .process(
                    playback_mode(role),
                    LoopMode::Unknown,
                    None,
                    None,
                    frames,
                    start as i32,
                    start as u32 + frames,
                    loop_length,
                    &[],
                    &mut output,
                )
                .unwrap();
            for event in output {
                if midi::is_note_on(event.data()) && event.data()[1] == 66 {
                    observed.push((
                        cycle * loop_length + start as u32 + event.time,
                        event.data()[1],
                    ));
                }
            }
        });
    }
    observed
}

fn record_and_render_audio_oracle(
    role: ChannelMode,
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_advance: u32,
    selected_advance: i32,
) -> (usize, Vec<u32>) {
    let retained_after = actual_advance.saturating_add(2);
    let capacity = loop_length as usize + retained_after as usize;
    let sample = identified_audio_sample(0x3f80_0052);
    let physical_frame = event_frame + actual_advance;
    let mut channel =
        AudioChannel::with_bounded_capacity(callback_size.max(1) as usize, capacity, role);
    channel
        .prepare_latency_retention(loop_length as usize, 0, retained_after)
        .unwrap();

    pump_callbacks(u64::from(loop_length), callback_size, |start, frames| {
        let mut input = vec![0.0; frames as usize];
        if u64::from(physical_frame) >= start
            && u64::from(physical_frame) < start + u64::from(frames)
        {
            input[(u64::from(physical_frame) - start) as usize] = sample;
        }
        channel.set_recording_buffer_size(frames as usize);
        channel.set_playback_buffer_size(frames as usize);
        channel
            .process(
                LoopMode::Recording,
                LoopMode::Unknown,
                None,
                None,
                frames as usize,
                start as i32,
                start as usize,
            )
            .unwrap();
        channel.finalize_process(&input, &mut vec![0.0; frames as usize]);
    });
    pump_callbacks(u64::from(retained_after), callback_size, |start, frames| {
        let global_start = u64::from(loop_length) + start;
        let mut input = vec![0.0; frames as usize];
        if u64::from(physical_frame) >= global_start
            && u64::from(physical_frame) < global_start + u64::from(frames)
        {
            input[(u64::from(physical_frame) - global_start) as usize] = sample;
        }
        channel.set_recording_buffer_size(frames as usize);
        channel.set_playback_buffer_size(frames as usize);
        channel
            .process(
                LoopMode::Stopped,
                LoopMode::Unknown,
                None,
                None,
                frames as usize,
                0,
                loop_length as usize,
            )
            .unwrap();
        channel.finalize_process(&input, &mut vec![0.0; frames as usize]);
    });
    channel
        .set_capture_alignment_frames(selected_advance)
        .unwrap();

    let mut observed = Vec::new();
    for cycle in 0..2 {
        pump_callbacks(u64::from(loop_length), callback_size, |start, frames| {
            channel.set_recording_buffer_size(frames as usize);
            channel.set_playback_buffer_size(frames as usize);
            channel
                .process(
                    playback_mode(role),
                    LoopMode::Unknown,
                    None,
                    None,
                    frames as usize,
                    start as i32,
                    loop_length as usize,
                )
                .unwrap();
            let mut output = vec![0.0; frames as usize];
            channel.finalize_process(&[], &mut output);
            for (offset, value) in output.into_iter().enumerate() {
                if value == sample {
                    observed.push(cycle * loop_length + start as u32 + offset as u32);
                }
            }
        });
    }
    (physical_frame as usize, observed)
}

fn record_and_render_midi_oracle(
    role: ChannelMode,
    callback_size: u32,
    loop_length: u32,
    event_frame: u32,
    actual_advance: u32,
    selected_advance: i32,
) -> (u32, Vec<u32>) {
    let retained_after = actual_advance.saturating_add(2);
    let physical_frame = event_frame + actual_advance;
    let mut channel = MidiChannel::with_capacity_elems(256, role);
    channel
        .prepare_latency_retention(0, retained_after)
        .unwrap();

    pump_callbacks(u64::from(loop_length), callback_size, |start, frames| {
        let input = if u64::from(physical_frame) >= start
            && u64::from(physical_frame) < start + u64::from(frames)
        {
            let time = (u64::from(physical_frame) - start) as u32;
            vec![
                MidiStorageElem::new(time, &midi::note_on(0, 67, 100)).unwrap(),
                MidiStorageElem::new(time, &midi::note_off(0, 67, 0)).unwrap(),
            ]
        } else {
            Vec::new()
        };
        channel.set_recording_buffer(frames);
        channel.set_playback_buffer(frames);
        channel
            .process(
                LoopMode::Recording,
                LoopMode::Unknown,
                None,
                None,
                frames,
                start as i32,
                start as u32 + frames,
                start as u32,
                &input,
                &mut Vec::with_capacity(16),
            )
            .unwrap();
    });
    pump_callbacks(u64::from(retained_after), callback_size, |start, frames| {
        let global_start = u64::from(loop_length) + start;
        let input = if u64::from(physical_frame) >= global_start
            && u64::from(physical_frame) < global_start + u64::from(frames)
        {
            let time = (u64::from(physical_frame) - global_start) as u32;
            vec![
                MidiStorageElem::new(time, &midi::note_on(0, 67, 100)).unwrap(),
                MidiStorageElem::new(time, &midi::note_off(0, 67, 0)).unwrap(),
            ]
        } else {
            Vec::new()
        };
        channel.set_recording_buffer(frames);
        channel.set_playback_buffer(frames);
        channel
            .process(
                LoopMode::Stopped,
                LoopMode::Unknown,
                None,
                None,
                frames,
                0,
                0,
                loop_length,
                &input,
                &mut Vec::with_capacity(16),
            )
            .unwrap();
    });
    channel
        .set_capture_alignment_frames(selected_advance)
        .unwrap();

    let mut observed = Vec::new();
    for cycle in 0..2 {
        pump_callbacks(u64::from(loop_length), callback_size, |start, frames| {
            channel.set_recording_buffer(frames);
            channel.set_playback_buffer(frames);
            let mut output = Vec::with_capacity(32);
            channel
                .process(
                    playback_mode(role),
                    LoopMode::Unknown,
                    None,
                    None,
                    frames,
                    start as i32,
                    start as u32 + frames,
                    loop_length,
                    &[],
                    &mut output,
                )
                .unwrap();
            for event in output {
                if midi::is_note_on(event.data()) && event.data()[1] == 67 {
                    observed.push(cycle * loop_length + start as u32 + event.time);
                }
            }
        });
    }
    (physical_frame, observed)
}

#[shoop_wasm_test_support::shoop_test]
fn ordinary_compensated_playback_matrix_matches_exact_audio_midi_oracle() {
    for callback_size in [1_u32, 7, 64, 127] {
        let loop_length = callback_size.saturating_add(5).max(5);
        let mut event_frames = vec![
            0,
            1,
            callback_size.saturating_sub(1),
            callback_size,
            callback_size + 1,
            loop_length - 1,
            loop_length,
        ];
        event_frames.sort_unstable();
        event_frames.dedup();
        let component_values = [
            0,
            1,
            callback_size.saturating_sub(1),
            callback_size,
            callback_size + 1,
            loop_length - 1,
            loop_length,
            loop_length + 1,
        ];
        for role in [ChannelMode::Direct, ChannelMode::Dry, ChannelMode::Wet] {
            for event_frame in event_frames.iter().copied() {
                for component in component_values {
                    let actual = component as i32;
                    for selected in [
                        actual,
                        0,
                        actual.saturating_add(2),
                        actual.saturating_sub(2),
                    ] {
                        let expected = (event_frame as i64 + i64::from(actual)
                            - i64::from(selected))
                        .rem_euclid(i64::from(loop_length))
                            as u32;
                        let expected_frames = vec![expected, loop_length + expected];
                        assert_eq!(
                            render_audio_oracle(
                                role,
                                callback_size,
                                loop_length,
                                event_frame,
                                actual,
                                selected,
                            ),
                            expected_frames,
                            "audio role={role:?} B={callback_size} L={loop_length} E={event_frame} actual={actual} selected={selected}"
                        );
                        assert_eq!(
                            render_midi_oracle(
                                role,
                                callback_size,
                                loop_length,
                                event_frame,
                                actual,
                                selected,
                            ),
                            expected_frames
                                .iter()
                                .map(|frame| (*frame, 66))
                                .collect::<Vec<_>>(),
                            "MIDI role={role:?} B={callback_size} L={loop_length} E={event_frame} actual={actual} selected={selected}"
                        );
                    }
                }
            }
        }
    }
}

#[shoop_wasm_test_support::shoop_test]
fn record_then_play_matrix_matches_raw_and_logical_audio_midi_oracles() {
    for callback_size in [1_u32, 7, 64, 127] {
        let loop_length = callback_size.saturating_add(7).max(8);
        let mut event_frames = vec![
            0,
            1,
            callback_size.saturating_sub(1),
            callback_size,
            callback_size + 1,
            loop_length - 1,
        ];
        event_frames.sort_unstable();
        event_frames.dedup();
        let advances = [
            0,
            1,
            callback_size.saturating_sub(1),
            callback_size,
            callback_size + 1,
            loop_length - 1,
            loop_length,
            loop_length + 1,
        ];
        for role in [ChannelMode::Direct, ChannelMode::Dry, ChannelMode::Wet] {
            for event_frame in event_frames.iter().copied() {
                for actual in advances {
                    let mut selections = vec![actual as i32];
                    if event_frame + 2 < loop_length {
                        selections.push(actual as i32 - 2);
                    }
                    if event_frame >= 2 {
                        selections.push(actual as i32 + 2);
                    }
                    if event_frame + actual < loop_length {
                        selections.push(0);
                    }
                    selections.sort_unstable();
                    selections.dedup();
                    for selected in selections {
                        let expected = (i64::from(event_frame) + i64::from(actual)
                            - i64::from(selected))
                        .rem_euclid(i64::from(loop_length))
                            as u32;
                        let expected_frames = vec![expected, loop_length + expected];
                        let (audio_raw, audio_logical) = record_and_render_audio_oracle(
                            role,
                            callback_size,
                            loop_length,
                            event_frame,
                            actual,
                            selected,
                        );
                        assert_eq!(audio_raw, (event_frame + actual) as usize);
                        assert_eq!(
                            audio_logical, expected_frames,
                            "audio record role={role:?} B={callback_size} L={loop_length} E={event_frame} actual={actual} selected={selected}"
                        );
                        let (midi_raw, midi_logical) = record_and_render_midi_oracle(
                            role,
                            callback_size,
                            loop_length,
                            event_frame,
                            actual,
                            selected,
                        );
                        assert_eq!(midi_raw, event_frame + actual);
                        assert_eq!(
                            midi_logical, expected_frames,
                            "MIDI record role={role:?} B={callback_size} L={loop_length} E={event_frame} actual={actual} selected={selected}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
