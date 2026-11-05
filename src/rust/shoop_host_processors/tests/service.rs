//! Production service commands backed by the real host DSP banks.
#![cfg(any(feature = "builtin-fx", feature = "oxisynth"))]
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::*;
use shoop_app_client::Client;
use shoop_app_protocol::*;
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{testing::OfflineRender, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::{ProcessorKind, ProcessorPolicy, ProcessorSettings};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
use std::{cell::RefCell, rc::Rc, time::Duration};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
const STEP: Duration = Duration::from_millis(16);

struct Clock(Rc<RefCell<OfflineRender>>);
impl EngineClock for Clock {
    fn advance(&mut self, elapsed: Duration) {
        assert_no_heap(|| {
            self.0
                .borrow_mut()
                .run_for(elapsed, |port, _| port as f32 + 0.25)
        })
        .unwrap();
    }
    fn wait(&mut self) -> bool {
        assert_no_heap(|| self.0.borrow_mut().run(16, |port, _| port as f32 + 0.25)).unwrap();
        true
    }
}
struct Fixture {
    service: Service,
    client: Client,
    connection: ConnectionId,
    render: Rc<RefCell<OfflineRender>>,
}
impl Fixture {
    fn persistence(&mut self, request: persistence::Request) {
        let message = WireMessage::Client(self.client.persistence(request).unwrap());
        let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
        let used = codec::encode(&message, &mut bytes).unwrap();
        let WireMessage::Client(message) = codec::decode(&bytes[..used]).unwrap() else {
            panic!()
        };
        self.service.handle(self.connection, message).unwrap();
        self.drain();
    }
    fn capture(
        &mut self,
        transfer: u64,
        mut after_cut: Option<Command>,
    ) -> std::sync::Arc<persistence_image::Image> {
        let key = persistence::Key {
            transfer: TransferId(transfer),
            epoch: self.client.snapshot().unwrap().persistence_epoch,
        };
        let target = persistence::Target::Session {
            revision: self.client.snapshot().unwrap().revision,
        };
        self.persistence(persistence::Request::Capture { key, target });
        let mut captured = None;
        for _ in 0..500 {
            self.ticks(1);
            if let Some(command) = after_cut.take() {
                self.confirmed(command);
            }
            while let Some(reply) = self.client.take_persistence_reply() {
                match reply {
                    persistence::Reply::Captured(description) => captured = Some(description),
                    persistence::Reply::Refused { reason, .. } => {
                        panic!("capture refused: {reason:?}")
                    }
                    _ => {}
                }
            }
            if captured.is_some() {
                break;
            }
        }
        let captured = captured.expect("capture stalled");
        let mut objects = Vec::new();
        for object in 0..captured.objects {
            self.persistence(persistence::Request::Describe { key, object });
            let persistence::Reply::Object {
                object: description,
                ..
            } = self.client.take_persistence_reply().unwrap()
            else {
                panic!()
            };
            let mut bytes = Vec::new();
            while (bytes.len() as u64) < description.bytes {
                self.persistence(persistence::Request::Read {
                    key,
                    object,
                    offset: bytes.len() as u64,
                });
                let persistence::Reply::Chunk { bytes: chunk, .. } =
                    self.client.take_persistence_reply().unwrap()
                else {
                    panic!()
                };
                bytes.extend_from_slice(&chunk);
            }
            objects.push(persistence::Object { description, bytes });
        }
        self.persistence(persistence::Request::Cancel { key });
        assert!(matches!(
            self.client.take_persistence_reply(),
            Some(persistence::Reply::Closed { .. })
        ));
        persistence_transfer::assemble(objects).unwrap()
    }
    fn load(&mut self, image: std::sync::Arc<persistence_image::Image>, transfer: u64) {
        let key = persistence::Key {
            transfer: TransferId(transfer),
            epoch: self.client.snapshot().unwrap().persistence_epoch,
        };
        let target = persistence::Target::Session {
            revision: self.client.snapshot().unwrap().revision,
        };
        let mut upload = persistence_transfer::UploadStream::new(image, key, target).unwrap();
        while let Some(request) = upload.next_request().unwrap() {
            self.persistence(request);
        }
        let mut ready = false;
        for _ in 0..100 {
            self.ticks(1);
            while let Some(reply) = self.client.take_persistence_reply() {
                match reply {
                    persistence::Reply::Ready { .. } => ready = true,
                    persistence::Reply::Refused { reason, .. } => {
                        panic!("stage refused: {reason:?}")
                    }
                    _ => {}
                }
            }
            if ready {
                break;
            }
        }
        assert!(ready, "staging stalled");
        self.confirmed(Command::CommitPersistence(key));
    }
    fn new(kind: ProcessorKind) -> Self {
        let description = EngineDescription {
            external_dry_wet: true,
            sample_rate: 8000,
            quantum: 16,
            max_frames: 128,
            ramp_frames: 0,
            telemetry_interval_frames: 0,
            strip_channels: 4,
            master_channels: 2,
            host: HostPorts::numbered(4, 4, 1),
            segments: vec![Segment {
                strips: 4,
                loops: 32,
            }],
            resource_pages: 8192,
            page_bytes: 256,
            processors: ProcessorSettings {
                builtin_fx: if kind == ProcessorKind::BuiltInFx {
                    ProcessorPolicy::On
                } else {
                    ProcessorPolicy::Off
                },
                oxisynth: if kind == ProcessorKind::OxiSynth {
                    ProcessorPolicy::On
                } else {
                    ProcessorPolicy::Off
                },
                fx_instances: 1,
                synth_instances: 1,
                memory_mb: 128,
            },
            ..Default::default()
        };
        let (render, client) = OfflineRender::new_with_processors(
            &description,
            shoop_host_processors::prepare(&description).unwrap(),
        )
        .unwrap();
        let render = Rc::new(RefCell::new(render));
        let backend = RtBackend::connect(client, Clock(render.clone())).unwrap();
        let mut service = Service::new(
            Box::new(backend),
            ServiceOptions {
                master_auto_connect: true,
                loop_smoothing_ms: 0,
            },
        )
        .unwrap();
        for _ in 0..100 {
            service.tick(STEP);
            if service.is_ready() {
                break;
            }
        }
        assert!(service.is_ready());
        let mut f = Self {
            service,
            client: Client::new(),
            connection: ConnectionId(1),
            render,
        };
        f.attach();
        f
    }
    fn drain(&mut self) {
        while let Some(message) = self.service.take_update(self.connection) {
            let mut encoder = stream::StreamEncoder::new(message, TransferId(1)).unwrap();
            while let Some(message) = encoder.next_message() {
                let mut bytes = [0; shoop_transport::MAX_RECORD_BYTES];
                let used = codec::encode(&message, &mut bytes).unwrap();
                self.client
                    .receive_wire(codec::decode(&bytes[..used]).unwrap())
                    .unwrap();
            }
        }
    }
    fn attach(&mut self) {
        self.service
            .handle(self.connection, self.client.attach().unwrap())
            .unwrap();
        self.drain();
        self.service
            .handle(self.connection, self.client.ready_message().unwrap())
            .unwrap();
        self.client.ready_sent().unwrap();
    }
    fn ticks(&mut self, count: usize) {
        for _ in 0..count {
            self.service.tick(STEP);
            self.drain();
        }
    }
    fn submit(&mut self, command: Command) -> OperationId {
        let message = self.client.command(command, None).unwrap();
        let ClientMessage::Submit { operation, .. } = message else {
            unreachable!()
        };
        self.service.handle(self.connection, message).unwrap();
        operation
    }
    fn run(&mut self, command: Command) -> Outcome {
        let operation = self.submit(command);
        for _ in 0..100 {
            self.drain();
            if let Some(outcome) = self.client.results().get(&operation) {
                return outcome.clone();
            }
            self.ticks(1);
        }
        panic!("service command did not finish");
    }
    fn confirmed(&mut self, command: Command) {
        assert!(matches!(self.run(command), Outcome::Confirmed { .. }));
    }
    fn track(&self, id: TrackId) -> TrackState {
        self.client
            .snapshot()
            .unwrap()
            .tracks
            .iter()
            .find(|track| track.id == id)
            .unwrap()
            .clone()
    }
}

#[shoop_test]
fn external_unequal_roles_survive_the_service_and_session_archive() {
    use persistence_image::{ChannelMode, Manifest, Payload};
    let mut f = Fixture::new(ProcessorKind::External);
    f.confirmed(Command::Global(GlobalControlAction::SetSync(false)));
    let Outcome::TrackCreated { track, loops, .. } = f.run(Command::AddTrack(TrackSpec {
        name: "External roles".into(),
        topology: TrackSpecTopology::DryWet {
            dry_audio_channels: 3,
            wet_audio_channels: 1,
            dry_midi: true,
            processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::EXTERNAL),
            default_playback_mode: DefaultPlaybackMode::DryThroughWet,
        },
        latency: Default::default(),
        initial_output_bus_names: vec!["Master".into()],
        creation_request_id: None,
    })) else {
        panic!("External creation refused")
    };
    let before = f.track(track);
    f.confirmed(Command::Loop {
        track,
        loop_id: loops[0],
        action: LoopAction::RecordClicked,
    });
    f.ticks(4);
    f.confirmed(Command::Loop {
        track,
        loop_id: loops[0],
        action: LoopAction::StopClicked,
    });
    let saved = file_image(f.capture(110, None));
    let Manifest::Session(session) = &saved.manifest else {
        panic!()
    };
    let stored = session.tracks.iter().find(|t| t.id == track).unwrap();
    let take = &stored.loops[0];
    assert_eq!(take.audio_labels(), ["Dry 1", "Dry 2", "Dry 3", "Wet 1"]);
    assert_eq!(
        take.audio.iter().map(|c| c.mode).collect::<Vec<_>>(),
        [
            ChannelMode::Dry,
            ChannelMode::Dry,
            ChannelMode::Dry,
            ChannelMode::Wet
        ]
    );
    assert_eq!(take.midi[0].channel.mode, ChannelMode::Dry);
    assert!(take.recorded_processor_state.is_none());
    f.load(saved.clone(), 111);
    let loaded = f
        .client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .find(|t| t.name == before.name)
        .unwrap()
        .clone();
    assert_ne!(loaded.id, track);
    assert_eq!(loaded.topology, before.topology);
    assert_eq!(loaded.default_playback_mode, before.default_playback_mode);
    assert_eq!(loaded.loops[0].mode, LoopMode::Stopped);
    let resaved = file_image(f.capture(112, None));
    let Manifest::Session(session) = &resaved.manifest else {
        panic!()
    };
    let after = &session
        .tracks
        .iter()
        .find(|t| t.id == loaded.id)
        .unwrap()
        .loops[0];
    let samples = |image: &persistence_image::Image, object: Option<u32>| match object {
        Some(object) => match &image.payloads[object as usize - 1] {
            Payload::Audio(samples) => samples.to_vec(),
            _ => panic!(),
        },
        None => Vec::new(),
    };
    for (a, b) in take.audio.iter().zip(&after.audio) {
        assert_eq!(a.mode, b.mode);
        assert_eq!(samples(&saved, a.object), samples(&resaved, b.object));
    }
    assert_eq!(after.midi[0].initial, take.midi[0].initial);
    structure_only_round_trip(&mut f, &resaved, &before.name, 120);
}

