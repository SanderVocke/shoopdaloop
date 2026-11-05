mod processors;
use anyhow::{anyhow, bail, ensure, Result};
use processors::*;
use shoop_app_api::*;
use shoop_app_protocol::persistence_image as image;
use shoop_session::*;
use std::{collections::BTreeSet, sync::Arc};

fn port_to_document(port: &image::Port) -> Result<PortDocument> {
    let state = &port.state;
    let input = state.direction == PortDirection::Input;
    Ok(PortDocument {
        id: state.id.raw(),
        name: state.name.clone(),
        data_type: match state.data_type {
            PortDataType::Audio => DataTypeDocument::Audio,
            PortDataType::Midi => DataTypeDocument::Midi,
        },
        direction: if input {
            PortDirectionDocument::Input
        } else {
            PortDirectionDocument::Output
        },
        role: match state.role {
            PortRole::AudioInput => PortRoleDocument::AudioInput,
            PortRole::AudioOutput => PortRoleDocument::AudioOutput,
            PortRole::MidiInput => PortRoleDocument::MidiInput,
            PortRole::MidiOutput => PortRoleDocument::MidiOutput,
            PortRole::AudioSend => PortRoleDocument::AudioSend,
            PortRole::AudioReturn => PortRoleDocument::AudioReturn,
            PortRole::MidiSend => PortRoleDocument::MidiSend,
        },
        input_connectability: vec![if input {
            ConnectabilityDocument::External
        } else {
            ConnectabilityDocument::Internal
        }],
        output_connectability: vec![if input {
            ConnectabilityDocument::Internal
        } else {
            ConnectabilityDocument::External
        }],
        gain: 1.,
        muted: false,
        passthrough_muted: state.owner == ApplicationPortOwner::GlobalFxControl,
        internal_connections: Vec::new(),
        external_connections: port.external_connections.clone(),
        ringbuffer_frames: 0,
    })
}
fn port_from_document(port: PortDocument, owner: ApplicationPortOwner) -> Result<image::Port> {
    let input = port.direction == PortDirectionDocument::Input;
    ensure!(
        port.gain == 1.
            && !port.muted
            && port.passthrough_muted == (owner == ApplicationPortOwner::GlobalFxControl)
            && port.internal_connections.is_empty()
            && port.ringbuffer_frames == 0
            && port.input_connectability
                == [if input {
                    ConnectabilityDocument::External
                } else {
                    ConnectabilityDocument::Internal
                }]
            && port.output_connectability
                == [if input {
                    ConnectabilityDocument::Internal
                } else {
                    ConnectabilityDocument::External
                }],
        "port {} requires unsupported gain, mute, monitoring, routing or recording history",
        port.name
    );
    Ok(image::Port {
        state: ApplicationPortState {
            publication: None,
            id: PortId::from_raw(port.id),
            owner,
            name: port.name,
            data_type: match port.data_type {
                DataTypeDocument::Audio => PortDataType::Audio,
                DataTypeDocument::Midi => PortDataType::Midi,
            },
            direction: if input {
                PortDirection::Input
            } else {
                PortDirection::Output
            },
            role: match port.role {
                PortRoleDocument::AudioInput => PortRole::AudioInput,
                PortRoleDocument::AudioOutput => PortRole::AudioOutput,
                PortRoleDocument::MidiInput => PortRole::MidiInput,
                PortRoleDocument::MidiOutput => PortRole::MidiOutput,
                PortRoleDocument::AudioSend => PortRole::AudioSend,
                PortRoleDocument::AudioReturn => PortRole::AudioReturn,
                PortRoleDocument::MidiSend => PortRole::MidiSend,
                PortRoleDocument::Internal => bail!("internal processor ports cannot be persisted"),
            },
            connection_policy: ConnectionPolicy::UserManaged,
        },
        external_connections: port.external_connections,
    })
}
fn connected_ports(
    ports: &[image::Port],
    midi: bool,
    index: usize,
    mode: image::ChannelMode,
) -> Vec<u64> {
    let roles: &[PortRole] = match (midi, mode) {
        (false, image::ChannelMode::Direct) => &[PortRole::AudioInput, PortRole::AudioOutput],
        (false, image::ChannelMode::Dry) => &[PortRole::AudioInput, PortRole::AudioSend],
        (false, image::ChannelMode::Wet) => &[PortRole::AudioReturn, PortRole::AudioOutput],
        (true, image::ChannelMode::Direct) => &[PortRole::MidiInput, PortRole::MidiOutput],
        (true, image::ChannelMode::Dry) => &[PortRole::MidiInput, PortRole::MidiSend],
        (true, image::ChannelMode::Wet) => &[],
    };
    roles
        .iter()
        .filter_map(|role| ports.iter().filter(|p| p.state.role == *role).nth(index))
        .map(|p| p.state.id.raw())
        .collect()
}
fn channel_document(
    channel: &image::Channel,
    id: u64,
    midi: bool,
    connected_port_ids: Vec<u64>,
    media_id: Option<String>,
) -> ChannelDocument {
    ChannelDocument {
        id,
        mode: mode_to_document(channel.mode),
        data_type: if midi {
            DataTypeDocument::Midi
        } else {
            DataTypeDocument::Audio
        },
        data_length_frames: channel.frames,
        start_offset_frames: channel.start_offset,
        capture_alignment_frames: channel.capture_alignment,
        preplay_frames: channel.preplay,
        gain: channel.gain,
        connected_port_ids,
        media_id,
        recording_started_at: channel.recording_started_at.clone(),
        recording_fx_state_id: None,
    }
}
pub(super) fn exact_midi(
    image: &image::Image,
    channel: &image::MidiChannel,
    sample_rate: u32,
) -> Result<ExactMidi> {
    let events = match channel.channel.object {
        None => Vec::new(),
        Some(id) => match &image.payloads[id as usize - 1] {
            image::Payload::Midi(events) => events
                .iter()
                .enumerate()
                .map(|(order, event)| ExactMidiEvent {
                    frame: u64::from(event.frame),
                    order: order as u32,
                    data: event.data.to_vec(),
                })
                .collect(),
            _ => bail!("invalid MIDI payload"),
        },
    };
    Ok(ExactMidi {
        sample_rate,
        length_frames: channel.channel.frames,
        start_state: channel.initial.clone(),
        events,
    })
}
pub(super) fn to_bundle(image: &image::Image) -> Result<SessionBundle> {
    image
        .validate()
        .map_err(|r| anyhow!("invalid capture: {r:?}"))?;
    let image::Manifest::Session(session) = &image.manifest else {
        bail!("expected session capture")
    };
    let mut bundle = SessionBundle::new(SessionDocument::empty(session.sample_rate));
    let mut groups = [
        TrackGroupDocument {
            name: "sync".into(),
            tracks: Vec::new(),
        },
        TrackGroupDocument {
            name: "main".into(),
            tracks: Vec::new(),
        },
    ];
    let mut next_channel = 1;
    let mut next_fx_state = 1;
    for track in &session.tracks {
        let topology = document_topology(track)?;
        image_topology(&topology)?;
        let fx_chain = processor_chain(track)?;
        let mut loops = Vec::new();
        for content in &track.loops {
            let recorded_id = if let Some(encoded) = &content.recorded_processor_state {
                let chain = fx_chain
                    .as_ref()
                    .ok_or_else(|| anyhow!("recorded state needs a processor"))?;
                let id = next_fx_state;
                next_fx_state += 1;
                bundle.document.fx_states.push(FxStateDocument {
                    id,
                    chain_type: chain.chain_type,
                    internal_state: canonical_state(chain.chain_type, encoded)?,
                });
                Some(id)
            } else {
                None
            };
            let mut channels = Vec::new();
            for (index, channel) in content.audio.iter().enumerate() {
                let media_id = if let Some(object) = channel.object {
                    let image::Payload::Audio(samples) = &image.payloads[object as usize - 1]
                    else {
                        bail!("invalid audio capture")
                    };
                    let id = format!("audio_t{}_l{}_c{index}", track.id.raw(), content.id.raw());
                    bundle.media.insert(
                        id.clone(),
                        MediaPayload::Audio(AudioPayload {
                            samples: samples.to_vec(),
                        }),
                    );
                    Some(id)
                } else {
                    None
                };
                let role_index = content.audio[..index]
                    .iter()
                    .filter(|c| c.mode == channel.mode)
                    .count();
                let mut document = channel_document(
                    channel,
                    next_channel,
                    false,
                    connected_ports(&track.ports, false, role_index, channel.mode),
                    media_id,
                );
                if channel.mode == image::ChannelMode::Wet {
                    document.recording_fx_state_id = recorded_id;
                }
                channels.push(document);
                next_channel += 1;
            }
            for (index, channel) in content.midi.iter().enumerate() {
                let media_id = if channel.channel.frames != 0
                    || channel.channel.object.is_some()
                    || !channel.initial.is_empty()
                {
                    let id = format!("midi_t{}_l{}_c{index}", track.id.raw(), content.id.raw());
                    bundle.media.insert(
                        id.clone(),
                        MediaPayload::Midi(exact_midi(image, channel, session.sample_rate)?),
                    );
                    Some(id)
                } else {
                    None
                };
                channels.push(channel_document(
                    &channel.channel,
                    next_channel,
                    true,
                    connected_ports(&track.ports, true, index, channel.channel.mode),
                    media_id,
                ));
                next_channel += 1;
            }
            loops.push(LoopDocument {
                id: content.id.raw(),
                name: content.name.clone(),
                length_frames: content.frames,
                is_sync: content.sync,
                gain: content.gain,
                balance: content.balance,
                channels,
                composite: None,
            });
        }
        let controls = &track.controls;
        groups[usize::from(!track.sync)].tracks.push(TrackDocument {
            id: track.id.raw(),
            name: track.name.clone(),
            port_name_base: track.port_name_base.clone(),
            is_sync: track.sync,
            width: None,
            topology,
            default_playback_mode: match track.default_playback_mode {
                DefaultPlaybackMode::Regular => DefaultPlaybackModeDocument::Regular,
                DefaultPlaybackMode::DryThroughWet => DefaultPlaybackModeDocument::DryThroughWet,
            },
            controls: TrackControlsDocument {
                output_gain_db: controls.output_gain_db,
                output_balance: controls.output_balance,
                output_muted: controls.muted,
                input_gain_db: controls.input_gain_db,
                input_balance: controls.input_balance,
                input_monitoring: controls.monitoring,
            },
            latency: TrackLatencyDocument::default(),
            loops,
            ports: track
                .ports
                .iter()
                .map(port_to_document)
                .collect::<Result<_>>()?,
            fx_chain,
        });
    }
    let global = &session.global;
    let doc = &mut bundle.document;
    doc.global = GlobalControlsDocument {
        default_recording_action: match global.default_recording_action {
            DefaultRecordingAction::Record => RecordingActionDocument::Record,
            DefaultRecordingAction::Grab => RecordingActionDocument::Grab,
        },
        play_after_record: global.play_after_record,
        sync: global.sync,
        solo: global.solo,
        auto_mute_other_track_inputs: global.auto_mute_other_track_inputs,
        auto_arm_track_inputs: global.auto_arm_track_inputs,
        apply_n_cycles: global.apply_n_cycles,
    };
    doc.track_groups = groups.into();
    doc.selected_loop_ids = session.selected.iter().map(|id| id.raw()).collect();
    doc.targeted_loop_id = session.target.map(|id| id.raw());
    doc.buses = session
        .buses
        .iter()
        .map(|master| {
            Ok(BusDocument {
                id: master.id.raw(),
                name: master.name.clone(),
                channels: master
                    .channels
                    .iter()
                    .map(|channel| BusChannelDocument {
                        id: channel.id.raw(),
                        label: channel.label.clone(),
                        output_port_id: channel.port.state.id.raw(),
                    })
                    .collect(),
                ports: master
                    .channels
                    .iter()
                    .map(|channel| port_to_document(&channel.port))
                    .collect::<Result<_>>()?,
                fx_chain: None,
                gain_db: master.gain_db,
                balance: master.balance,
                muted: master.muted,
            })
        })
        .collect::<Result<_>>()?;
    doc.preview_bus = match session.preview_bus {
        Some(id) => shoop_session::PreviewBusDocument::Bus(id.raw()),
        None => shoop_session::PreviewBusDocument::Disabled,
    };
    doc.bus_display_order = session
        .bus_display_order
        .iter()
        .map(|id| id.raw())
        .collect();
    doc.global_ports = session
        .global_ports
        .iter()
        .map(port_to_document)
        .collect::<Result<_>>()?;
    doc.mixer_routes = session
        .routes
        .iter()
        .map(|route| MixerRouteDocument {
            source_port_id: route.source_port_id.raw(),
            destination_channel_id: route.destination_channel_id.raw(),
        })
        .collect();
    validate_bundle(&bundle)?;
    Ok(bundle)
}

