//! Production service commands backed by the real host DSP banks.
#![cfg(any(feature = "builtin-fx", feature = "oxisynth"))]
use shoop_app::{ConnectionId, Service, ServiceOptions};
use shoop_app_api::*;
use shoop_app_client::Client;
use shoop_app_protocol::*;
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{testing::OfflineRender, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::{Capabilities, ProcessorKind, ProcessorPolicy, ProcessorSettings};
use shoop_rt_transport::carrier::Carrier;
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
struct Foundation<C>(C);
impl<C: Carrier> Carrier for Foundation<C> {
    fn send(
        &mut self,
        message: shoop_rt_protocol::ClientMessage,
    ) -> Result<(), shoop_transport::CarrierError> {
        self.0.send(message)
    }
    fn receive(&mut self) -> Option<shoop_rt_protocol::EngineMessage> {
        let mut message = self.0.receive()?;
        if let shoop_rt_protocol::EngineMessage::Welcome { capabilities, .. } = &mut message {
            *capabilities = capabilities
                .union(Capabilities::DRY_WET)
                .union(Capabilities::TRACK_PROCESSORS)
                .union(Capabilities::GLOBAL_FX_CONTROL);
        }
        Some(message)
    }
}
struct Fixture {
    service: Service,
    client: Client,
    connection: ConnectionId,
    render: Rc<RefCell<OfflineRender>>,
}
impl Fixture {
    fn new(kind: ProcessorKind) -> Self {
        let description = EngineDescription {
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
        let client =
            shoop_rt_transport::client::ProtocolClient::new(Foundation(client.into_carrier()));
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
    f.confirmed(control(TrackAction::Remove));
    f.ticks(4);
    assert!(f
        .client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .all(|entry| entry.id != track));
}