#[cfg(feature = "builtin-fx")]
#[shoop_test]
fn fx_editor_learn_recorded_restore_and_detached_controls_run_through_the_service_wire() {
    service_processor(ProcessorKind::BuiltInFx);
}
#[cfg(feature = "oxisynth")]
#[shoop_test]
fn synth_editor_learn_recorded_restore_and_detached_controls_run_through_the_service_wire() {
    service_processor(ProcessorKind::OxiSynth);
}
fn service_processor(kind: ProcessorKind) {
    let mut f = Fixture::new(kind);
    let type_id = if kind == ProcessorKind::BuiltInFx {
        TrackProcessorTypeId::BUILTIN_FX
    } else {
        TrackProcessorTypeId::OXISYNTH
    };
    let channels = if kind == ProcessorKind::BuiltInFx {
        3
    } else {
        2
    };
    let catalog = f.client.snapshot().unwrap().track_processors.clone();
    let resources = catalog
        .iter()
        .find(|descriptor| descriptor.id.as_str() == type_id)
        .unwrap()
        .resources
        .unwrap();
    assert_eq!(
        (
            resources.instance_limit,
            resources.instances_in_use,
            resources.buffering_frames
        ),
        (1, 0, 16)
    );
    assert!(
        resources.reserved_host_bytes > 0
            && resources.reserved_host_bytes <= resources.budget_host_bytes
    );
    assert!(catalog
        .iter()
        .any(|descriptor| descriptor.id.as_str() == type_id
            && descriptor.available
            && descriptor.editor.is_some()));
    f.confirmed(Command::Global(GlobalControlAction::SetSync(false)));
    let Outcome::TrackCreated { track, loops, .. } = f.run(Command::AddTrack(TrackSpec {
        name: type_id.into(),
        topology: TrackSpecTopology::DryWet {
            dry_audio_channels: channels,
            wet_audio_channels: channels,
            dry_midi: true,
            processor_type: TrackProcessorTypeId::new(type_id),
            default_playback_mode: DefaultPlaybackMode::DryThroughWet,
        },
        latency: Default::default(),
        initial_output_bus_names: vec!["Master".into()],
        creation_request_id: None,
    })) else {
        panic!("processed track creation failed")
    };
    let snapshot = f.client.snapshot().unwrap().clone();
    let outputs = snapshot
        .connections
        .application_ports
        .iter()
        .filter(|port| {
            matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track)
                && port.role == PortRole::AudioOutput
        })
        .map(|port| port.id)
        .collect::<Vec<_>>();
    for (&source, channel) in outputs.iter().zip(snapshot.buses[0].channels.iter()) {
        f.confirmed(Command::SetMixerRoute {
            source,
            destination: channel.id,
            connected: true,
        });
    }
    let control = |action| Command::Track { track, action };
    let loop_action = |action| Command::Loop {
        track,
        loop_id: loops[0],
        action,
    };
    f.confirmed(control(TrackAction::FxVisibilityChanged(true)));
    f.confirmed(control(TrackAction::InputMonitoringChanged {
        enabled: true,
        respect_auto_mute: false,
    }));
    if kind == ProcessorKind::BuiltInFx {
        for action in [
            BuiltInFxControl::SetStageEnabled(BuiltInFxStage::Drive, true),
            BuiltInFxControl::SetReverbEnabled(false),
            BuiltInFxControl::SetDriveType(BuiltInFxDriveType::Fuzz),
            BuiltInFxControl::SetParameter(BuiltInFxParameter::DriveMix, 0.),
            BuiltInFxControl::SetParameter(BuiltInFxParameter::DriveOutput, -6.),
            BuiltInFxControl::AssignMidiCc(BuiltInFxMidiCcAssignment {
                parameter: BuiltInFxParameter::DriveOutput,
                channel: 2,
                controller: 17,
            }),
        ] {
            f.confirmed(control(TrackAction::BuiltInFx(action)));
        }
    } else {
        for action in [
            OxiSynthControl::SelectPreset("0:40".into()),
            OxiSynthControl::SetReverbSend(0.25),
            OxiSynthControl::SetChorusSend(0.5),
            OxiSynthControl::AssignMidiCc(OxiSynthMidiCcAssignment {
                parameter: OxiSynthParameter::ReverbSend,
                channel: 2,
                controller: 17,
            }),
        ] {
            f.confirmed(control(TrackAction::OxiSynth(action)));
        }
        f.render
            .borrow_mut()
            .midi_input_mut(0)
            .push(3, 128, &[0x90, 60, 120])
            .unwrap();
    }
    f.ticks(8);
    let state = f.track(track);
    let catalog = &f.client.snapshot().unwrap().track_processors;
    assert_eq!(
        catalog
            .iter()
            .find(|descriptor| descriptor.id.as_str() == type_id)
            .unwrap()
            .resources
            .unwrap()
            .instances_in_use,
        1
    );
    assert_eq!(state.latency.automatic_processor_advance_frames, Some(16));
    assert_eq!(state.latency.effective_processor_advance_frames, Some(16));
    assert_eq!(
        state.latency.processor_adjustment,
        ProcessorLatencyAdjustmentState::Automatic
    );
    assert_eq!(
        state.default_playback_mode,
        DefaultPlaybackMode::DryThroughWet
    );
    assert_eq!(state.controls.input_stereo, channels == 2);
    assert_eq!(state.loops[0].stereo, channels == 2);
    if channels != 2 {
        let expected = 20. * (channels as f32 - 0.75).log10();
        assert!((state.controls.input_peak_left_db - expected).abs() < 1e-4);
        assert_eq!(
            state.controls.input_peak_left_db,
            state.controls.input_peak_right_db
        );
        assert_eq!(
            state.controls.output_peak_left_db,
            state.controls.output_peak_right_db
        );
    }

    assert!(state.fx.as_ref().unwrap().active && state.fx.as_ref().unwrap().visible);
    assert!(f.render.borrow().output(0)[64..128]
        .iter()
        .any(|sample| sample.abs() > 1e-4));
    f.render
        .borrow_mut()
        .midi_input_mut(0)
        .push(5, 128, &[0xb2, 17, 0])
        .unwrap();
    f.ticks(8);
    let learned = f.track(track);
    assert_eq!(
        learned
            .controls
            .latest_input_midi_message
            .unwrap()
            .midi_cc(),
        Some((2, 17, 0))
    );
    match learned.fx.unwrap().editor.unwrap() {
        TrackProcessorEditorState::BuiltInFx(state) => assert_eq!(state.drive_output_db, -18.),
        TrackProcessorEditorState::OxiSynth(state) => {
            assert_eq!(state.reverb_send, 0.);
            assert_eq!(state.selected_preset_id, "0:40");
        }
    }
    f.confirmed(loop_action(LoopAction::RecordClicked));
    f.ticks(3);
    f.confirmed(loop_action(LoopAction::StopClicked));
    f.ticks(4);
    assert!(f.track(track).loops[0].has_recorded_fx_state);
    assert_eq!(f.track(track).loops[0].processor_alignment_frames, Some(16));
    let message = f.client.read_media(loops[0]).unwrap();
    f.service.handle(f.connection, message).unwrap();
    for _ in 0..200 {
        f.ticks(1);
        if f.client.media().is_some() {
            break;
        }
    }
    let media = f.client.media().unwrap().as_ref().unwrap().clone();
    assert_eq!(media.audio.len(), channels as usize * 2);
    for (channel, dry) in media.audio[..channels as usize].iter().enumerate() {
        assert!(dry.samples.len() >= 128);
        assert!(dry
            .samples
            .iter()
            .all(|sample| *sample == channel as f32 + 0.25));
    }
    if kind == ProcessorKind::BuiltInFx {
        let expected = 0.25 * 10.0_f32.powf(-18. / 20.);
        assert!(media.audio[channels as usize]
            .samples
            .iter()
            .all(|sample| (*sample - expected).abs() < 1e-6));
    } else {
        assert!(media.audio[channels as usize]
            .samples
            .iter()
            .any(|sample| sample.abs() > 1e-4));
    }

    let edit = if kind == ProcessorKind::BuiltInFx {
        TrackAction::BuiltInFx(BuiltInFxControl::SetParameter(
            BuiltInFxParameter::DriveOutput,
            -3.,
        ))
    } else {
        TrackAction::OxiSynth(OxiSynthControl::SetReverbSend(0.75))
    };
    let operation = f.submit(control(edit));
    f.service.detach(f.connection);
    for _ in 0..8 {
        f.service.tick(STEP);
    }
    f.client = Client::new();
    f.connection = ConnectionId(2);
    f.attach();
    let results = &f.client.snapshot().unwrap().results;
    assert_eq!(
        results
            .iter()
            .filter(|result| result.id == operation)
            .count(),
        1
    );
    assert!(results.iter().any(
        |result| result.id == operation && matches!(result.outcome, Outcome::Confirmed { .. })
    ));
    if kind == ProcessorKind::BuiltInFx {
        f.confirmed(control(TrackAction::BuiltInFx(
            BuiltInFxControl::AssignMidiCc(BuiltInFxMidiCcAssignment {
                parameter: BuiltInFxParameter::DriveOutput,
                channel: 2,
                controller: 19,
            }),
        )));
    } else {
        f.confirmed(control(TrackAction::OxiSynth(
            OxiSynthControl::SelectPreset("0:41".into()),
        )));
        f.confirmed(control(TrackAction::OxiSynth(
            OxiSynthControl::AssignMidiCc(OxiSynthMidiCcAssignment {
                parameter: OxiSynthParameter::ReverbSend,
                channel: 2,
                controller: 19,
            }),
        )));
    }
    f.confirmed(loop_action(LoopAction::RestoreRecordedFxState));
    f.ticks(4);
    match f.track(track).fx.unwrap().editor.unwrap() {
        TrackProcessorEditorState::BuiltInFx(state) => {
            assert_eq!(state.drive_output_db, -18.);
            assert_eq!(state.midi_cc_assignments.len(), 1);
            assert_eq!(state.midi_cc_assignments[0].controller, 19);
        }
        TrackProcessorEditorState::OxiSynth(state) => {
            assert_eq!(state.reverb_send, 0.);
            assert_eq!(state.chorus_send, 0.5);
            assert_eq!(state.selected_preset_id, "0:40");
            assert_eq!(state.midi_cc_assignments.len(), 1);
            assert_eq!(state.midi_cc_assignments[0].controller, 19);
        }
    }
    assert!(matches!(
        f.run(control(TrackAction::FxRestoreState("malformed".into()))),
        Outcome::Refused { .. }
    ));
    assert!(
        !f.track(track).loops[0].empty,
        "{:?}",
        f.track(track).loops[0]
    );
    f.confirmed(loop_action(LoopAction::PlayClicked));
    f.ticks(3);
    assert_eq!(
        f.track(track).loops[0].mode,
        LoopMode::Playing,
        "{:?}",
        f.track(track).loops[0]
    );
    f.confirmed(control(TrackAction::FxActiveChanged(false)));
    f.ticks(4);
    assert!(!f.track(track).fx.unwrap().active);
    assert!(f.render.borrow().output(0)[64..128]
        .iter()
        .any(|sample| sample.abs() > 1e-4));
    f.confirmed(control(TrackAction::FxActiveChanged(true)));
    f.ticks(4);
    assert!(f.track(track).fx.unwrap().active);
    let invalid_mapping = if kind == ProcessorKind::BuiltInFx {
        TrackAction::BuiltInFx(BuiltInFxControl::AssignMidiCc(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::DriveOutput,
            channel: 2,
            controller: 200,
        }))
    } else {
        TrackAction::OxiSynth(OxiSynthControl::AssignMidiCc(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ReverbSend,
            channel: 2,
            controller: 200,
        }))
    };
    assert!(matches!(
        f.run(control(invalid_mapping)),
        Outcome::Refused { .. }
    ));
    let remove = if kind == ProcessorKind::BuiltInFx {
        TrackAction::BuiltInFx(BuiltInFxControl::RemoveMidiCc(
            BuiltInFxParameter::DriveOutput,
        ))
    } else {
        TrackAction::OxiSynth(OxiSynthControl::RemoveMidiCc(OxiSynthParameter::ReverbSend))
    };
    f.confirmed(control(remove));
    let clear = if kind == ProcessorKind::BuiltInFx {
        TrackAction::BuiltInFx(BuiltInFxControl::ClearMidiCcAssignments)
    } else {
        TrackAction::OxiSynth(OxiSynthControl::ClearMidiCcAssignments)
    };
    f.confirmed(control(clear));
    f.ticks(4);
    match f.track(track).fx.unwrap().editor.unwrap() {
        TrackProcessorEditorState::BuiltInFx(state) => {
            assert!(state.midi_cc_assignments.is_empty())
        }
        TrackProcessorEditorState::OxiSynth(state) => assert!(state.midi_cc_assignments.is_empty()),
    }
    let loaded = session_file_round_trip(&mut f, track, kind);
    f.confirmed(Command::Track {
        track: loaded,
        action: TrackAction::Remove,
    });
    f.ticks(4);
    assert!(f
        .client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .all(|entry| entry.id != loaded));
}

