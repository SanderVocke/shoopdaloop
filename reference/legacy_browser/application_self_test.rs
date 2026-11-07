#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_SCRIPT_NAME: &str = "browser-self-test-session.lua";
#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_SCRIPT_SOURCE: &str =
    "shoop_announce_api_version(1, 0); local shoop_control = require('shoop_control'); local dialog = require('shoop_dialog'); local markdown = dialog.markdown_file('help.md'); shoop_control.register_keyboard_event_cb(function(_) end)";
#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_MARKDOWN: &[u8] = b"# Browser bundle\n\n![resource](image.png)";
#[cfg(target_arch = "wasm32")]
const BROWSER_SESSION_IMAGE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[cfg(target_arch = "wasm32")]
fn browser_session_script_bundle() -> std::sync::Arc<shoop_scripting::ScriptResourceBundle> {
    use shoop_script_resources::{
        NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource, ScriptResourceBundle,
    };

    let entrypoint = NormalizedRelativePath::parse("main.lua").unwrap();
    std::sync::Arc::new(
        ScriptResourceBundle::new(
            entrypoint.clone(),
            std::collections::BTreeMap::from([
                (
                    entrypoint,
                    ScriptResource::new(
                        ResourceKind::Lua,
                        std::sync::Arc::<[u8]>::from(BROWSER_SESSION_SCRIPT_SOURCE.as_bytes()),
                    ),
                ),
                (
                    NormalizedRelativePath::parse("help.md").unwrap(),
                    ScriptResource::new(
                        ResourceKind::Markdown,
                        std::sync::Arc::<[u8]>::from(BROWSER_SESSION_MARKDOWN),
                    ),
                ),
                (
                    NormalizedRelativePath::parse("image.png").unwrap(),
                    ScriptResource::new(
                        ResourceKind::Image,
                        std::sync::Arc::<[u8]>::from(BROWSER_SESSION_IMAGE),
                    ),
                ),
            ]),
            ResourceLimits::default(),
        )
        .expect("browser self-test resources must form a valid bundle"),
    )
}

#[cfg(target_arch = "wasm32")]
fn browser_unsupported_session_bytes(
    sample_rate: u32,
    carla: bool,
) -> Result<std::sync::Arc<[u8]>, String> {
    use shoop_session::{
        encode_session, DefaultPlaybackModeDocument, FxChainDocument, FxChainTypeDocument,
        SessionBundle, SessionDocument, TrackControlsDocument, TrackDocument, TrackGroupDocument,
        TrackTopologyDocument,
    };

    let mut document = SessionDocument::empty(sample_rate);
    let (name, port_name_base, topology, fx_chain) = if carla {
        (
            "Unsupported browser Carla",
            "unsupported_browser_carla",
            TrackTopologyDocument::Carla {
                chain_type: FxChainTypeDocument::CarlaRack,
                audio_channels: 1,
                midi: true,
                dry_audio_channels: Some(1),
                wet_audio_channels: Some(1),
            },
            Some(FxChainDocument {
                id: 100,
                title: "Unavailable".to_owned(),
                chain_type: FxChainTypeDocument::CarlaRack,
                ports: Vec::new(),
                internal_state: "opaque browser rejection state".to_owned(),
                builtin_fx_midi_cc_assignments: Vec::new(),
                midi_cc_assignments: Vec::new(),
            }),
        )
    } else {
        (
            "Unsupported browser External",
            "unsupported_browser_external",
            TrackTopologyDocument::DryWetExternal {
                dry_audio_channels: 1,
                wet_audio_channels: 1,
                dry_midi: true,
            },
            None,
        )
    };
    document.track_groups.push(TrackGroupDocument {
        name: "main".to_owned(),
        tracks: vec![TrackDocument {
            id: 99,
            name: name.to_owned(),
            port_name_base: port_name_base.to_owned(),
            is_sync: false,
            width: None,
            topology,
            default_playback_mode: DefaultPlaybackModeDocument::Regular,
            controls: TrackControlsDocument::default(),
            latency: Default::default(),
            loops: Vec::new(),
            ports: Vec::new(),
            fx_chain,
        }],
    });
    encode_session(&SessionBundle::new(document), "browser-capability-test")
        .map(std::sync::Arc::from)
        .map_err(|error| format!("could not encode browser capability fixture: {error}"))
}

