use anyhow::{anyhow, bail, ensure, Result};
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
            _ => bail!("unsupported persisted port role"),
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
                _ => bail!(
                    "port {} requires unsupported send/return/internal routing",
                    port.id
                ),
            },
            connection_policy: ConnectionPolicy::UserManaged,
        },
        external_connections: port.external_connections,
    })
}
fn connected_ports(ports: &[image::Port], midi: bool, index: usize) -> Vec<u64> {
    if midi {
        ports
            .iter()
            .filter(|p| matches!(p.state.role, PortRole::MidiInput | PortRole::MidiOutput))
            .map(|p| p.state.id.raw())
            .collect()
    } else {
        [PortRole::AudioInput, PortRole::AudioOutput]
            .into_iter()
            .filter_map(|role| ports.iter().filter(|p| p.state.role == role).nth(index))
            .map(|p| p.state.id.raw())
            .collect()
    }
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
        mode: ChannelModeDocument::Direct,
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
    for track in &session.tracks {
        let mut loops = Vec::new();
        for content in &track.loops {
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
                channels.push(channel_document(
                    channel,
                    next_channel,
                    false,
                    connected_ports(&track.ports, false, index),
                    media_id,
                ));
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
                    connected_ports(&track.ports, true, index),
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
            topology: if track.audio_channels == 0 && !track.midi {
                TrackTopologyDocument::Trigger
            } else {
                TrackTopologyDocument::Direct {
                    audio_channels: track.audio_channels,
                    midi: track.midi,
                }
            },
            default_playback_mode: DefaultPlaybackModeDocument::Regular,
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
            fx_chain: None,
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
    let master = &session.master;
    doc.buses = vec![BusDocument {
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
    }];
    doc.bus_display_order = vec![master.id.raw()];
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

pub(super) fn from_bundle(mut bundle: SessionBundle) -> Result<Arc<image::Image>> {
    validate_bundle(&bundle)?;
    let doc = &bundle.document;
    let mut unsupported = BTreeSet::new();
    if !doc.scripts.is_empty() || !bundle.scripts.is_empty() {
        unsupported.insert("session scripts/resources (P16)");
    }
    if !doc.fx_states.is_empty() {
        unsupported.insert("recorded FX states (P06)");
    }
    if !doc.midi_control.bindings.is_empty() || !doc.settings.is_empty() {
        unsupported.insert("reserved MIDI bindings/session settings");
    }
    if doc.global.default_recording_action != RecordingActionDocument::Record {
        unsupported.insert("default grab recording (P04)");
    }
    if doc.buses.len() != 1
        || doc
            .buses
            .first()
            .is_none_or(|b| b.name != MASTER_BUS_NAME || b.channels.len() != 2)
    {
        unsupported.insert("additional or nonstandard buses (P13)");
    }
    if doc.buses.iter().any(|b| b.fx_chain.is_some()) {
        unsupported.insert("bus FX (P07)");
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
            if !matches!(
                track.topology,
                TrackTopologyDocument::Direct {
                    audio_channels: 0..=2,
                    ..
                } | TrackTopologyDocument::Trigger
            ) {
                unsupported.insert("FX/dry-wet or wider track topology (P03/P06)");
            }
            if track.fx_chain.is_some() {
                unsupported.insert("track FX (P06)");
            }
            if track.width.is_some() {
                unsupported.insert("stored track width");
            }
            if track.latency != TrackLatencyDocument::default() {
                unsupported.insert("latency compensation/take alignment (P09/P10)");
            }
            if track.default_playback_mode != DefaultPlaybackModeDocument::Regular {
                unsupported.insert("dry/wet default playback (P03)");
            }
            for content in &track.loops {
                if content.composite.is_some() {
                    unsupported.insert("composite loops (P08)");
                }
                for channel in &content.channels {
                    if channel.mode != ChannelModeDocument::Direct || channel.preplay_frames != 0 {
                        unsupported.insert("channel mode/preplay (P03/P12)");
                    }
                    if channel.recording_fx_state_id.is_some() {
                        unsupported.insert("recorded FX states (P06)");
                    }
                }
            }
        }
    }
    ensure!(
        unsupported.is_empty(),
        "Session requires unsupported features: {}",
        unsupported.into_iter().collect::<Vec<_>>().join(", ")
    );
    let doc = bundle.document;
    let master = doc.buses.into_iter().next().unwrap();
    ensure!(
        doc.bus_display_order == [master.id],
        "unsupported bus display order"
    );
    let mut master_ports = master.ports;
    let mut channels = Vec::new();
    for (index, channel) in master.channels.into_iter().enumerate() {
        ensure!(
            channel.label == MASTER_BUS_CHANNEL_LABELS[index],
            "unsupported Master channel labels"
        );
        let position = master_ports
            .iter()
            .position(|p| p.id == channel.output_port_id)
            .ok_or_else(|| anyhow!("missing Master port"))?;
        channels.push(image::MasterChannel {
            id: BusChannelId::from_raw(channel.id),
            label: channel.label,
            port: port_from_document(
                master_ports.remove(position),
                ApplicationPortOwner::Bus {
                    bus_id: BusId::from_raw(master.id),
                },
            )?,
        });
    }
    ensure!(master_ports.is_empty(), "unsupported extra Master ports");
    let mut payloads = Vec::new();
    let mut tracks = Vec::new();
    for track in doc.track_groups.into_iter().flat_map(|group| group.tracks) {
        let (audio_channels, midi) = match track.topology {
            TrackTopologyDocument::Direct {
                audio_channels,
                midi,
            } => (audio_channels, midi),
            TrackTopologyDocument::Trigger => (0, false),
            _ => unreachable!(),
        };
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
            let mut audio = Vec::new();
            let mut midi_channels = Vec::new();
            for channel in content.channels {
                let is_midi = channel.data_type == DataTypeDocument::Midi;
                let expected = connected_ports(
                    &ports,
                    is_midi,
                    if is_midi {
                        midi_channels.len()
                    } else {
                        audio.len()
                    },
                );
                ensure!(
                    channel.connected_port_ids == expected,
                    "loop {} channel {} requires unsupported channel routing",
                    content.id,
                    channel.id
                );
                let mut initial = Vec::new();
                let object = if let Some(id) = &channel.media_id {
                    let payload = bundle
                        .media
                        .remove(id)
                        .ok_or_else(|| anyhow!("missing or reused media {id}"))?;
                    payloads.push(match payload {
                        MediaPayload::Audio(audio) if !is_midi => {
                            image::Payload::Audio(audio.samples.into())
                        }
                        MediaPayload::Midi(midi) if is_midi => {
                            initial = midi.start_state;
                            ensure!(midi.events.len() <= 1_000_000, "MIDI event budget exceeded");
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
                            )
                        }
                        _ => bail!("media type mismatch"),
                    });
                    Some(payloads.len() as u32)
                } else {
                    None
                };
                let channel = image::Channel {
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
                audio.len() == audio_channels as usize && midi_channels.len() == usize::from(midi),
                "loop {} has unsupported channel shape",
                content.id
            );
            loops.push(image::Loop {
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
        bundle.media.is_empty(),
        "unreferenced media cannot be preserved"
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
            .chain(channels.iter().map(|c| c.port.state.id.raw()))
            .collect::<BTreeSet<_>>();
        let id = (1..)
            .find(|id| !used.contains(id))
            .ok_or_else(|| anyhow!("port identity space exhausted"))?;
        global_ports.push(image::Port {
            state: ApplicationPortState {
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
            master: image::Master {
                id: BusId::from_raw(master.id),
                name: master.name,
                gain_db: master.gain_db,
                balance: master.balance,
                muted: master.muted,
                channels,
            },
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