fn file_image(
    image: std::sync::Arc<persistence_image::Image>,
) -> std::sync::Arc<persistence_image::Image> {
    use shoop_persistence_codec::codec::{Value, Work};
    let Value::File { bytes, .. } = Work::Encode {
        image,
        kind: IoTaskKind::SaveSession,
        wav: false,
        standard: false,
        channels: Vec::new(),
    }
    .run()
    .unwrap() else {
        panic!()
    };
    let Value::Decoded(decoded) = Work::Decode {
        kind: IoTaskKind::LoadSession,
        bytes,
        rate: 8000,
    }
    .run()
    .unwrap() else {
        panic!()
    };
    let Value::Image(image) = Work::Prepare {
        decoded,
        mapping: Vec::new(),
        update_length: true,
    }
    .run()
    .unwrap() else {
        panic!()
    };
    image
}
fn session_file_round_trip(f: &mut Fixture, track: TrackId, kind: ProcessorKind) -> TrackId {
    use persistence_image::{Image, Manifest, Payload};
    let scalar = |value| {
        if kind == ProcessorKind::BuiltInFx {
            TrackAction::BuiltInFx(BuiltInFxControl::SetParameter(
                BuiltInFxParameter::DriveOutput,
                value,
            ))
        } else {
            TrackAction::OxiSynth(OxiSynthControl::SetReverbSend(value))
        }
    };
    let (saved_value, late_value) = if kind == ProcessorKind::BuiltInFx {
        (-3., -6.)
    } else {
        (0.25, 0.75)
    };
    f.confirmed(Command::Track {
        track,
        action: scalar(saved_value),
    });
    let mapping = if kind == ProcessorKind::BuiltInFx {
        TrackAction::BuiltInFx(BuiltInFxControl::AssignMidiCc(BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::DriveOutput,
            channel: 2,
            controller: 22,
        }))
    } else {
        TrackAction::OxiSynth(OxiSynthControl::AssignMidiCc(OxiSynthMidiCcAssignment {
            parameter: OxiSynthParameter::ReverbSend,
            channel: 2,
            controller: 22,
        }))
    };
    f.confirmed(Command::Track {
        track,
        action: mapping,
    });
    f.ticks(4);
    let before = f.track(track);
    let expected = before
        .fx
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .canonical_state()
        .unwrap();
    let captured = f.capture(
        100,
        Some(Command::Track {
            track,
            action: scalar(late_value),
        }),
    );
    assert_ne!(
        f.track(track)
            .fx
            .unwrap()
            .editor
            .unwrap()
            .canonical_state()
            .unwrap(),
        expected
    );
    let encoded = file_image(captured);
    let Manifest::Session(session) = &encoded.manifest else {
        panic!()
    };
    let stored = session.tracks.iter().find(|t| t.id == track).unwrap();
    assert_eq!(stored.processor_state.as_deref(), Some(expected.as_str()));
    assert_eq!(stored.default_playback_mode, before.default_playback_mode);
    let take = stored.loops.iter().find(|l| l.frames > 0).unwrap();
    assert!(take.recorded_processor_state.is_some());
    assert_eq!(
        stored.builtin_fx_midi_cc_assignments.len() + stored.oxisynth_midi_cc_assignments.len(),
        1
    );
    f.load(encoded.clone(), 101);
    let loaded = f
        .client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .find(|t| t.name == before.name)
        .unwrap()
        .clone();
    assert_ne!(loaded.id, track);
    assert_eq!(loaded.topology, before.topology);
    assert_eq!(loaded.default_playback_mode, before.default_playback_mode);
    assert_eq!(
        loaded
            .fx
            .as_ref()
            .unwrap()
            .editor
            .as_ref()
            .unwrap()
            .canonical_state()
            .unwrap(),
        expected
    );
    let resaved = file_image(f.capture(102, None));
    let Manifest::Session(after) = &resaved.manifest else {
        panic!()
    };
    let after = after.tracks.iter().find(|t| t.id == loaded.id).unwrap();
    assert_eq!(after.processor_state, stored.processor_state);
    assert_eq!(
        after.builtin_fx_midi_cc_assignments,
        stored.builtin_fx_midi_cc_assignments
    );
    assert_eq!(
        after.oxisynth_midi_cc_assignments,
        stored.oxisynth_midi_cc_assignments
    );
    let after_take = after.loops.iter().find(|l| l.frames > 0).unwrap();
    assert_eq!(
        after_take.recorded_processor_state,
        take.recorded_processor_state
    );
    assert_eq!(after_take.frames, take.frames);
    assert_eq!(after_take.audio.len(), take.audio.len());
    let audio = |image: &Image, object: Option<u32>| match object {
        Some(object) => match &image.payloads[object as usize - 1] {
            Payload::Audio(samples) => samples.to_vec(),
            _ => panic!(),
        },
        None => Vec::new(),
    };
    for (a, b) in after_take.audio.iter().zip(&take.audio) {
        assert_eq!(a.mode, b.mode);
        assert_eq!(audio(&resaved, a.object), audio(&encoded, b.object));
        assert_eq!(
            (
                a.frames,
                a.gain,
                a.start_offset,
                a.capture_alignment,
                a.preplay
            ),
            (
                b.frames,
                b.gain,
                b.start_offset,
                b.capture_alignment,
                b.preplay
            )
        );
    }
    let events = |image: &Image, object: Option<u32>| match object {
        Some(object) => match &image.payloads[object as usize - 1] {
            Payload::Midi(events) => events.to_vec(),
            _ => panic!(),
        },
        None => Vec::new(),
    };
    for (a, b) in after_take.midi.iter().zip(&take.midi) {
        assert_eq!(a.initial, b.initial);
        assert_eq!(
            events(&resaved, a.channel.object),
            events(&encoded, b.channel.object)
        );
    }
    f.confirmed(Command::Loop {
        track: loaded.id,
        loop_id: loaded.loops[0].id,
        action: LoopAction::RestoreRecordedFxState,
    });
    f.ticks(4);
    let restored = f.track(loaded.id).fx.unwrap().editor.unwrap();
    assert_eq!(
        restored.canonical_state().unwrap(),
        take.recorded_processor_state.as_ref().unwrap().as_str()
    );
    match restored {
        TrackProcessorEditorState::BuiltInFx(state) => {
            assert_eq!(state.midi_cc_assignments[0].controller, 22)
        }
        TrackProcessorEditorState::OxiSynth(state) => {
            assert_eq!(state.midi_cc_assignments[0].controller, 22)
        }
    }
    structure_only_round_trip(f, &resaved, &before.name, 130)
}