#[cfg(target_arch = "wasm32")]
fn set_browser_web_midi_test_status(status: &str) {
    if let Some(element) = browser_status_element() {
        let _ = element.set_attribute("data-web-midi-self-test", status);
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_click_request(kind: shoop_egui::ClickTrackKind) -> shoop_egui::ClickTrackRequest {
    shoop_egui::ClickTrackRequest {
        kind,
        bpm: 1_000.0,
        click_count: 1,
        midi_note: 67,
        ..Default::default()
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserSelfTest {
    Disabled,
    WaitForAudio,
    WaitForDryWetForm,
    WaitForWebMidi,
    WaitForWebMidiControlBeforeAudio,
    AddWebMidiTrack,
    WaitForWebMidiTrack,
    WaitForWebMidiTrackReady {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        input_port: shoop_egui::PortId,
        output_port: shoop_egui::PortId,
        tiny_input_port: shoop_egui::PortId,
        callbacks_before: u64,
    },
    WaitForWebMidiConnections {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        input_port: shoop_egui::PortId,
        output_port: shoop_egui::PortId,
        tiny_input_port: shoop_egui::PortId,
    },
    WaitForWebMidiControl {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
    },
    WaitForWebMidiRecorded {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        callbacks_after_control: Option<u64>,
    },
    WaitForWebMidiInput {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
    },
    WaitForWebMidiStopped {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
    },
    WaitForWebMidiSessionLoad {
        callbacks_before: u64,
    },
    WaitForWebMidiSave {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        callbacks_before: u64,
    },
    WaitForWebMidiPlayback {
        track_id: shoop_egui::TrackId,
        loop_id: shoop_egui::LoopId,
        callbacks_before: u64,
    },
    AddTrack,
    WaitForTrack,
    WaitForRecording,
    WaitForPianoPress {
        callbacks_before: u64,
    },
    WaitForPianoRelease {
        callbacks_before: u64,
    },
    WaitForStopped,
    WaitForDetails,
    WaitForPlaying,
    WaitForDisconnectedOutput {
        left: shoop_egui::PortId,
        right: shoop_egui::PortId,
        callbacks_before: u64,
    },
    WaitForReconnectedOutput {
        left: shoop_egui::PortId,
        right: shoop_egui::PortId,
        callbacks_before: u64,
    },
    SaveSession {
        callbacks_before: u64,
    },
    WaitForSessionSave {
        callbacks_before: u64,
    },
    WaitForSessionLoad {
        callbacks_before: u64,
    },
    SaveLoadedScriptSession {
        callbacks_before: u64,
    },
    WaitForLoadedScriptSave {
        callbacks_before: u64,
    },
    PlayLoadedLoop,
    WaitForLoadedPlayback,
    WaitForMediaStopped,
    ExportLoopAudio,
    WaitForLoopAudioSelection,
    WaitForLoopAudioExport,
    WaitForLoopAudioMapping,
    WaitForLoopAudioImport,
    ExportLoopMidi,
    WaitForLoopMidiExport,
    WaitForLoopMidiImport,
    GenerateClickAudio,
    WaitForClickAudio {
        previous_task: shoop_egui::TaskId,
        request_revision: u64,
    },
    WaitForClickAudioSelection,
    WaitForClickAudioExport,
    PreviewClickAudio,
    WaitForClickPreview,
    GenerateClickMidi,
    WaitForClickMidi {
        previous_task: shoop_egui::TaskId,
        request_revision: u64,
    },
    WaitForClickMidiExport,
    RejectProcessedSession,
    RejectExternalSession,
    WaitForProcessedSessionRejection {
        audio_progress_before: u64,
        external: bool,
    },
    AddLuaVersionChecks,
    WaitForLuaVersionChecks,
    WaitForLuaDialogs,
    WaitForLuaDialogCallback {
        script_id: shoop_egui::ScriptId,
        welcome: shoop_egui::ScriptDialogId,
        guide: shoop_egui::ScriptDialogId,
    },
    WaitForLuaDialogsStopped,
    Complete,
    Failed,
}

#[cfg(any(target_arch = "wasm32", test))]
fn click_request_was_rejected(
    snapshot_revision: u64,
    io_task: Option<&shoop_egui::IoTaskState>,
    previous_task: shoop_egui::TaskId,
    request_revision: u64,
) -> bool {
    snapshot_revision > request_revision
        && io_task.is_none_or(|task| {
            task.id == previous_task || task.kind != shoop_egui::IoTaskKind::GenerateClickTrack
        })
}

#[cfg(any(target_arch = "wasm32", test))]
fn lua_version_rejection_is_expected(script: &shoop_egui::ScriptState) -> bool {
    let lifecycle = if script.name == "lua-api-unannounced.lua" {
        shoop_egui::ScriptLifecycle::Error
    } else {
        shoop_egui::ScriptLifecycle::Incompatible
    };
    script.lifecycle == lifecycle && script.latest_error.is_some()
}

#[cfg(target_arch = "wasm32")]
impl BrowserSelfTest {
    fn from_args(args: &AppArgs) -> Self {
        if args.web_midi_test {
            set_browser_self_test_status("web-midi");
            set_browser_web_midi_test_status("awaiting-permission");
            Self::WaitForWebMidi
        } else if args.self_test {
            set_browser_self_test_status("awaiting-audio");
            Self::WaitForAudio
        } else {
            Self::Disabled
        }
    }

    fn update(
        &mut self,
        runtime: &mut Runtime,
        snapshot: &AppSnapshot,
        widget: &mut AppWidget,
        args: &AppArgs,
    ) {
        let result = match *self {
            Self::Disabled | Self::Complete | Self::Failed => return,
            Self::WaitForAudio => {
                if !runtime.audio_running() {
                    return;
                }
                if !widget.browser_test_open_builtin_synth_form(&snapshot.track_processors) {
                    return self.fail(
                        "browser dry/wet form did not expose the Built-in Synth processor contract",
                    );
                }
                mark_browser_dry_wet_capability_check();
                Ok(Self::WaitForDryWetForm)
            }
            Self::WaitForDryWetForm => {
                let builtin_fx = snapshot.track_processors.iter().find(|processor| {
                    processor.id.as_str() == shoop_egui::TrackProcessorTypeId::BUILTIN_FX
                });
                let builtin_synth = snapshot.track_processors.iter().find(|processor| {
                    processor.id.as_str() == shoop_egui::TrackProcessorTypeId::OXISYNTH
                });
                if snapshot.track_processors.len() != 2
                    || !builtin_fx.is_some_and(|processor| {
                        [1, 2, 3, 6]
                            .into_iter()
                            .all(|channels| processor.constraints.accepts(channels, channels, true))
                            && !processor.constraints.accepts(2, 2, false)
                            && !processor.constraints.accepts(2, 1, true)
                            && processor.features.state
                            && processor.features.embedded_ui
                    })
                    || builtin_synth.is_none()
                {
                    return self.fail("browser built-in processor catalog changed unexpectedly");
                }
                widget.browser_test_close_add_track();
                widget.browser_test_open_global_connections();
                Ok(Self::AddTrack)
            }
            Self::WaitForWebMidi => {
                if runtime.midi.state() != browser_midi::BrowserMidiState::Running {
                    return;
                }
                let Some(apc) = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
                else {
                    return self.fail("bundled APC script is missing");
                };
                runtime
                    .dispatch(AppIntent::SetScriptEnabled {
                        script_id: apc.id,
                        enabled: true,
                    })
                    .map(|()| Self::WaitForWebMidiControlBeforeAudio)
            }
            Self::WaitForWebMidiControlBeforeAudio => {
                let ready = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
                    .is_some_and(|script| {
                        script.lifecycle == shoop_egui::ScriptLifecycle::Listening
                            && script.midi.connections == 2
                    });
                if !ready {
                    return;
                }
                set_browser_web_midi_test_status("control-ready-without-audio");
                if !runtime.audio_running() {
                    return;
                }
                widget.browser_test_open_global_connections();
                Ok(Self::AddWebMidiTrack)
            }
            Self::AddWebMidiTrack => runtime
                .dispatch(AppIntent::Global(shoop_egui::GlobalControlAction::SetSync(
                    false,
                )))
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrack(shoop_egui::DirectTrackSpec {
                        name: "Browser Web MIDI test".to_owned(),
                        audio_channels: 0,
                        midi: true,
                    }))
                })
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrackWithTopology(shoop_egui::TrackSpec {
                        name: "Browser Web MIDI Tiny".to_owned(),
                        topology: shoop_egui::TrackSpecTopology::DryWet {
                            dry_audio_channels: 0,
                            wet_audio_channels: 0,
                            dry_midi: true,
                            processor_type: shoop_egui::TrackProcessorTypeId::new(
                                shoop_egui::TrackProcessorTypeId::OXISYNTH,
                            ),
                            default_playback_mode: shoop_egui::DefaultPlaybackMode::Regular,
                        },
                        latency: shoop_egui::TrackLatencySpec::default(),
                        initial_output_bus_names: Vec::new(),
                        creation_request_id: None,
                    }))
                })
                .map(|()| Self::WaitForWebMidiTrack),
            Self::WaitForWebMidiTrack => {
                let Some(track) = snapshot.tracks.iter().find(|track| !track.is_sync) else {
                    return;
                };
                let Some(loop_state) = track.loops.first() else {
                    return;
                };
                let input_port = snapshot.connections.application_ports.iter().find(|port| {
                    matches!(
                        port.owner,
                        shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                            if track_id == track.id
                    ) && port.role == shoop_egui::PortRole::MidiInput
                });
                let output_port = snapshot.connections.application_ports.iter().find(|port| {
                    matches!(
                        port.owner,
                        shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                            if track_id == track.id
                    ) && port.role == shoop_egui::PortRole::MidiOutput
                });
                let (Some(input_port), Some(output_port)) = (input_port, output_port) else {
                    return;
                };
                let Some(tiny) = snapshot.tracks.iter().find(|candidate| {
                    candidate.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
                    })
                }) else {
                    return;
                };
                let Some(tiny_midi_input) =
                    snapshot.connections.application_ports.iter().find(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == tiny.id
                        ) && port.role == shoop_egui::PortRole::MidiInput
                    })
                else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: tiny_midi_input.id,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Track {
                            track_id: tiny.id,
                            action: shoop_egui::TrackAction::InputMonitoringChanged {
                                enabled: true,
                                respect_auto_mute: false,
                            },
                        })
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Track {
                            track_id: tiny.id,
                            action: shoop_egui::TrackAction::OxiSynth(
                                shoop_egui::OxiSynthControl::SelectPreset("0:40".to_owned()),
                            ),
                        })
                    })
                    .map(|()| Self::WaitForWebMidiTrackReady {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        input_port: input_port.id,
                        output_port: output_port.id,
                        tiny_input_port: tiny_midi_input.id,
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForWebMidiTrackReady {
                track_id,
                loop_id,
                input_port,
                output_port,
                tiny_input_port,
                callbacks_before,
            } => {
                if snapshot.status.callback_count <= callbacks_before.saturating_add(10) {
                    return;
                }
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: input_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::SetPortConnected {
                            port_id: output_port,
                            host_port_id: shoop_egui::HostPortId::new("webmidi:sink:test-output"),
                            connected: true,
                        })
                    })
                    .map(|()| Self::WaitForWebMidiConnections {
                        track_id,
                        loop_id,
                        input_port,
                        output_port,
                        tiny_input_port,
                    })
            }
            Self::WaitForWebMidiConnections {
                track_id,
                loop_id,
                input_port,
                output_port,
                tiny_input_port,
            } => {
                let track_input_connected =
                    snapshot.connections.confirmed_links.iter().any(|link| {
                        link.application_port_id == input_port
                            && link.host_port_id.as_str() == "webmidi:source:test-input"
                    });
                let track_output_connected =
                    snapshot.connections.confirmed_links.iter().any(|link| {
                        link.application_port_id == output_port
                            && link.host_port_id.as_str() == "webmidi:sink:test-output"
                    });
                let tiny_input_connected =
                    snapshot.connections.confirmed_links.iter().any(|link| {
                        link.application_port_id == tiny_input_port
                            && link.host_port_id.as_str() == "webmidi:source:test-input"
                    });
                let Some(apc) = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == APC_MINI_SCRIPT_FILENAME)
                else {
                    return;
                };
                let control_ports = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::LuaControl { script_id, .. }
                                if script_id == apc.id
                        )
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let control_links = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| control_ports.contains(&link.application_port_id))
                    .count();
                if !track_input_connected {
                    let _ = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: input_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    });
                }
                if !track_output_connected {
                    let _ = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: output_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:sink:test-output"),
                        connected: true,
                    });
                }
                if !tiny_input_connected {
                    let _ = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: tiny_input_port,
                        host_port_id: shoop_egui::HostPortId::new("webmidi:source:test-input"),
                        connected: true,
                    });
                }
                if !track_input_connected
                    || !track_output_connected
                    || !tiny_input_connected
                    || apc.lifecycle != shoop_egui::ScriptLifecycle::Listening
                    || control_ports.len() != 2
                    || control_links != 2
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Track {
                        track_id,
                        action: shoop_egui::TrackAction::InputMonitoringChanged {
                            enabled: true,
                            respect_auto_mute: false,
                        },
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id,
                            loop_id,
                            action: shoop_egui::LoopAction::IconClicked(Default::default()),
                        })
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id,
                            loop_id,
                            action: shoop_egui::LoopAction::RecordClicked,
                        })
                    })
                    .map(|()| Self::WaitForWebMidiControl { track_id, loop_id })
            }
            Self::WaitForWebMidiControl { track_id, loop_id } => {
                let Some(track) = snapshot.tracks.iter().find(|track| track.id == track_id) else {
                    return;
                };
                let Some(loop_state) = track.loops.iter().find(|loop_| loop_.id == loop_id) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Recording {
                    return;
                }
                set_browser_web_midi_test_status("awaiting-input");
                Ok(Self::WaitForWebMidiRecorded {
                    track_id,
                    loop_id,
                    callbacks_after_control: None,
                })
            }
            Self::WaitForWebMidiRecorded {
                track_id,
                loop_id,
                callbacks_after_control,
            } => {
                if !snapshot.global_controls.solo {
                    return;
                }
                if let Some(callbacks_after_control) = callbacks_after_control {
                    if snapshot.status.callback_count <= callbacks_after_control.saturating_add(100)
                    {
                        return;
                    }
                    runtime
                        .dispatch(AppIntent::Loop {
                            track_id,
                            loop_id,
                            action: shoop_egui::LoopAction::StopClicked,
                        })
                        .map(|()| Self::WaitForWebMidiInput { track_id, loop_id })
                } else {
                    Ok(Self::WaitForWebMidiRecorded {
                        track_id,
                        loop_id,
                        callbacks_after_control: Some(snapshot.status.callback_count),
                    })
                }
            }
            Self::WaitForWebMidiInput { track_id, loop_id } => {
                let Some(track) = snapshot.tracks.iter().find(|track| track.id == track_id) else {
                    return;
                };
                let Some(loop_state) = track.loops.iter().find(|loop_| loop_.id == loop_id) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Stopped {
                    return;
                }
                runtime
                    .dispatch(AppIntent::RequestSaveSession)
                    .map(|()| Self::WaitForWebMidiStopped { track_id, loop_id })
            }
            Self::WaitForWebMidiStopped {
                track_id: _,
                loop_id: _,
            } => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let bundle = match shoop_session::decode_session(&output.bytes) {
                    Ok(bundle) => bundle,
                    Err(error) => {
                        return self.fail(&format!("could not decode Web MIDI session: {error}"));
                    }
                };
                let recorded = bundle.media.values().any(|payload| {
                    matches!(
                        payload,
                        shoop_session::MediaPayload::Midi(midi)
                            if midi.events.iter().any(|event| event.data == [0x90, 83, 0x7f])
                    )
                });
                let routes = bundle
                    .document
                    .track_groups
                    .iter()
                    .flat_map(|group| &group.tracks)
                    .flat_map(|track| {
                        track.ports.iter().flat_map(|port| {
                            port.external_connections
                                .iter()
                                .filter(|endpoint| endpoint.starts_with("webmidi:"))
                                .map(|endpoint| {
                                    format!("{}:{:?}:{endpoint}", track.name, port.role)
                                })
                        })
                    })
                    .collect::<Vec<_>>();
                if !recorded || routes.len() != 3 {
                    return self.fail(&format!(
                        "Web MIDI recording or persisted routes are missing: recorded={recorded}, routes={routes:?}"
                    ));
                }
                runtime
                    .dispatch(AppIntent::LoadSessionBytes {
                        name: output.suggested_name,
                        bytes: output.bytes,
                    })
                    .map(|()| Self::WaitForWebMidiSessionLoad {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForWebMidiSessionLoad { callbacks_before } => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::LoadSession
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                let Some(track) = snapshot
                    .tracks
                    .iter()
                    .find(|track| track.name == "Browser Web MIDI test")
                else {
                    return self.fail("loaded Web MIDI session lost its track");
                };
                let Some(loop_state) = track.loops.first() else {
                    return self.fail("loaded Web MIDI session lost its loop");
                };
                let track_ports = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == track.id
                        )
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let restored_routes = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| {
                        track_ports.contains(&link.application_port_id)
                            && link.host_port_id.as_str().starts_with("webmidi:")
                    })
                    .count();
                if restored_routes < 2 {
                    return;
                }
                if restored_routes > 2 {
                    return self
                        .fail("loaded Web MIDI session restored duplicate direct-track routes");
                }
                let all_track_ports = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(port.owner, shoop_egui::ApplicationPortOwner::Track { .. })
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let all_restored_routes = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| {
                        all_track_ports.contains(&link.application_port_id)
                            && link.host_port_id.as_str().starts_with("webmidi:")
                    })
                    .count();
                if all_restored_routes < 3 {
                    return;
                }
                if all_restored_routes > 3 {
                    return self.fail("loaded Web MIDI session restored duplicate track routes");
                }
                if snapshot.status.callback_count <= callbacks_before {
                    return;
                }
                set_browser_web_midi_test_status("ready-for-playback");
                Ok(Self::WaitForWebMidiSave {
                    track_id: track.id,
                    loop_id: loop_state.id,
                    callbacks_before: snapshot.status.callback_count,
                })
            }
            Self::WaitForWebMidiSave {
                track_id,
                loop_id,
                callbacks_before: _,
            } => {
                let ready = browser_status_element().is_some_and(|element| {
                    element
                        .get_attribute("data-web-midi-playback-ready")
                        .as_deref()
                        == Some("true")
                });
                if !ready {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id,
                        loop_id,
                        action: shoop_egui::LoopAction::PlayClicked,
                    })
                    .map(|()| {
                        set_browser_web_midi_test_status("awaiting-playback-output");
                        Self::WaitForWebMidiPlayback {
                            track_id,
                            loop_id,
                            callbacks_before: snapshot.status.callback_count,
                        }
                    })
            }
            Self::WaitForWebMidiPlayback {
                track_id,
                loop_id,
                callbacks_before,
            } => {
                let playing = snapshot
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .and_then(|track| track.loops.iter().find(|loop_| loop_.id == loop_id))
                    .is_some_and(|loop_| loop_.mode == shoop_egui::LoopMode::Playing);
                if !playing
                    || snapshot.status.callback_count <= callbacks_before.saturating_add(100)
                {
                    return;
                }
                set_browser_web_midi_test_status("passed");
                mark_browser_self_test_nonzero_io();
                Ok(Self::Complete)
            }
            Self::AddTrack => runtime
                .dispatch(AppIntent::Global(shoop_egui::GlobalControlAction::SetSync(
                    false,
                )))
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrack(shoop_egui::DirectTrackSpec {
                        name: "Browser self-test stereo".to_owned(),
                        audio_channels: 2,
                        midi: true,
                    }))
                })
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrack(shoop_egui::DirectTrackSpec {
                        name: "Browser self-test mono".to_owned(),
                        audio_channels: 1,
                        midi: false,
                    }))
                })
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrackWithTopology(shoop_egui::TrackSpec {
                        name: "Browser Built-in FX".to_owned(),
                        topology: shoop_egui::TrackSpecTopology::DryWet {
                            dry_audio_channels: 3,
                            wet_audio_channels: 3,
                            dry_midi: true,
                            processor_type: shoop_egui::TrackProcessorTypeId::new(
                                shoop_egui::TrackProcessorTypeId::BUILTIN_FX,
                            ),
                            default_playback_mode: shoop_egui::DefaultPlaybackMode::Regular,
                        },
                        latency: shoop_egui::TrackLatencySpec::default(),
                        initial_output_bus_names: Vec::new(),
                        creation_request_id: None,
                    }))
                })
                .and_then(|()| {
                    runtime.dispatch(AppIntent::AddTrackWithTopology(shoop_egui::TrackSpec {
                        name: "Browser Built-in Synth".to_owned(),
                        topology: shoop_egui::TrackSpecTopology::DryWet {
                            dry_audio_channels: 2,
                            wet_audio_channels: 2,
                            dry_midi: true,
                            processor_type: shoop_egui::TrackProcessorTypeId::new(
                                shoop_egui::TrackProcessorTypeId::OXISYNTH,
                            ),
                            default_playback_mode: shoop_egui::DefaultPlaybackMode::Regular,
                        },
                        latency: shoop_egui::TrackLatencySpec::default(),
                        initial_output_bus_names: Vec::new(),
                        creation_request_id: None,
                    }))
                })
                .map(|()| Self::WaitForTrack),
            Self::WaitForTrack => {
                let Some(track) = snapshot.tracks.iter().find(|track| !track.is_sync) else {
                    return;
                };
                let Some(loop_state) = track.loops.first() else {
                    return;
                };
                let Some(builtin_fx) = snapshot.tracks.iter().find(|track| {
                    track.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::BUILTIN_FX
                    })
                }) else {
                    return;
                };
                let builtin_fx_controls = [
                    shoop_egui::TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                    shoop_egui::TrackAction::BuiltInFx(
                        shoop_egui::BuiltInFxControl::SetReverbEnabled(false),
                    ),
                    shoop_egui::TrackAction::BuiltInFx(
                        shoop_egui::BuiltInFxControl::SetStageEnabled(
                            shoop_egui::BuiltInFxStage::Drive,
                            true,
                        ),
                    ),
                    shoop_egui::TrackAction::BuiltInFx(
                        shoop_egui::BuiltInFxControl::SetDriveType(
                            shoop_egui::BuiltInFxDriveType::Fuzz,
                        ),
                    ),
                    shoop_egui::TrackAction::BuiltInFx(
                        shoop_egui::BuiltInFxControl::SetParameter(
                            shoop_egui::BuiltInFxParameter::Drive,
                            24.0,
                        ),
                    ),
                    shoop_egui::TrackAction::BuiltInFx(
                        shoop_egui::BuiltInFxControl::AssignMidiCc(
                            shoop_egui::BuiltInFxMidiCcAssignment {
                                parameter: shoop_egui::BuiltInFxParameter::Drive,
                                channel: 2,
                                controller: 17,
                            },
                        ),
                    ),
                    shoop_egui::TrackAction::FxVisibilityChanged(true),
                ]
                .into_iter()
                .try_for_each(|action| {
                    runtime.dispatch(AppIntent::Track {
                        track_id: builtin_fx.id,
                        action,
                    })
                });
                let Some(tiny) = snapshot.tracks.iter().find(|track| {
                    track.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
                    })
                }) else {
                    return;
                };
                let tiny_controls = [
                    shoop_egui::TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                    shoop_egui::TrackAction::OxiSynth(
                        shoop_egui::OxiSynthControl::SelectPreset("0:40".to_owned()),
                    ),
                    shoop_egui::TrackAction::OxiSynth(
                        shoop_egui::OxiSynthControl::SetReverbSend(0.4),
                    ),
                    shoop_egui::TrackAction::OxiSynth(
                        shoop_egui::OxiSynthControl::SetChorusSend(0.6),
                    ),
                    shoop_egui::TrackAction::OxiSynth(shoop_egui::OxiSynthControl::Panic),
                    shoop_egui::TrackAction::FxVisibilityChanged(true),
                    shoop_egui::TrackAction::FxVisibilityChanged(false),
                    shoop_egui::TrackAction::FxVisibilityChanged(true),
                ]
                .into_iter()
                .try_for_each(|action| {
                    runtime.dispatch(AppIntent::Track {
                        track_id: tiny.id,
                        action,
                    })
                });
                let tiny_audio_inputs = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == tiny.id
                        ) && port.role == shoop_egui::PortRole::AudioInput
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                let tiny_audio_links = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .filter(|link| tiny_audio_inputs.contains(&link.application_port_id))
                    .map(|link| (link.application_port_id, link.host_port_id.clone()))
                    .collect::<Vec<_>>();
                builtin_fx_controls
                    .and_then(|()| tiny_controls)
                    .and_then(|()| {
                        tiny_audio_links
                            .into_iter()
                            .try_for_each(|(port_id, host_port_id)| {
                                runtime.dispatch(AppIntent::SetPortConnected {
                                    port_id,
                                    host_port_id,
                                    connected: false,
                                })
                            })
                    })
                    .and_then(|()| {
                        if snapshot.status.audio_driver == shoop_egui::AudioDriverState::Dummy {
                            Ok(Self::SaveSession {
                                callbacks_before: snapshot.status.callback_count,
                            })
                        } else {
                            runtime
                                .dispatch(AppIntent::Track {
                                    track_id: track.id,
                                    action: shoop_egui::TrackAction::InputMonitoringChanged {
                                        enabled: true,
                                        respect_auto_mute: false,
                                    },
                                })
                                .and_then(|()| {
                                    runtime.dispatch(AppIntent::Loop {
                                        track_id: track.id,
                                        loop_id: loop_state.id,
                                        action: shoop_egui::LoopAction::IconClicked(
                                            Default::default(),
                                        ),
                                    })
                                })
                                .and_then(|()| {
                                    runtime.dispatch(AppIntent::Loop {
                                        track_id: track.id,
                                        loop_id: loop_state.id,
                                        action: shoop_egui::LoopAction::RecordClicked,
                                    })
                                })
                                .map(|()| Self::WaitForRecording)
                        }
                    })
            }
            Self::WaitForRecording => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Recording
                    || loop_state.empty
                    || args.stress && snapshot.status.callback_count < 1_500
                {
                    return;
                }
                let Some(tiny) = snapshot.tracks.iter().find(|candidate| {
                    candidate.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
                    })
                }) else {
                    return;
                };
                let tiny_audio_inputs = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == tiny.id
                        ) && port.role == shoop_egui::PortRole::AudioInput
                    })
                    .map(|port| port.id)
                    .collect::<Vec<_>>();
                if let Some(link) = snapshot
                    .connections
                    .confirmed_links
                    .iter()
                    .find(|link| tiny_audio_inputs.contains(&link.application_port_id))
                {
                    if let Err(error) = runtime.dispatch(AppIntent::SetPortConnected {
                        port_id: link.application_port_id,
                        host_port_id: link.host_port_id.clone(),
                        connected: false,
                    }) {
                        return self.fail(&format!(
                            "could not isolate browser Built-in Synth audio input: {error}"
                        ));
                    }
                    return;
                }
                runtime
                    .dispatch(AppIntent::Track {
                        track_id: track.id,
                        action: shoop_egui::TrackAction::OutputMuteChanged(true),
                    })
                    .and_then(|()| {
                        (48..=65).try_for_each(|note| {
                            runtime.dispatch(AppIntent::Piano(shoop_egui::PianoAction::Press(
                                shoop_egui::MidiNote::new(note).unwrap(),
                            )))
                        })
                    })
                    .map(|()| Self::WaitForPianoPress {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForPianoPress { callbacks_before } => {
                if snapshot.status.callback_count <= callbacks_before
                    || snapshot.status.output_peak <= 0.000_001
                {
                    return;
                }
                (48..=65)
                    .try_for_each(|note| {
                        runtime.dispatch(AppIntent::Piano(shoop_egui::PianoAction::Release(
                            shoop_egui::MidiNote::new(note).unwrap(),
                        )))
                    })
                    .map(|()| Self::WaitForPianoRelease {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForPianoRelease { callbacks_before } => {
                if snapshot.status.callback_count <= callbacks_before {
                    return;
                }
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::Track {
                        track_id: track.id,
                        action: shoop_egui::TrackAction::OutputMuteChanged(false),
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id: track.id,
                            loop_id: loop_state.id,
                            action: shoop_egui::LoopAction::StopClicked,
                        })
                    })
                    .map(|()| Self::WaitForStopped)
            }
            Self::WaitForStopped => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Stopped {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        action: shoop_egui::LoopAction::IconClicked(Default::default()),
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::Loop {
                            track_id: track.id,
                            loop_id: loop_state.id,
                            action: shoop_egui::LoopAction::IconClicked(Default::default()),
                        })
                    })
                    .map(|()| Self::WaitForDetails)
            }
            Self::WaitForDetails => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                let waveform_ready = snapshot.details.as_ref().is_some_and(|details| {
                    details.loop_id == loop_state.id
                        && details.channels.first().is_some_and(|channel| {
                            !channel.samples.is_empty()
                                && channel
                                    .samples
                                    .iter()
                                    .any(|sample| sample.abs() > 0.000_001)
                        })
                });
                if !waveform_ready {
                    return;
                }
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        action: shoop_egui::LoopAction::PlayClicked,
                    })
                    .map(|()| Self::WaitForPlaying)
            }
            Self::WaitForPlaying => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Playing
                    || snapshot.status.output_peak <= 0.000_001
                    || snapshot.status.callback_count == 0
                    || snapshot
                        .details
                        .as_ref()
                        .is_none_or(|details| details.loop_id != loop_state.id)
                {
                    return;
                }
                mark_browser_self_test_nonzero_io();
                let mut outputs: Vec<_> = snapshot
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            shoop_egui::ApplicationPortOwner::Track { track_id, .. }
                                if track_id == track.id
                        ) && port.role == shoop_egui::PortRole::AudioOutput
                    })
                    .map(|port| port.id)
                    .collect();
                outputs.sort();
                if outputs.len() < 2 {
                    return self.fail("browser stereo route ports are missing");
                }
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: outputs[0],
                        host_port_id: shoop_egui::HostPortId::new("webaudio:destination_1"),
                        connected: false,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::SetPortConnected {
                            port_id: outputs[1],
                            host_port_id: shoop_egui::HostPortId::new("webaudio:destination_2"),
                            connected: false,
                        })
                    })
                    .map(|()| Self::WaitForDisconnectedOutput {
                        left: outputs[0],
                        right: outputs[1],
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForDisconnectedOutput {
                left,
                right,
                callbacks_before,
            } => {
                let disconnected = !snapshot.connections.confirmed_links.iter().any(|link| {
                    (link.application_port_id == left
                        && link.host_port_id.as_str() == "webaudio:destination_1")
                        || (link.application_port_id == right
                            && link.host_port_id.as_str() == "webaudio:destination_2")
                });
                if !disconnected
                    || snapshot.status.callback_count <= callbacks_before.saturating_add(5)
                    || snapshot.status.output_peak > 0.000_001
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::SetPortConnected {
                        port_id: left,
                        host_port_id: shoop_egui::HostPortId::new("webaudio:destination_1"),
                        connected: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::SetPortConnected {
                            port_id: right,
                            host_port_id: shoop_egui::HostPortId::new("webaudio:destination_2"),
                            connected: true,
                        })
                    })
                    .map(|()| Self::WaitForReconnectedOutput {
                        left,
                        right,
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::WaitForReconnectedOutput {
                left,
                right,
                callbacks_before,
            } => {
                let left_connected = snapshot.connections.confirmed_links.iter().any(|link| {
                    link.application_port_id == left
                        && link.host_port_id.as_str() == "webaudio:destination_1"
                });
                let right_connected = snapshot.connections.confirmed_links.iter().any(|link| {
                    link.application_port_id == right
                        && link.host_port_id.as_str() == "webaudio:destination_2"
                });
                if !left_connected
                    || !right_connected
                    || snapshot.status.callback_count <= callbacks_before
                    || snapshot.status.output_peak <= 0.000_001
                {
                    return;
                }
                let Some(tiny) = snapshot.tracks.iter().find(|track| {
                    track.fx.as_ref().is_some_and(|fx| {
                        fx.processor_type.as_str()
                            == shoop_egui::TrackProcessorTypeId::OXISYNTH
                    })
                }) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::Track {
                        track_id: tiny.id,
                        action: shoop_egui::TrackAction::InputMonitoringChanged {
                            enabled: true,
                            respect_auto_mute: false,
                        },
                    })
                    .map(|()| Self::SaveSession {
                        callbacks_before: snapshot.status.callback_count,
                    })
            }
            Self::SaveSession { callbacks_before } => runtime
                .dispatch(AppIntent::RequestSaveSession)
                .map(|()| Self::WaitForSessionSave { callbacks_before }),
            Self::WaitForSessionSave { callbacks_before } => {
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return;
                }
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                if !output.suggested_name.ends_with(".shoop") || output.bytes.is_empty() {
                    return self.fail("browser session output is invalid");
                }
                let mut bundle = match shoop_session::decode_session(&output.bytes) {
                    Ok(bundle) => bundle,
                    Err(error) => {
                        return self.fail(&format!("could not decode browser session: {error}"));
                    }
                };
                bundle.document.scripts.push(shoop_session::ScriptDocument {
                    id: 9_000_001,
                    name: BROWSER_SESSION_SCRIPT_NAME.to_owned(),
                    entrypoint: "main.lua".to_owned(),
                    enabled: true,
                });
                bundle
                    .scripts
                    .insert(9_000_001, browser_session_script_bundle());
                let bytes = match shoop_session::encode_session(&bundle, env!("CARGO_PKG_VERSION"))
                {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return self.fail(&format!("could not encode browser session: {error}"));
                    }
                };
                runtime
                    .dispatch(AppIntent::LoadSessionBytes {
                        name: output.suggested_name,
                        bytes: bytes.into(),
                    })
                    .map(|()| Self::WaitForSessionLoad { callbacks_before })
            }
            Self::WaitForSessionLoad { callbacks_before } => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::LoadSession
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                if snapshot
                    .tracks
                    .iter()
                    .filter(|track| !track.is_sync)
                    .count()
                    != 4
                {
                    return self.fail("loaded browser session lost tracks");
                }
                let builtin_fx_state = snapshot.tracks.iter().find_map(|track| {
                    let fx = track.fx.as_ref()?;
                    if fx.processor_type.as_str()
                        != shoop_egui::TrackProcessorTypeId::BUILTIN_FX
                    {
                        return None;
                    }
                    match fx.editor.as_ref()? {
                        shoop_egui::TrackProcessorEditorState::BuiltInFx(editor) => {
                            Some((
                                fx.visible,
                                editor.reverb_enabled,
                                editor.drive_enabled,
                                editor.drive_type,
                                editor.drive_db,
                                editor.midi_cc_assignments.len(),
                            ))
                        }
                        shoop_egui::TrackProcessorEditorState::OxiSynth(_) => None,
                    }
                });
                let Some((
                    builtin_fx_visible,
                    reverb_enabled,
                    drive_enabled,
                    drive_type,
                    drive_db,
                    assignments,
                )) = builtin_fx_state
                else {
                    return;
                };
                if builtin_fx_visible
                    || reverb_enabled
                    || !drive_enabled
                    || drive_type != shoop_egui::BuiltInFxDriveType::Fuzz
                    || (drive_db - 24.0).abs() > f32::EPSILON
                    || assignments != 1
                {
                    return self.fail("loaded browser Built-in FX state changed");
                }
                let synth_state = snapshot.tracks.iter().find_map(|track| {
                    let fx = track.fx.as_ref()?;
                    if fx.processor_type.as_str() != shoop_egui::TrackProcessorTypeId::OXISYNTH {
                        return None;
                    }
                    match fx.editor.as_ref()? {
                        shoop_egui::TrackProcessorEditorState::BuiltInFx(_) => None,
                        shoop_egui::TrackProcessorEditorState::OxiSynth(editor) => {
                            Some((fx.visible, editor))
                        }
                    }
                });
                let Some((synth_visible, synth_state)) = synth_state else {
                    return;
                };
                if synth_visible {
                    return self.fail("loaded browser Built-in Synth editor visibility persisted");
                }
                if synth_state.selected_preset_id != "0:40"
                    || synth_state.reverb_send != 0.4
                    || synth_state.chorus_send != 0.6
                {
                    return self.fail(&format!(
                        "loaded browser Built-in Synth state changed: {synth_state:?}"
                    ));
                }
                if !snapshot.scripting.scripts.iter().any(|script| {
                    script.kind == ScriptKind::Session
                        && script.name == BROWSER_SESSION_SCRIPT_NAME
                        && script.enabled
                        && script.lifecycle == shoop_egui::ScriptLifecycle::Listening
                }) {
                    return self.fail(&format!(
                        "loaded browser session lost its active Lua script: {:?}",
                        snapshot.scripting.scripts
                    ));
                }
                let resource_base_uri = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .find(|script| script.name == BROWSER_SESSION_SCRIPT_NAME)
                    .and_then(|script| script.resource_base_uri.as_deref());
                let Some(resource_base_uri) = resource_base_uri else {
                    return self.fail("browser session Markdown lost its bundle resource origin");
                };
                let image_uri = format!("{resource_base_uri}image.png");
                if !matches!(
                    shoop_script_resources::read_resource_uri(&image_uri),
                    Ok(Some(bytes)) if bytes.as_ref() == BROWSER_SESSION_IMAGE
                ) {
                    return self.fail("browser session image was unavailable from its bundle");
                }
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return self.fail("audio callbacks did not advance through session reload");
                }
                if args.stress {
                    // Ordinary hosted/direct-file workflows verify exact script resave.
                    // Avoid a duplicate capture here so the stress case remains focused on
                    // sustained render/capture and reload.
                    Ok(Self::PlayLoadedLoop)
                } else {
                    Ok(Self::SaveLoadedScriptSession {
                        callbacks_before: snapshot.status.callback_count,
                    })
                }
            }
            Self::SaveLoadedScriptSession { callbacks_before } => runtime
                .dispatch(AppIntent::RequestSaveSession)
                .map(|()| Self::WaitForLoadedScriptSave { callbacks_before }),
            Self::WaitForLoadedScriptSave { callbacks_before } => {
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && snapshot.status.callback_count <= callbacks_before
                {
                    return;
                }
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let bundle = match shoop_session::decode_session(&output.bytes) {
                    Ok(bundle) => bundle,
                    Err(error) => {
                        return self.fail(&format!(
                            "could not decode browser script round trip: {error}"
                        ));
                    }
                };
                if !bundle.document.scripts.iter().any(|script| {
                    script.name == BROWSER_SESSION_SCRIPT_NAME
                        && script.enabled
                        && bundle.scripts.get(&script.id).is_some_and(|resources| {
                            resources.entrypoint_resource().bytes.as_ref()
                                == BROWSER_SESSION_SCRIPT_SOURCE.as_bytes()
                                && resources
                                    .resources
                                    .get(
                                        &shoop_script_resources::NormalizedRelativePath::parse(
                                            "help.md",
                                        )
                                        .unwrap(),
                                    )
                                    .is_some_and(|resource| {
                                        resource.bytes.as_ref() == BROWSER_SESSION_MARKDOWN
                                    })
                                && resources
                                    .resources
                                    .get(
                                        &shoop_script_resources::NormalizedRelativePath::parse(
                                            "image.png",
                                        )
                                        .unwrap(),
                                    )
                                    .is_some_and(|resource| {
                                        resource.bytes.as_ref() == BROWSER_SESSION_IMAGE
                                    })
                        })
                }) {
                    return self.fail("browser session Lua source did not round trip exactly");
                }
                if snapshot.status.audio_driver == shoop_egui::AudioDriverState::Dummy {
                    Ok(Self::ExportLoopAudio)
                } else {
                    Ok(Self::PlayLoadedLoop)
                }
            }
            Self::PlayLoadedLoop => {
                let Some((track, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::Loop {
                        track_id: track.id,
                        loop_id: loop_state.id,
                        action: shoop_egui::LoopAction::PlayClicked,
                    })
                    .map(|()| Self::WaitForLoadedPlayback)
            }
            Self::WaitForLoadedPlayback => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Playing
                    || snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                        && snapshot.status.output_peak <= 0.000_001
                {
                    return;
                }
                if args.stress || args.session_only {
                    Ok(Self::RejectProcessedSession)
                } else {
                    let Some((track, loop_state)) = first_main_loop(snapshot) else {
                        return;
                    };
                    runtime
                        .dispatch(AppIntent::Global(
                            shoop_egui::GlobalControlAction::DeselectAll,
                        ))
                        .and_then(|()| {
                            runtime.dispatch(AppIntent::Loop {
                                track_id: track.id,
                                loop_id: loop_state.id,
                                action: shoop_egui::LoopAction::StopClicked,
                            })
                        })
                        .map(|()| Self::WaitForMediaStopped)
                }
            }
            Self::WaitForMediaStopped => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                if loop_state.mode != shoop_egui::LoopMode::Stopped
                    || snapshot
                        .tracks
                        .iter()
                        .flat_map(|track| &track.loops)
                        .any(|loop_| loop_.selected)
                    || snapshot.details.is_some()
                {
                    return;
                }
                Ok(Self::ExportLoopAudio)
            }
            Self::ExportLoopAudio => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::RequestLoopAudioExport {
                        loop_id: loop_state.id,
                        format: shoop_egui::LoopAudioExportFormat::Exact,
                    })
                    .map(|()| Self::WaitForLoopAudioSelection)
            }
            Self::WaitForLoopAudioSelection => {
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                let Some(selection) = &task.audio_channel_selection else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ConfirmAudioChannelSelection {
                        task_id: task.id,
                        channels: selection.default_selection.clone(),
                    })
                    .map(|()| Self::WaitForLoopAudioExport)
            }
            Self::WaitForLoopAudioExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ImportLoopAudioBytes {
                        loop_id: loop_state.id,
                        name: output.suggested_name,
                        bytes: output.bytes,
                        update_loop_length: true,
                    })
                    .map(|()| Self::WaitForLoopAudioMapping)
            }
            Self::WaitForLoopAudioMapping => {
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                let Some(mapping) = &task.audio_channel_mapping else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ConfirmAudioChannelMapping {
                        task_id: task.id,
                        source_for_destination: mapping.default_mapping.clone(),
                    })
                    .map(|()| Self::WaitForLoopAudioImport)
            }
            Self::WaitForLoopAudioImport => {
                if snapshot.io_task.as_ref().is_some_and(|task| {
                    task.kind == shoop_egui::IoTaskKind::ImportLoopAudio
                        && task.status == shoop_egui::IoTaskStatus::Failed
                }) {
                    return self.fail(&format!(
                        "browser loop audio import failed: {:?}",
                        snapshot.io_task
                    ));
                }
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::ImportLoopAudio
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                Ok(Self::ExportLoopMidi)
            }
            Self::ExportLoopMidi => {
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::RequestLoopMidiExport {
                        loop_id: loop_state.id,
                        standard: false,
                    })
                    .map(|()| Self::WaitForLoopMidiExport)
            }
            Self::WaitForLoopMidiExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ImportLoopMidiBytes {
                        loop_id: loop_state.id,
                        name: output.suggested_name,
                        bytes: output.bytes,
                        update_loop_length: true,
                    })
                    .map(|()| Self::WaitForLoopMidiImport)
            }
            Self::WaitForLoopMidiImport => {
                if snapshot.io_task.as_ref().is_some_and(|task| {
                    task.kind == shoop_egui::IoTaskKind::ImportLoopMidi
                        && task.status == shoop_egui::IoTaskStatus::Failed
                }) {
                    return self.fail(&format!(
                        "browser loop MIDI import failed: {:?}",
                        snapshot.io_task
                    ));
                }
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::ImportLoopMidi
                        || task.status != shoop_egui::IoTaskStatus::Completed
                }) {
                    return;
                }
                Ok(Self::GenerateClickAudio)
            }
            Self::GenerateClickAudio => {
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                if !widget.browser_test_open_click_track(snapshot, loop_state.id) {
                    self.fail("production click-track dialog did not open");
                    return;
                }
                let previous_task = snapshot
                    .io_task
                    .as_ref()
                    .map(|task| task.id)
                    .unwrap_or_default();
                runtime
                    .dispatch(AppIntent::GenerateClickTrack {
                        loop_id: loop_state.id,
                        request: browser_click_request(shoop_egui::ClickTrackKind::Audio),
                    })
                    .map(|()| Self::WaitForClickAudio {
                        previous_task,
                        request_revision: snapshot.revision,
                    })
            }
            Self::WaitForClickAudio {
                previous_task,
                request_revision,
            } => {
                if click_request_was_rejected(
                    snapshot.revision,
                    snapshot.io_task.as_ref(),
                    previous_task,
                    request_revision,
                ) {
                    return self.fail("browser audio click request was rejected before task creation");
                }
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                if task.id != previous_task
                    && task.kind == shoop_egui::IoTaskKind::GenerateClickTrack
                    && task.status == shoop_egui::IoTaskStatus::Failed
                {
                    return self.fail(&format!(
                        "browser audio click generation failed: {:?}",
                        snapshot.io_task
                    ));
                }
                if task.id == previous_task
                    || task.kind != shoop_egui::IoTaskKind::GenerateClickTrack
                    || task.status != shoop_egui::IoTaskStatus::Completed
                {
                    return;
                }
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                let expected = u64::from(snapshot.status.sample_rate) * 3 / 50;
                if loop_state.length_frames != expected || loop_state.empty {
                    self.fail("generated browser audio click length/state is incorrect");
                    return;
                }
                runtime
                    .dispatch(AppIntent::RequestLoopAudioExport {
                        loop_id: loop_state.id,
                        format: shoop_egui::LoopAudioExportFormat::Exact,
                    })
                    .map(|()| Self::WaitForClickAudioSelection)
            }
            Self::WaitForClickAudioSelection => {
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                let Some(selection) = &task.audio_channel_selection else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::ConfirmAudioChannelSelection {
                        task_id: task.id,
                        channels: selection.default_selection.clone(),
                    })
                    .map(|()| Self::WaitForClickAudioExport)
            }
            Self::WaitForClickAudioExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Ok(audio) = shoop_session::decode_loop_audio(&output.bytes) else {
                    self.fail("generated browser click audio did not decode");
                    return;
                };
                let expected = (u64::from(snapshot.status.sample_rate) * 3 / 50) as usize;
                if audio.channels.len() != 2
                    || audio
                        .channels
                        .iter()
                        .any(|channel| channel.samples.len() != expected)
                    || audio
                        .channels
                        .iter()
                        .any(|channel| !channel.samples.iter().any(|sample| *sample != 0.0))
                {
                    self.fail("generated browser click audio payload is incorrect");
                    return;
                }
                Ok(Self::PreviewClickAudio)
            }
            Self::PreviewClickAudio => {
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::PreviewClickTrack {
                output: shoop_egui::ClickTrackPreviewOutput::System,
                        loop_id: loop_state.id,
                        request: browser_click_request(shoop_egui::ClickTrackKind::Audio),
                    })
                    .map(|()| Self::WaitForClickPreview)
            }
            Self::WaitForClickPreview => match snapshot.click_track.preview_status {
                shoop_egui::ClickTrackPreviewStatus::Completed => Ok(Self::GenerateClickMidi),
                shoop_egui::ClickTrackPreviewStatus::Failed => {
                    self.fail("browser click preview failed");
                    return;
                }
                _ => return,
            },
            Self::GenerateClickMidi => {
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                let previous_task = snapshot
                    .io_task
                    .as_ref()
                    .map(|task| task.id)
                    .unwrap_or_default();
                runtime
                    .dispatch(AppIntent::GenerateClickTrack {
                        loop_id: loop_state.id,
                        request: browser_click_request(shoop_egui::ClickTrackKind::Midi),
                    })
                    .map(|()| Self::WaitForClickMidi {
                        previous_task,
                        request_revision: snapshot.revision,
                    })
            }
            Self::WaitForClickMidi {
                previous_task,
                request_revision,
            } => {
                if click_request_was_rejected(
                    snapshot.revision,
                    snapshot.io_task.as_ref(),
                    previous_task,
                    request_revision,
                ) {
                    return self.fail("browser MIDI click request was rejected before task creation");
                }
                let Some(task) = &snapshot.io_task else {
                    return;
                };
                if task.id != previous_task
                    && task.kind == shoop_egui::IoTaskKind::GenerateClickTrack
                    && task.status == shoop_egui::IoTaskStatus::Failed
                {
                    return self.fail(&format!(
                        "browser MIDI click generation failed: {:?}",
                        snapshot.io_task
                    ));
                }
                if task.id == previous_task
                    || task.kind != shoop_egui::IoTaskKind::GenerateClickTrack
                    || task.status != shoop_egui::IoTaskStatus::Completed
                {
                    return;
                }
                let Some((_, loop_state)) = first_mixed_main_loop(snapshot) else {
                    return;
                };
                runtime
                    .dispatch(AppIntent::RequestLoopMidiExport {
                        loop_id: loop_state.id,
                        standard: false,
                    })
                    .map(|()| Self::WaitForClickMidiExport)
            }
            Self::WaitForClickMidiExport => {
                let Some(output) = runtime.take_file_output() else {
                    return;
                };
                let Ok(midi) = shoop_session::decode_exact_midi(&output.bytes) else {
                    self.fail("generated browser click MIDI did not decode");
                    return;
                };
                if midi.length_frames != u64::from(snapshot.status.sample_rate) * 3 / 50
                    || midi.events.len() != 2
                    || midi.events[0].data != [0x90, 67, 127]
                {
                    self.fail("generated browser click MIDI payload is incorrect");
                    return;
                }
                Ok(Self::RejectProcessedSession)
            }
            Self::RejectProcessedSession | Self::RejectExternalSession => {
                let external = matches!(*self, Self::RejectExternalSession);
                match browser_unsupported_session_bytes(snapshot.status.sample_rate, !external) {
                    Ok(bytes) => runtime
                        .dispatch(AppIntent::LoadSessionBytes {
                            name: "unsupported-processed.shoop".to_owned(),
                            bytes,
                        })
                        .map(|()| Self::WaitForProcessedSessionRejection {
                            audio_progress_before: snapshot
                                .status
                                .callback_count
                                .max(snapshot.status.processed_frames),
                            external,
                        }),
                    Err(error) => return self.fail(&error),
                }
            }
            Self::WaitForProcessedSessionRejection {
                audio_progress_before,
                external,
            } => {
                if snapshot.io_task.as_ref().is_none_or(|task| {
                    task.kind != shoop_egui::IoTaskKind::LoadSession
                        || task.status != shoop_egui::IoTaskStatus::Failed
                }) {
                    return;
                }
                if snapshot
                    .status
                    .callback_count
                    .max(snapshot.status.processed_frames)
                    <= audio_progress_before
                {
                    return;
                }
                let Some((_, loop_state)) = first_main_loop(snapshot) else {
                    return self.fail("processed-session rejection removed direct tracks");
                };
                if snapshot.status.audio_driver != shoop_egui::AudioDriverState::Dummy
                    && loop_state.empty
                {
                    return self.fail("processed-session rejection removed direct loop media");
                }
                if external {
                    Ok(Self::AddLuaVersionChecks)
                } else {
                    Ok(Self::RejectExternalSession)
                }
            }
            Self::AddLuaVersionChecks => runtime
                .dispatch(AppIntent::Global(
                    shoop_egui::GlobalControlAction::SetSolo(false),
                ))
                .and_then(|()| {
                    [
                        (
                            "lua-api-higher-minor.lua",
                            "shoop_announce_api_version(1, 7); require('shoop_control').set_solo(true)",
                        ),
                        (
                            "lua-api-lower-major.lua",
                            "shoop_announce_api_version(0, 0); require('shoop_control').set_solo(true)",
                        ),
                        (
                            "lua-api-higher-major.lua",
                            "shoop_announce_api_version(2, 0); require('shoop_control').set_solo(true)",
                        ),
                        (
                            "lua-api-unannounced.lua",
                            "require('shoop_control').set_solo(true)",
                        ),
                    ]
                    .into_iter()
                    .try_for_each(|(name, source)| {
                        runtime.dispatch(AppIntent::AddScriptSource {
                            name: name.to_owned(),
                            source: std::sync::Arc::from(source),
                            kind: ScriptKind::User,
                            enabled: true,
                        })
                    })
                })
                .map(|()| Self::WaitForLuaVersionChecks),
            Self::WaitForLuaVersionChecks => {
                let rejected = snapshot
                    .scripting
                    .scripts
                    .iter()
                    .filter(|script| script.name.starts_with("lua-api-"))
                    .collect::<Vec<_>>();
                if rejected.len() != 4
                    || rejected
                        .iter()
                        .any(|script| !lua_version_rejection_is_expected(script))
                    || snapshot.global_controls.solo
                    || !snapshot.scripting.dialogs.is_empty()
                {
                    return;
                }
                runtime
                    .dispatch(AppIntent::AddScriptSource {
                        name: "dialogs.lua".to_owned(),
                        source: std::sync::Arc::from(
                            "shoop_announce_api_version(1, 0); local c=require('shoop_control'); local d=require('shoop_dialog'); d.simple('Lua dialog example', {d.button('Toggle Solo and show guide', function() c.set_solo(true); d.open('Lua dialog guide') end)}); d.paged('Lua dialog guide', {{d.markdown('Page one')}, {d.markdown('Page two')}}); d.open('Lua dialog example')",
                        ),
                        kind: ScriptKind::User,
                        enabled: true,
                    })
                    .and_then(|()| {
                        runtime.dispatch(AppIntent::AddScriptSource {
                            name: "dialog-survivor.lua".to_owned(),
                            source: std::sync::Arc::from(
                                "shoop_announce_api_version(1, 0); local d=require('shoop_dialog'); d.simple('Survivor', {d.rich_text('Still active')})",
                            ),
                            kind: ScriptKind::User,
                            enabled: true,
                        })
                    })
                    .map(|()| Self::WaitForLuaDialogs)
            }
            Self::WaitForLuaDialogs => {
                let Some(welcome) = snapshot
                    .scripting
                    .dialogs
                    .iter()
                    .find(|dialog| dialog.name == "Lua dialog example")
                else {
                    return;
                };
                let Some(guide) = snapshot
                    .scripting
                    .dialogs
                    .iter()
                    .find(|dialog| dialog.name == "Lua dialog guide")
                else {
                    return;
                };
                if snapshot.scripting.dialogs.len() != 3
                    || widget.browser_test_lua_dialog_count() != 3
                    || widget.browser_test_lua_dialog_state(welcome.id) != Some((true, 0))
                    || widget.browser_test_lua_dialog_state(guide.id) != Some((false, 0))
                {
                    return;
                }
                let shoop_egui::ScriptDialogKind::Simple(content) = &welcome.kind else {
                    return self.fail("browser Lua simple dialog has the wrong flavor");
                };
                let Some(button_id) = content.elements.iter().find_map(|element| match element {
                    shoop_egui::ScriptDialogElement::Button { id, .. } => *id,
                    _ => None,
                }) else {
                    return self.fail("browser Lua dialog callback button is missing");
                };
                widget.browser_test_close_lua_dialog(welcome.id);
                widget.browser_test_set_lua_dialog_page(guide.id, 1);
                runtime
                    .dispatch(AppIntent::InvokeScriptDialogButton {
                        script_id: welcome.owner_script_id,
                        dialog_id: welcome.id,
                        button_id,
                    })
                    .map(|()| Self::WaitForLuaDialogCallback {
                        script_id: welcome.owner_script_id,
                        welcome: welcome.id,
                        guide: guide.id,
                    })
            }
            Self::WaitForLuaDialogCallback {
                script_id,
                welcome,
                guide,
            } => {
                let guide_opened = snapshot
                    .scripting
                    .dialogs
                    .iter()
                    .find(|dialog| dialog.id == guide)
                    .is_some_and(|dialog| dialog.open_request == 1);
                if !snapshot.global_controls.solo
                    || !guide_opened
                    || widget.browser_test_lua_dialog_state(welcome) != Some((false, 0))
                    || widget.browser_test_lua_dialog_state(guide) != Some((true, 1))
                {
                    return;
                }
                widget.browser_test_close_lua_dialog(guide);
                widget.browser_test_open_lua_dialog_from_list(guide);
                if widget.browser_test_lua_dialog_state(guide) != Some((true, 1)) {
                    return self.fail("browser Lua dialog page state did not survive close/reopen");
                }
                runtime
                    .dispatch(AppIntent::StopScript { script_id })
                    .map(|()| Self::WaitForLuaDialogsStopped)
            }
            Self::WaitForLuaDialogsStopped => {
                if snapshot.scripting.dialogs.len() != 1
                    || snapshot.scripting.dialogs[0].name != "Survivor"
                    || widget.browser_test_lua_dialog_count() != 1
                {
                    return;
                }
                Ok(Self::Complete)
            }
        };

        match result {
            Ok(next) => {
                *self = next;
                if next == Self::Complete {
                    set_browser_self_test_status("passed");
                } else {
                    set_browser_self_test_status(&format!("{next:?}"));
                }
            }
            Err(error) => {
                *self = Self::Failed;
                set_browser_self_test_status("failed");
                log::error!("browser dummy-engine self-test dispatch failed: {error}");
            }
        }
    }

    fn fail(&mut self, message: &str) {
        *self = Self::Failed;
        set_browser_self_test_status("failed");
        if let Some(element) = browser_status_element() {
            let _ = element.set_attribute("data-self-test-error", message);
        }
        log::error!("browser self-test failed: {message}");
    }
}

#[cfg(target_arch = "wasm32")]
fn first_main_loop(
    snapshot: &AppSnapshot,
) -> Option<(&shoop_egui::TrackState, &shoop_egui::LoopState)> {
    let track = snapshot.tracks.iter().find(|track| !track.is_sync)?;
    Some((track, track.loops.first()?))
}

#[cfg(target_arch = "wasm32")]
fn first_mixed_main_loop(
    snapshot: &AppSnapshot,
) -> Option<(&shoop_egui::TrackState, &shoop_egui::LoopState)> {
    snapshot
        .tracks
        .iter()
        .filter(|track| !track.is_sync)
        .find_map(|track| {
            track
                .loops
                .iter()
                .find(|loop_| loop_.has_audio && loop_.has_midi)
                .map(|loop_| (track, loop_))
        })
}