pub(super) fn requirements(doc: &SessionDocument) -> Result<()> {
    let mut unsupported = BTreeSet::new();
    if !doc.scripts.is_empty() {
        unsupported.insert("session scripts/resources (P16)");
    }
    for state in &doc.fx_states {
        canonical_state(state.chain_type, &state.internal_state)?;
    }
    if !doc.midi_control.bindings.is_empty() || !doc.settings.is_empty() {
        unsupported.insert("reserved MIDI bindings/session settings");
    }
    if doc.global.default_recording_action != RecordingActionDocument::Record {
        unsupported.insert("default grab recording (P04)");
    }
    if doc.buses.len() > 8
        || doc
            .buses
            .iter()
            .any(|b| b.channels.is_empty() || b.channels.len() > 8)
    {
        unsupported.insert("buses exceeding replacement engine capacity");
    }
    if doc.buses.iter().any(|b| b.fx_chain.is_some()) {
        unsupported.insert("bus FX (P08)");
    }
    let mut seen_groups = BTreeSet::new();
    for group in &doc.track_groups {
        if !seen_groups.insert(group.name.as_str())
            || !matches!(group.name.as_str(), "sync" | "main")
            || group
                .tracks
                .iter()
                .any(|track| track.is_sync != (group.name == "sync"))
        {
            unsupported.insert("custom track groups");
        }
        for track in &group.tracks {
            image_topology(&track.topology)?;
            if let Some(chain) = &track.fx_chain {
                canonical_state(chain.chain_type, &chain.internal_state)?;
                ensure!(
                    chain.ports.is_empty(),
                    "processor port routing is unsupported"
                );
            }
            if track.width.is_some() {
                unsupported.insert("stored track width");
            }
            if track.latency != TrackLatencyDocument::default() {
                unsupported.insert("latency compensation/take alignment (P09/P10)");
            }
            for content in &track.loops {
                if content.composite.is_some() {
                    unsupported.insert("composite loops (P03)");
                }
                for channel in &content.channels {
                    mode_from_document(channel.mode)?;
                }
            }
        }
    }
    ensure!(
        unsupported.is_empty(),
        "Session requires unsupported features: {}",
        unsupported.into_iter().collect::<Vec<_>>().join(", ")
    );
    Ok(())
}
pub(super) fn from_bundle(bundle: SessionBundle) -> Result<Arc<image::Image>> {
    requirements(&bundle.document)?;
    ensure!(bundle.scripts.is_empty(), "session scripts/resources (P16)");
    validate_bundle(&bundle)?;
    let media = bundle
        .media
        .into_iter()
        .map(|(id, payload)| {
            let (payload, initial) = match payload {
                MediaPayload::Audio(audio) => {
                    (image::Payload::Audio(Arc::new(audio.samples)), Vec::new())
                }
                MediaPayload::Midi(midi) => (
                    image::Payload::Midi(
                        midi.events
                            .into_iter()
                            .map(|event| {
                                Ok(MidiEventState {
                                    frame: u32::try_from(event.frame)?,
                                    data: event.data.into(),
                                })
                            })
                            .collect::<Result<Vec<_>>>()?
                            .into(),
                    ),
                    midi.start_state,
                ),
            };
            Ok((id, (payload, initial)))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    let mut used_media = BTreeSet::new();
    let doc = bundle.document;
    let fx_states = doc
        .fx_states
        .iter()
        .map(|state| {
            Ok((
                state.id,
                canonical_state(state.chain_type, &state.internal_state)?,
            ))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    let mut used_states = BTreeSet::new();
    let preview_bus = match doc.preview_bus {
        shoop_session::PreviewBusDocument::Automatic => {
            let mut masters = doc.buses.iter().filter(|b| b.name == "Master");
            let first = masters.next().map(|b| BusId::from_raw(b.id));
            if masters.next().is_none() {
                first
            } else {
                None
            }
        }
        shoop_session::PreviewBusDocument::Disabled => None,
        shoop_session::PreviewBusDocument::Bus(id) => Some(BusId::from_raw(id)),
    };
    let bus_display_order = doc
        .bus_display_order
        .iter()
        .map(|id| BusId::from_raw(*id))
        .collect();
    let mut buses = Vec::new();
    for bus in doc.buses {
        let mut ports = bus.ports;
        let mut channels = Vec::new();
        for channel in bus.channels {
            let position = ports
                .iter()
                .position(|p| p.id == channel.output_port_id)
                .ok_or_else(|| anyhow!("missing bus output port"))?;
            channels.push(image::BusChannel {
                id: BusChannelId::from_raw(channel.id),
                label: channel.label,
                port: port_from_document(
                    ports.remove(position),
                    ApplicationPortOwner::Bus {
                        bus_id: BusId::from_raw(bus.id),
                    },
                )?,
            });
        }
        ensure!(ports.is_empty(), "unsupported extra bus ports");
        buses.push(image::Bus {
            id: BusId::from_raw(bus.id),
            name: bus.name,
            channels,
            gain_db: bus.gain_db,
            balance: bus.balance,
            muted: bus.muted,
        });
    }
    let mut payloads = Vec::new();
    let mut tracks = Vec::new();
    for track in doc.track_groups.into_iter().flat_map(|group| group.tracks) {
        let (topology, audio_channels, midi) = image_topology(&track.topology)?;
        let audio_count = match &topology {
            TrackTopology::Direct => audio_channels,
            TrackTopology::DryWet {
                dry_audio_channels,
                wet_audio_channels,
                ..
            } => dry_audio_channels + wet_audio_channels,
        } as usize;
        let track_id = TrackId::from_raw(track.id);
        let ports = track
            .ports
            .into_iter()
            .map(|p| {
                port_from_document(
                    p,
                    ApplicationPortOwner::Track {
                        track_id,
                        kind: if track.is_sync {
                            TrackPortOwnerKind::Sync
                        } else {
                            TrackPortOwnerKind::Main
                        },
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut loops = Vec::new();
        for content in track.loops {
            let mut audio = Vec::<image::Channel>::new();
            let mut midi_channels = Vec::<image::MidiChannel>::new();
            let mut recorded_id = None::<Option<u64>>;
            for channel in content.channels {
                let mode = mode_from_document(channel.mode)?;
                if mode == image::ChannelMode::Wet {
                    if let Some(previous) = recorded_id {
                        ensure!(
                            previous == channel.recording_fx_state_id,
                            "wet channels disagree about recorded state"
                        );
                    }
                    recorded_id = Some(channel.recording_fx_state_id);
                } else {
                    ensure!(
                        channel.recording_fx_state_id.is_none(),
                        "only wet audio can carry recorded state"
                    );
                }
                let is_midi = channel.data_type == DataTypeDocument::Midi;
                let expected = connected_ports(
                    &ports,
                    is_midi,
                    if is_midi {
                        midi_channels
                            .iter()
                            .filter(|m| m.channel.mode == mode)
                            .count()
                    } else {
                        audio.iter().filter(|c| c.mode == mode).count()
                    },
                    mode,
                );
                ensure!(
                    channel.connected_port_ids == expected,
                    "loop {} channel {} requires unsupported channel routing",
                    content.id,
                    channel.id
                );
                let mut initial = Vec::new();
                let object = if let Some(id) = &channel.media_id {
                    let (payload, state) =
                        media.get(id).ok_or_else(|| anyhow!("missing media {id}"))?;
                    ensure!(
                        matches!(payload, image::Payload::Midi(_)) == is_midi,
                        "media type mismatch"
                    );
                    used_media.insert(id.clone());
                    initial = state.clone();
                    payloads.push(payload.clone());
                    Some(payloads.len() as u32)
                } else {
                    None
                };
                let channel = image::Channel {
                    mode,
                    object,
                    frames: channel.data_length_frames,
                    gain: channel.gain,
                    start_offset: channel.start_offset_frames,
                    capture_alignment: channel.capture_alignment_frames,
                    preplay: channel.preplay_frames,
                    recording_started_at: channel.recording_started_at,
                };
                if is_midi {
                    midi_channels.push(image::MidiChannel { channel, initial });
                } else {
                    audio.push(channel);
                }
            }
            ensure!(
                audio.len() == audio_count && midi_channels.len() == usize::from(midi),
                "loop {} has unsupported channel shape",
                content.id
            );
            audio.sort_by_key(|c| c.mode as u8);
            let recorded_processor_state = recorded_id
                .flatten()
                .map(|id| {
                    used_states.insert(id);
                    fx_states
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| anyhow!("missing recorded processor state"))
                })
                .transpose()?;
            loops.push(image::Loop {
                recorded_processor_state,
                id: LoopId::from_raw(content.id),
                name: content.name,
                sync: content.is_sync,
                frames: content.length_frames,
                gain: content.gain,
                balance: content.balance,
                audio,
                midi: midi_channels,
            });
        }
        let c = track.controls;
        tracks.push(image::Track {
            topology,
            default_playback_mode: match track.default_playback_mode {
                DefaultPlaybackModeDocument::Regular => DefaultPlaybackMode::Regular,
                DefaultPlaybackModeDocument::DryThroughWet => DefaultPlaybackMode::DryThroughWet,
            },
            processor_state: track
                .fx_chain
                .as_ref()
                .map(|chain| canonical_state(chain.chain_type, &chain.internal_state))
                .transpose()?,
            builtin_fx_midi_cc_assignments: track
                .fx_chain
                .as_ref()
                .into_iter()
                .flat_map(|chain| &chain.builtin_fx_midi_cc_assignments)
                .map(|a| BuiltInFxMidiCcAssignment {
                    parameter: fx_from_document(a.parameter),
                    channel: a.channel,
                    controller: a.controller,
                })
                .collect(),
            oxisynth_midi_cc_assignments: track
                .fx_chain
                .as_ref()
                .into_iter()
                .flat_map(|chain| &chain.midi_cc_assignments)
                .map(|a| OxiSynthMidiCcAssignment {
                    parameter: synth_from_document(a.parameter),
                    channel: a.channel,
                    controller: a.controller,
                })
                .collect(),
            id: track_id,
            name: track.name,
            port_name_base: track.port_name_base,
            sync: track.is_sync,
            audio_channels,
            midi,
            controls: image::Controls {
                input_gain_db: c.input_gain_db,
                input_balance: c.input_balance,
                monitoring: c.input_monitoring,
                output_gain_db: c.output_gain_db,
                output_balance: c.output_balance,
                muted: c.output_muted,
            },
            ports,
            loops,
        });
    }
    ensure!(
        used_states.len() == fx_states.len(),
        "unreferenced recorded processor state"
    );
    ensure!(
        used_media.len() == media.len(),
        "unreferenced media cannot be preserved"
    );
    ensure!(
        doc.global_ports.len() <= 1
            && doc
                .global_ports
                .iter()
                .all(|p| p.name == "Global FX Control MIDI In"
                    && p.data_type == DataTypeDocument::Midi
                    && p.direction == PortDirectionDocument::Input
                    && p.role == PortRoleDocument::MidiInput),
        "unsupported global port model (P08)"
    );
    let mut global_ports = doc
        .global_ports
        .into_iter()
        .map(|p| port_from_document(p, ApplicationPortOwner::GlobalFxControl))
        .collect::<Result<Vec<_>>>()?;
    if global_ports.is_empty() {
        let used = tracks
            .iter()
            .flat_map(|t| &t.ports)
            .map(|p| p.state.id.raw())
            .chain(
                buses
                    .iter()
                    .flat_map(|b| b.channels.iter().map(|c| c.port.state.id.raw())),
            )
            .collect::<BTreeSet<_>>();
        let id = (1..)
            .find(|id| !used.contains(id))
            .ok_or_else(|| anyhow!("port identity space exhausted"))?;
        global_ports.push(image::Port {
            state: ApplicationPortState {
                publication: None,
                id: PortId::from_raw(id),
                owner: ApplicationPortOwner::GlobalFxControl,
                name: "Global FX Control MIDI In".into(),
                data_type: PortDataType::Midi,
                direction: PortDirection::Input,
                role: PortRole::MidiInput,
                connection_policy: ConnectionPolicy::UserManaged,
            },
            external_connections: Vec::new(),
        });
    }
    let g = doc.global;
    let image = Arc::new(image::Image {
        manifest: image::Manifest::Session(image::Session {
            sample_rate: doc.sample_rate,
            global: GlobalControlState {
                default_recording_action: DefaultRecordingAction::Record,
                play_after_record: g.play_after_record,
                sync: g.sync,
                solo: g.solo,
                auto_mute_other_track_inputs: g.auto_mute_other_track_inputs,
                auto_arm_track_inputs: g.auto_arm_track_inputs,
                apply_n_cycles: g.apply_n_cycles,
            },
            tracks,
            buses,
            bus_display_order,
            preview_bus,
            global_ports,
            routes: doc
                .mixer_routes
                .into_iter()
                .map(|r| MixerRouteState {
                    source_port_id: PortId::from_raw(r.source_port_id),
                    destination_channel_id: BusChannelId::from_raw(r.destination_channel_id),
                })
                .collect(),
            selected: doc
                .selected_loop_ids
                .into_iter()
                .map(LoopId::from_raw)
                .collect(),
            target: doc.targeted_loop_id.map(LoopId::from_raw),
        }),
        payloads,
    });
    image
        .validate()
        .map_err(|r| anyhow!("invalid or oversized core session: {r:?}"))?;
    Ok(image)
}