fn structure_only_round_trip(
    f: &mut Fixture,
    source: &persistence_image::Image,
    name: &str,
    transfer: u64,
) -> TrackId {
    use persistence_image::Manifest;
    let mut image = source.clone();
    let Manifest::Session(session) = &mut image.manifest else {
        panic!()
    };
    for track in &mut session.tracks {
        for looped in &mut track.loops {
            looped.frames = 0;
            looped.recorded_processor_state = None;
            for channel in &mut looped.audio {
                channel.object = None;
                channel.frames = 0;
                channel.capture_alignment = 0;
                channel.recording_started_at = None;
            }
            for midi in &mut looped.midi {
                midi.channel.object = None;
                midi.channel.frames = 0;
                midi.channel.capture_alignment = 0;
                midi.channel.recording_started_at = None;
                midi.initial.clear();
            }
        }
    }
    image.payloads.clear();
    let source = file_image(std::sync::Arc::new(image));
    f.load(source.clone(), transfer);
    let state = f.client.snapshot().unwrap();
    let loaded = state.tracks.iter().find(|t| t.name == name).unwrap().id;
    let resaved = file_image(f.capture(transfer + 1, None));
    let Manifest::Session(before) = &source.manifest else {
        panic!()
    };
    let Manifest::Session(after) = &resaved.manifest else {
        panic!()
    };
    let before = before.tracks.iter().find(|t| t.name == name).unwrap();
    let after = after.tracks.iter().find(|t| t.name == name).unwrap();
    assert_eq!(after.topology, before.topology);
    assert_eq!(after.processor_state, before.processor_state);
    assert_eq!(after.default_playback_mode, before.default_playback_mode);
    assert_eq!(
        after.builtin_fx_midi_cc_assignments,
        before.builtin_fx_midi_cc_assignments
    );
    assert_eq!(
        after.oxisynth_midi_cc_assignments,
        before.oxisynth_midi_cc_assignments
    );
    assert_eq!(
        after.loops[0].audio_labels(),
        before.loops[0].audio_labels()
    );
    assert!(after
        .loops
        .iter()
        .all(|looped| looped.frames == 0 && looped.recorded_processor_state.is_none()));
    loaded
}
