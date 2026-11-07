// Frozen frontend acceptance scenarios from 925db5b2. Migration evidence only.
// Restore these expectations through the current client/service route as parity returns.
    fn confirmed_driver_switch_is_saved_once_and_completed_after_persistence() {
        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
        app.runtime.tick(Duration::ZERO);
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&app.settings.active());
        draft.set(shoop_egui::DUMMY_SAMPLE_RATE, 32_000);
        draft.set(shoop_egui::DUMMY_BUFFER_SIZE, 128);
        let config =
            shoop_egui::audio_driver_config_from_draft(&draft, shoop_egui::AudioDriverKind::Dummy)
                .unwrap();
        app.handle_settings_action(SettingsAction::RequestAudioDriverSwitch { config, draft });
        let deadline = Instant::now() + Duration::from_secs(3);
        let prepared = loop {
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            if snapshot.audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
            {
                break snapshot;
            }
            assert!(Instant::now() < deadline, "driver preflight timed out");
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            prepared.audio_drivers.switch.status,
            shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
        );
        let request_id = prepared.audio_drivers.switch.request_id;
        app.runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.runtime.tick(Duration::ZERO);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Persisting
            {
                break;
            }
            assert!(Instant::now() < deadline, "driver switch timed out");
            thread::sleep(Duration::from_millis(5));
        }

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.settings.poll();
            let snapshot = app.runtime.snapshot();
            app.reconcile_audio_settings(&snapshot);
            app.runtime.tick(Duration::ZERO);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Completed
            {
                break;
            }
            assert!(Instant::now() < deadline, "driver settings save timed out");
            thread::sleep(Duration::from_millis(5));
        }
        let active = app.settings.active();
        assert_eq!(active.get(shoop_egui::DUMMY_SAMPLE_RATE).unwrap(), 32_000);
        assert_eq!(
            active.get(shoop_egui::SELECTED_AUDIO_DRIVER).unwrap(),
            "dummy"
        );
        assert!(app.pending_audio_settings.is_none());
    }

    fn failed_driver_settings_save_retries_without_switching_backend_again() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("blocked");
        let path = blocker.join("settings.json");
        let mut builder = SettingsRegistryBuilder::default();
        register_settings(&mut builder).unwrap();
        register_audio_settings(&mut builder).unwrap();
        register_script_settings(&mut builder).unwrap();
        let manager = SettingsManager::load_from_path(builder.finish(), "test", path);
        std::fs::write(&blocker, b"not a directory").unwrap();

        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
        app.settings = manager;
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&app.settings.active());
        draft.set(shoop_egui::DUMMY_SAMPLE_RATE, 32_000);
        draft.set(shoop_egui::DUMMY_BUFFER_SIZE, 128);
        let config =
            shoop_egui::audio_driver_config_from_draft(&draft, shoop_egui::AudioDriverKind::Dummy)
                .unwrap();
        app.handle_settings_action(SettingsAction::RequestAudioDriverSwitch { config, draft });
        let deadline = Instant::now() + Duration::from_secs(10);
        let request_id = loop {
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            if snapshot.audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::AwaitingConfirmation
            {
                break snapshot.audio_drivers.switch.request_id;
            }
            assert!(Instant::now() < deadline, "driver preflight timed out");
            thread::sleep(Duration::from_millis(5));
        };
        app.runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.settings.poll();
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            app.reconcile_audio_settings(&snapshot);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Failed
            {
                break;
            }
            assert!(Instant::now() < deadline, "injected save failure timed out");
            thread::sleep(Duration::from_millis(5));
        }
        let failed = app.runtime.snapshot();
        assert_eq!(
            failed.audio_drivers.active.as_ref().unwrap().sample_rate,
            32_000
        );
        assert!(failed.audio_drivers.switch.persistence_retry_available);
        assert_eq!(failed.audio_drivers.switch.request_id, request_id);

        std::fs::remove_file(&blocker).unwrap();
        app.handle_settings_action(SettingsAction::RetryAudioDriverPersistence { request_id });
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.settings.poll();
            app.runtime.tick(Duration::ZERO);
            let snapshot = app.runtime.snapshot();
            app.reconcile_audio_settings(&snapshot);
            if app.runtime.snapshot().audio_drivers.switch.status
                == shoop_egui::AudioDriverSwitchStatus::Completed
            {
                break;
            }
            if Instant::now() >= deadline {
                let view = app.settings.view();
                let current = app.runtime.snapshot();
                let pending = app
                    .pending_audio_settings
                    .as_ref()
                    .map(|pending| (pending.request_id, pending.saving));
                panic!(
                    "driver save retry timed out: persistence={:?}, switch={:?}, pending={pending:?}",
                    view.persistence, current.audio_drivers.switch.status
                );
            }
            thread::sleep(Duration::from_millis(5));
        }
        let completed = app.runtime.snapshot();
        assert_eq!(completed.audio_drivers.switch.request_id, request_id);
        assert_eq!(
            completed.audio_drivers.active.as_ref().unwrap().sample_rate,
            32_000
        );
        assert_eq!(
            app.settings
                .active()
                .get(shoop_egui::DUMMY_SAMPLE_RATE)
                .unwrap(),
            32_000
        );
    }

    fn native_preview_buttons_share_cancellation_and_ignore_stale_results() {
        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
        let wait = |app: &mut UnifiedApp, predicate: &dyn Fn(&shoop_egui::AppSnapshot) -> bool| {
            let start = Instant::now();
            loop {
                app.runtime.process_audio_previews();
                let snapshot = app.runtime.snapshot();
                if predicate(&snapshot) {
                    break snapshot;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "preview timed out: {:?}",
                    snapshot.click_track
                );
                thread::sleep(Duration::from_millis(5));
            }
        };
        let initial = wait(&mut app, &|snapshot| {
            snapshot.status.sample_rate > 0 && !snapshot.tracks.is_empty()
        });
        let before = initial.tracks[0].loops[0].clone();
        let loop_id = before.id;
        let request = shoop_egui::ClickTrackRequest {
            bpm: 600.0,
            click_count: 1,
            ..Default::default()
        };
        app.runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request: request.clone(),
                output: shoop_egui::ClickTrackPreviewOutput::MasterBus,
            })
            .unwrap();
        let done = wait(&mut app, &|snapshot| {
            snapshot.click_track.preview_status == shoop_egui::ClickTrackPreviewStatus::Completed
        });
        assert_eq!(done.tracks[0].loops[0].length_frames, before.length_frames);
        assert_eq!(done.tracks[0].loops[0].empty, before.empty);
        let long = shoop_egui::ClickTrackRequest {
            bpm: 60.0,
            click_count: 8,
            ..Default::default()
        };
        app.runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request: long.clone(),
                output: shoop_egui::ClickTrackPreviewOutput::MasterBus,
            })
            .unwrap();
        let playing = wait(&mut app, &|snapshot| {
            snapshot.click_track.preview_status == shoop_egui::ClickTrackPreviewStatus::Playing
        });
        let first_id = playing.click_track.preview_request_id;
        app.runtime
            .dispatch(AppIntent::StopClickTrackPreview)
            .unwrap();
        wait(&mut app, &|snapshot| {
            snapshot.click_track.preview_status == shoop_egui::ClickTrackPreviewStatus::Cancelled
        });
        app.runtime
            .dispatch(AppIntent::CompleteClickTrackPreview {
                request_id: first_id,
                success: true,
                message: "late completion".to_owned(),
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::StartMasterClickTrackPreview {
                request_id: first_id,
                sample_rate: initial.status.sample_rate,
                samples: Arc::from([0.5; 64]),
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request,
                output: shoop_egui::ClickTrackPreviewOutput::System,
            })
            .unwrap();
        let system = wait(&mut app, &|snapshot| {
            snapshot.click_track.preview_request_id > first_id
                && matches!(
                    snapshot.click_track.preview_status,
                    shoop_egui::ClickTrackPreviewStatus::Completed
                        | shoop_egui::ClickTrackPreviewStatus::Failed
                )
        });
        let system_id = system.click_track.preview_request_id;
        app.runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request: long,
                output: shoop_egui::ClickTrackPreviewOutput::MasterBus,
            })
            .unwrap();
        wait(&mut app, &|snapshot| {
            snapshot.click_track.preview_request_id > system_id
                && snapshot.click_track.preview_status
                    == shoop_egui::ClickTrackPreviewStatus::Playing
        });
        app.runtime.dispatch(AppIntent::RequestNewSession).unwrap();
        wait(&mut app, &|snapshot| {
            snapshot.click_track.preview_status == shoop_egui::ClickTrackPreviewStatus::Cancelled
        });
    }

    fn native_dummy_workflow_creates_records_and_controls_tracks_and_loops() {
        let mut app = UnifiedApp::new(1.0, false, &AppArgs::default()).unwrap();
        let track_specs = [
            ("Native stereo + MIDI", 2, true),
            ("Native mono", 1, false),
            ("Native MIDI", 0, true),
            ("Native disabled", 0, false),
            ("Native custom", 4, false),
        ];
        for (name, audio_channels, midi) in track_specs {
            app.runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels,
                    midi,
                }))
                .unwrap();
        }
        let started = Instant::now();
        let snapshot = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.tracks.len() == track_specs.len() + 1 {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        assert!(snapshot.tracks[1..]
            .iter()
            .all(|track| track.loops.len() == 8));
        assert!(snapshot.tracks[1].controls.output_stereo);
        assert!(!snapshot.tracks[3].controls.has_output_audio);
        assert!(!snapshot.tracks[4].controls.has_output);

        let snapshot = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.connections.application_ports.iter().any(|port| {
                matches!(
                    port.owner,
                    ApplicationPortOwner::Track { track_id, .. }
                        if track_id == snapshot.tracks[1].id
                ) && port.role == PortRole::MidiInput
                    && snapshot.connections.host_ports.iter().any(|host| {
                        host.data_type == port.data_type && host.direction != port.direction
                    })
            }) {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        let connection_targets: Vec<_> = [
            (
                snapshot.tracks[0].id,
                PortRole::AudioInput,
                "system:capture_1",
            ),
            (
                snapshot.tracks[1].id,
                PortRole::AudioInput,
                "system:capture_2",
            ),
            (
                snapshot.tracks[1].id,
                PortRole::MidiInput,
                "controller:midi_out",
            ),
        ]
        .into_iter()
        .map(|(track_id, role, endpoint)| {
            let port_id = snapshot
                .connections
                .application_ports
                .iter()
                .find(|port| {
                    matches!(
                        port.owner,
                        ApplicationPortOwner::Track { track_id: owner, .. }
                            if owner == track_id
                    ) && port.role == role
                })
                .unwrap()
                .id;
            (port_id, endpoint)
        })
        .collect();
        for (port_id, endpoint) in &connection_targets {
            app.runtime
                .dispatch(AppIntent::SetPortConnected {
                    port_id: *port_id,
                    host_port_id: HostPortId::new(*endpoint),
                    connected: true,
                })
                .unwrap();
        }
        let started_connections = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            let all_connected = connection_targets.iter().all(|(port_id, endpoint)| {
                snapshot.connections.confirmed_links.iter().any(|link| {
                    link.application_port_id == *port_id && link.host_port_id.as_str() == *endpoint
                }) && !snapshot.connections.pending_links.iter().any(|link| {
                    link.application_port_id == *port_id && link.host_port_id.as_str() == *endpoint
                })
            });
            if all_connected {
                break;
            }
            assert!(started_connections.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
        app.runtime
            .dispatch(AppIntent::SetPortConnected {
                port_id: connection_targets[1].0,
                host_port_id: HostPortId::new(connection_targets[1].1),
                connected: false,
            })
            .unwrap();

        let track_id = snapshot.tracks[1].id;
        let loop_id = snapshot.tracks[1].loops[0].id;
        app.runtime
            .dispatch(AppIntent::Global(shoop_egui::GlobalControlAction::SetSync(
                false,
            )))
            .unwrap();
        app.runtime
            .dispatch(AppIntent::Track {
                track_id,
                action: TrackAction::OutputGainChanged(-3.0),
            })
            .unwrap();
        for monitored_track in [snapshot.tracks[1].id, snapshot.tracks[3].id] {
            app.runtime
                .dispatch(AppIntent::Track {
                    track_id: monitored_track,
                    action: TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                })
                .unwrap();
        }
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::BalanceChanged(0.25),
            })
            .unwrap();
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        let started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.tracks[1].controls.output_gain_db == -3.0
                && snapshot.tracks[1].loops[0].balance == 0.25
                && snapshot.tracks[1].loops[0].mode == LoopMode::Recording
                && snapshot.details.is_some()
            {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }

        let piano_note = MidiNote::new(65).unwrap();
        app.runtime
            .dispatch(AppIntent::Piano(PianoAction::Press(piano_note)))
            .unwrap();
        thread::sleep(Duration::from_millis(20));
        app.runtime
            .dispatch(AppIntent::Piano(PianoAction::Release(piano_note)))
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        wait_for_loop_mode(&app, track_id, loop_id, LoopMode::Stopped);
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        wait_for_loop_mode(&app, track_id, loop_id, LoopMode::Playing);
        let frames_before_save = app.runtime.snapshot().status.processed_frames;
        app.runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        let started = Instant::now();
        let output = loop {
            if let Some(output) = app.runtime.take_file_output() {
                break output;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        };
        let started = Instant::now();
        let after_save = loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.status.processed_frames > frames_before_save {
                break snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        };
        let saved_loop = after_save
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.loops.iter().find(|loop_| loop_.id == loop_id))
            .unwrap();
        assert_eq!(saved_loop.mode, LoopMode::Playing);
        assert!(output.suggested_name.ends_with(".shoop"));
        let saved = shoop_session::decode_session(&output.bytes).unwrap();
        let saved_piano_loop = saved
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.id == loop_id.raw())
            .unwrap();
        let recorded_midi = saved_piano_loop
            .channels
            .iter()
            .filter_map(|channel| channel.media_id.as_ref())
            .find_map(|id| match &saved.media[id] {
                shoop_session::MediaPayload::Midi(midi) => Some(midi),
                shoop_session::MediaPayload::Audio(_) => None,
            })
            .unwrap();
        assert!(recorded_midi
            .events
            .iter()
            .any(|event| event.data == [0x90, 65, 100]));
        assert!(recorded_midi
            .events
            .iter()
            .any(|event| event.data == [0x80, 65, 0]));

        app.runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: output.suggested_name,
                bytes: output.bytes,
            })
            .unwrap();
        let started = Instant::now();
        loop {
            let loaded = app.runtime.snapshot();
            if loaded.io_task.as_ref().is_some_and(|task| {
                task.kind == shoop_egui::IoTaskKind::LoadSession
                    && task.status == shoop_egui::IoTaskStatus::Completed
            }) {
                assert_eq!(loaded.tracks.len(), track_specs.len() + 1);
                assert!(loaded
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .all(|loop_| {
                        loop_.mode == LoopMode::Stopped || loop_.mode == LoopMode::Unknown
                    }));
                assert!(loaded.tracks.iter().any(|track| {
                    track.name == "Native stereo + MIDI"
                        && (track.controls.output_gain_db + 3.0).abs() < 0.001
                }));
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }

        let loaded = app.runtime.snapshot();
        let target = loaded
            .tracks
            .iter()
            .find(|track| track.name == "Native stereo + MIDI")
            .unwrap();
        let generated_track = target.id;
        let generated_loop = target.loops[1].id;
        let mut request = shoop_egui::ClickTrackRequest::default();
        request.bpm = 600.0;
        request.click_count = 2;
        let before_preview = target.loops[1].clone();
        app.runtime
            .dispatch(AppIntent::PreviewClickTrack {
                output: shoop_egui::ClickTrackPreviewOutput::System,
                loop_id: generated_loop,
                request: request.clone(),
            })
            .unwrap();
        let preview_started = Instant::now();
        loop {
            app.runtime.process_audio_previews();
            let snapshot = app.runtime.snapshot();
            if matches!(
                snapshot.click_track.preview_status,
                shoop_egui::ClickTrackPreviewStatus::Completed
                    | shoop_egui::ClickTrackPreviewStatus::Failed
            ) {
                let after_preview = snapshot
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .find(|loop_| loop_.id == generated_loop)
                    .unwrap();
                assert_eq!(after_preview.id, before_preview.id);
                assert_eq!(after_preview.length_frames, before_preview.length_frames);
                assert_eq!(after_preview.empty, before_preview.empty);
                break;
            }
            assert!(preview_started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
        app.runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id: generated_loop,
                request: request.clone(),
            })
            .unwrap();
        wait_for_click_generation(&app, generated_loop, 9_600, None);
        let first_task = app.runtime.snapshot().io_task.as_ref().unwrap().id;
        request.kind = shoop_egui::ClickTrackKind::Midi;
        request.midi_note = 67;
        app.runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id: generated_loop,
                request,
            })
            .unwrap();
        wait_for_click_generation(&app, generated_loop, 9_600, Some(first_task));
        app.runtime
            .dispatch(AppIntent::Loop {
                track_id: generated_track,
                loop_id: generated_loop,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        wait_for_loop_mode(&app, generated_track, generated_loop, LoopMode::Playing);
        app.runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        let save_started = Instant::now();
        let generated_session = loop {
            if let Some(output) = app.runtime.take_file_output() {
                break output;
            }
            assert!(save_started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        };
        let decoded = shoop_session::decode_session(&generated_session.bytes).unwrap();
        let saved_generated = decoded
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.id == generated_loop.raw())
            .unwrap();
        assert_eq!(saved_generated.length_frames, 9_600);
        assert_eq!(
            saved_generated
                .channels
                .iter()
                .filter(|channel| channel.media_id.is_some())
                .count(),
            3
        );
        let generated_media = saved_generated
            .channels
            .iter()
            .filter_map(|channel| channel.media_id.as_ref())
            .map(|id| &decoded.media[id])
            .collect::<Vec<_>>();
        let audio = generated_media
            .iter()
            .filter_map(|payload| match payload {
                shoop_session::MediaPayload::Audio(audio) => Some(&audio.samples),
                shoop_session::MediaPayload::Midi(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(audio.len(), 2);
        assert_eq!(audio[0], audio[1]);
        assert!(audio[0].iter().any(|sample| *sample != 0.0));
        let midi = generated_media
            .iter()
            .find_map(|payload| match payload {
                shoop_session::MediaPayload::Midi(midi) => Some(midi),
                shoop_session::MediaPayload::Audio(_) => None,
            })
            .unwrap();
        assert_eq!(midi.length_frames, 9_600);
        assert_eq!(
            midi.events
                .iter()
                .map(|event| (event.frame, event.data.as_slice()))
                .collect::<Vec<_>>(),
            vec![
                (0, [0x90, 67, 127].as_slice()),
                (4_800, [0x80, 67, 127].as_slice()),
                (4_800, [0x90, 67, 127].as_slice()),
                (9_599, [0x80, 67, 127].as_slice()),
            ]
        );
        app.runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: generated_session.suggested_name,
                bytes: generated_session.bytes,
            })
            .unwrap();
        let reload_started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.io_task.as_ref().is_some_and(|task| {
                task.kind == shoop_egui::IoTaskKind::LoadSession
                    && task.status == shoop_egui::IoTaskStatus::Completed
            }) && snapshot
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .find(|loop_| loop_.id == generated_loop)
                .is_some_and(|loop_| loop_.length_frames == 9_600 && !loop_.empty)
            {
                break;
            }
            assert!(reload_started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn invalid_preview_fails_without_opening_hardware() {
        let runtime =
            crate::application::ApplicationRuntime::start(Box::new(FakeBackend::default()))
                .unwrap();
        let handle = runtime.handle();
        let started = std::time::Instant::now();
        let loop_id = loop {
            let snapshot = handle.snapshot();
            if snapshot.status.sample_rate > 0 {
                break snapshot.tracks[0].loops[0].id;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        };
        handle
            .dispatch(AppIntent::PreviewClickTrack {
                output: shoop_egui::ClickTrackPreviewOutput::System,
                loop_id,
                request: shoop_egui::ClickTrackRequest::default(),
            })
            .unwrap();
        let preview = loop {
            if let Some(preview) = handle.take_audio_preview() {
                break preview;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        };
        let request_id = preview.request_id;
        let player = NativePreviewPlayer::new(handle.clone()).unwrap();
        player.play(ApplicationAudioPreview {
            output: shoop_egui::ClickTrackPreviewOutput::System,
            request_id,
            sample_rate: 0,
            samples: Arc::from([]),
        });
        while handle.snapshot().click_track.preview_status
            != shoop_egui::ClickTrackPreviewStatus::Failed
        {
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(handle.snapshot().click_track.preview_request_id, request_id);
    }
    fn wait_for_click_generation(
        app: &UnifiedApp,
        loop_id: shoop_egui::LoopId,
        length: u64,
        previous_task: Option<shoop_egui::TaskId>,
    ) {
        let started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot.io_task.as_ref().is_some_and(|task| {
                task.kind == shoop_egui::IoTaskKind::GenerateClickTrack
                    && task.status == shoop_egui::IoTaskStatus::Completed
                    && Some(task.id) != previous_task
            }) && snapshot
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .find(|loop_| loop_.id == loop_id)
                .is_some_and(|loop_| loop_.length_frames == length && !loop_.empty)
            {
                return;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn wait_for_loop_mode(
        app: &UnifiedApp,
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        expected: LoopMode,
    ) {
        let started = Instant::now();
        loop {
            let snapshot = app.runtime.snapshot();
            if snapshot
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .and_then(|track| track.loops.iter().find(|loop_| loop_.id == loop_id))
                .is_some_and(|loop_| loop_.mode == expected)
            {
                return;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(5));
        }
    }
