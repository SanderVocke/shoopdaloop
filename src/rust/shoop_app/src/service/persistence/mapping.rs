use super::*;
use shoop_app_protocol::persistence_image as image;

pub(in crate::service) struct Candidate {
    tracks: BTreeMap<TrackId, Track>,
    order: Vec<TrackId>,
    loops: BTreeMap<LoopId, Loop>,
    ports: BTreeMap<PortId, Port>,
    channels: BTreeMap<BusChannelId, BackendBusChannelId>,
    connections: BTreeMap<(PortId, String), Intent>,
    routes: BTreeMap<MixerRouteState, Intent>,
    global: GlobalControlState,
    annotations: BTreeMap<(LoopId, bool, u32), (u64, Option<String>)>,
}
fn next(counter: &mut u64) -> Result<u64> {
    let id = *counter;
    *counter = id
        .checked_add(1)
        .ok_or_else(|| anyhow!("session identity space exhausted"))?;
    Ok(id)
}
fn backend_port(port: &image::Port, owner: BackendPortOwner) -> Result<BackendSessionPort> {
    let state = &port.state;
    if !state.id.is_valid()
        || state.name.len() > 128
        || state.connection_policy != ConnectionPolicy::UserManaged
        || port.external_connections.len() > 4096
        || port
            .external_connections
            .iter()
            .any(|name| name.len() > 512)
    {
        return Err(anyhow!("invalid or unsupported persisted port"));
    }
    Ok(BackendSessionPort {
        source_id: state.id.raw(),
        descriptor: BackendPortDescriptor {
            id: BackendPortId::from_raw(state.id.raw()),
            owner,
            name: state.name.clone(),
            data_type: match state.data_type {
                PortDataType::Audio => BackendPortDataType::Audio,
                PortDataType::Midi => BackendPortDataType::Midi,
            },
            direction: match state.direction {
                PortDirection::Input => BackendPortDirection::Input,
                PortDirection::Output => BackendPortDirection::Output,
            },
            role: match state.role {
                PortRole::AudioInput => BackendPortRole::AudioInput,
                PortRole::AudioOutput => BackendPortRole::AudioOutput,
                PortRole::MidiInput => BackendPortRole::MidiInput,
                PortRole::MidiOutput => BackendPortRole::MidiOutput,
                _ => return Err(anyhow!("session requires unsupported port roles")),
            },
        },
        external_connections: port.external_connections.clone(),
    })
}
fn audio_samples(image: &image::Image, channel: &image::Channel) -> Result<Vec<f32>> {
    match channel.object {
        Some(object) => match image.payloads.get(object as usize - 1) {
            Some(image::Payload::Audio(samples)) => Ok(samples.to_vec()),
            _ => Err(anyhow!("missing audio payload")),
        },
        None => Ok(Vec::new()),
    }
}
fn midi_events(image: &image::Image, channel: &image::Channel) -> Result<Vec<BackendMidiEvent>> {
    match channel.object {
        Some(object) => match image.payloads.get(object as usize - 1) {
            Some(image::Payload::Midi(events)) => Ok(events
                .iter()
                .map(|event| BackendMidiEvent {
                    time: event.frame,
                    data: event.data.to_vec(),
                })
                .collect()),
            _ => Err(anyhow!("missing MIDI payload")),
        },
        None => Ok(Vec::new()),
    }
}
fn backend_loop(image: &image::Image, content: &image::Loop) -> Result<BackendLoopContent> {
    Ok(BackendLoopContent {
        source_id: content.id.raw(),
        length: content.frames as u32,
        gain: content.gain,
        balance: content.balance,
        audio: content
            .audio
            .iter()
            .map(|channel| {
                Ok(BackendAudioContent {
                    mode: BackendChannelMode::Direct,
                    samples: audio_samples(image, channel)?,
                    gain: channel.gain,
                    start_offset: channel.start_offset as i32,
                    capture_alignment_frames: channel.capture_alignment as i32,
                    preplay: channel.preplay as u32,
                })
            })
            .collect::<Result<_>>()?,
        midi: content
            .midi
            .iter()
            .map(|channel| {
                if channel.channel.gain != 1. {
                    return Err(anyhow!("MIDI channel gain is unsupported"));
                }
                Ok(BackendMidiContent {
                    mode: BackendChannelMode::Direct,
                    length: channel.channel.frames as u32,
                    start_state: channel.initial.clone(),
                    events: midi_events(image, &channel.channel)?,
                    start_offset: channel.channel.start_offset as i32,
                    capture_alignment_frames: channel.channel.capture_alignment as i32,
                    preplay: channel.channel.preplay as u32,
                })
            })
            .collect::<Result<_>>()?,
    })
}
fn controls(state: &image::Controls, channels: u32, midi: bool) -> TrackControlState {
    TrackControlState {
        has_output: channels > 0 || midi,
        has_output_audio: channels > 0,
        output_stereo: channels == 2,
        output_gain_db: state.output_gain_db,
        output_balance: state.output_balance,
        output_muted: state.muted,
        has_input: channels > 0 || midi,
        has_input_audio: channels > 0,
        input_stereo: channels == 2,
        input_gain_db: state.input_gain_db,
        input_balance: state.input_balance,
        input_monitoring: state.monitoring,
        ..Default::default()
    }
}
impl Candidate {
    fn port(
        &mut self,
        service: &mut Service,
        source: &image::Port,
        owner: ApplicationPortOwner,
        identities: &mut BTreeMap<PortId, PortId>,
    ) -> Result<PortId> {
        if identities.contains_key(&source.state.id) {
            return Err(anyhow!("duplicate session port identity"));
        }
        let id = PortId::from_raw(next(&mut service.next_port)?);
        identities.insert(source.state.id, id);
        let mut view = source.state.clone();
        view.id = id;
        view.owner = owner;
        self.ports.insert(
            id,
            Port {
                backend: BackendPortId::from_raw(source.state.id.raw()),
                view,
            },
        );
        for external in &source.external_connections {
            let error = (!service
                .observed
                .connections
                .host_ports
                .contains_key(external))
            .then(|| {
                format!("external port {external} is unavailable; connection intent retained")
            });
            self.connections.insert(
                (id, external.clone()),
                Intent {
                    connected: true,
                    pending: None,
                    error,
                },
            );
        }
        Ok(id)
    }
    pub(super) fn prepare(
        service: &mut Service,
        image: &image::Image,
        session: &image::Session,
    ) -> Result<(Self, BackendSessionData, BTreeMap<u64, u64>)> {
        if session.sample_rate != service.observed.status.sample_rate
            || session.global.default_recording_action != DefaultRecordingAction::Record
        {
            return Err(anyhow!(
                "session requires a different sample rate or unsupported recording policy"
            ));
        }
        if session.master.name != MASTER_BUS_NAME
            || !session.master.id.is_valid()
            || !session.master.gain_db.is_finite()
            || !(-1.0..=1.0).contains(&session.master.balance)
            || session.master.channels.len() != 2
            || session.global_ports.len() != 1
        {
            return Err(anyhow!(
                "session requires unsupported buses or global ports"
            ));
        }
        let mut candidate = Self {
            tracks: BTreeMap::new(),
            order: Vec::new(),
            loops: BTreeMap::new(),
            ports: BTreeMap::new(),
            channels: BTreeMap::new(),
            connections: BTreeMap::new(),
            routes: BTreeMap::new(),
            global: session.global.clone(),
            annotations: BTreeMap::new(),
        };
        let mut track_ids = BTreeSet::new();
        let mut loop_ids = BTreeMap::new();
        let mut port_ids = BTreeMap::new();
        let mut tracks = Vec::new();
        let mut sync_source = None;
        for track in &session.tracks {
            if !track.id.is_valid()
                || !track_ids.insert(track.id)
                || track.name.len() > 128
                || track.port_name_base.len() > 128
                || track.audio_channels > 2
                || (track.sync && (track.loops.len() != 1 || sync_source.is_some()))
            {
                return Err(anyhow!("invalid or unsupported session track"));
            }
            if ![
                track.controls.input_gain_db,
                track.controls.output_gain_db,
                track.controls.input_balance,
                track.controls.output_balance,
            ]
            .iter()
            .all(|value| value.is_finite())
            {
                return Err(anyhow!("track controls must be finite"));
            }
            let id = TrackId::from_raw(next(&mut service.next_track)?);
            let mut ports = Vec::new();
            for port in &track.ports {
                if port.state.owner
                    != (ApplicationPortOwner::Track {
                        track_id: track.id,
                        kind: if track.sync {
                            TrackPortOwnerKind::Sync
                        } else {
                            TrackPortOwnerKind::Main
                        },
                    })
                {
                    return Err(anyhow!("track port ownership is inconsistent"));
                }
                ports.push(candidate.port(
                    service,
                    port,
                    ApplicationPortOwner::Track {
                        track_id: id,
                        kind: if track.sync {
                            TrackPortOwnerKind::Sync
                        } else {
                            TrackPortOwnerKind::Main
                        },
                    },
                    &mut port_ids,
                )?);
            }
            let mut loops = Vec::new();
            for content in &track.loops {
                if content.sync != track.sync
                    || content.audio.len() != track.audio_channels as usize
                    || content.midi.len() != usize::from(track.midi)
                {
                    return Err(anyhow!("session loop shape does not match its track"));
                }
                let live = LoopId::from_raw(next(&mut service.next_loop)?);
                if loop_ids.insert(content.id, live).is_some() {
                    return Err(anyhow!("duplicate session loop identity"));
                }
                if track.sync {
                    sync_source = Some(content.id.raw());
                }
                loops.push(live);
                candidate.loops.insert(
                    live,
                    Loop {
                        backend: BackendLoopId::from_raw(content.id.raw()),
                        track: id,
                        view: LoopState {
                            id: live,
                            name: content.name.clone(),
                            structural_state: StructuralState::Confirmed,
                            length_frames: content.frames,
                            empty: content.frames == 0,
                            mode: LoopMode::Stopped,
                            sync: content.sync,
                            has_audio: track.audio_channels > 0,
                            has_midi: track.midi,
                            show_gain: track.audio_channels > 0,
                            stereo: track.audio_channels == 2,
                            gain: content.gain,
                            balance: content.balance,
                            ..Default::default()
                        },
                    },
                );
                for (midi, channels) in [
                    (false, content.audio.iter().collect::<Vec<_>>()),
                    (
                        true,
                        content
                            .midi
                            .iter()
                            .map(|channel| &channel.channel)
                            .collect(),
                    ),
                ] {
                    for (index, channel) in channels.into_iter().enumerate() {
                        candidate.annotations.insert(
                            (live, midi, index as u32),
                            (0, channel.recording_started_at.clone()),
                        );
                    }
                }
            }
            let state = BackendTrackState {
                topology: BackendTrackTopology::Direct {
                    audio_channels: track.audio_channels,
                    midi: track.midi,
                },
                audio_channels: track.audio_channels,
                midi: track.midi,
                output_gain_db: track.controls.output_gain_db,
                output_balance: track.controls.output_balance,
                output_muted: track.controls.muted,
                input_gain_db: track.controls.input_gain_db,
                input_balance: track.controls.input_balance,
                input_monitoring: track.controls.monitoring,
                ..Default::default()
            };
            tracks.push(BackendSessionTrack {
                source_id: track.id.raw(),
                port_name_base: track.port_name_base.clone(),
                topology: state.topology.clone(),
                state,
                loops: track
                    .loops
                    .iter()
                    .map(|content| backend_loop(image, content))
                    .collect::<Result<_>>()?,
                ports: track
                    .ports
                    .iter()
                    .map(|port| backend_port(port, BackendPortOwner::Track))
                    .collect::<Result<_>>()?,
                processor_state: None,
                builtin_fx_midi_cc_assignments: Vec::new(),
                oxisynth_midi_cc_assignments: Vec::new(),
            });
            candidate.tracks.insert(
                id,
                Track {
                    backend: BackendTrackId::from_raw(track.id.raw()),
                    audio_channels: track.audio_channels,
                    midi: track.midi,
                    monitoring_intent: None,
                    view: TrackState {
                        id,
                        name: track.name.clone(),
                        is_sync: track.sync,
                        structural_state: StructuralState::Confirmed,
                        controls: controls(&track.controls, track.audio_channels, track.midi),
                        port_ids: ports.into(),
                        ..Default::default()
                    },
                    loops,
                },
            );
            candidate.order.push(id);
        }
        for selected in &session.selected {
            candidate
                .loops
                .get_mut(
                    loop_ids
                        .get(selected)
                        .ok_or_else(|| anyhow!("selection references a missing loop"))?,
                )
                .unwrap()
                .view
                .selected = true;
        }
        if let Some(target) = session.target {
            candidate
                .loops
                .get_mut(
                    loop_ids
                        .get(&target)
                        .ok_or_else(|| anyhow!("target references a missing loop"))?,
                )
                .unwrap()
                .view
                .targeted = true;
        }
        let mut channels = Vec::new();
        let mut channel_ids = BTreeMap::new();
        for (index, channel) in session.master.channels.iter().enumerate() {
            if !channel.id.is_valid()
                || channel.port.state.owner
                    != (ApplicationPortOwner::Bus {
                        bus_id: session.master.id,
                    })
                || channel.label != MASTER_BUS_CHANNEL_LABELS[index]
                || channel_ids
                    .insert(channel.id, BusChannelId::from_raw(index as u64 + 1))
                    .is_some()
            {
                return Err(anyhow!("invalid Master channel"));
            }
            candidate.port(
                service,
                &channel.port,
                ApplicationPortOwner::Bus {
                    bus_id: BusId::from_raw(1),
                },
                &mut port_ids,
            )?;
            let id = BusChannelId::from_raw(index as u64 + 1);
            candidate
                .channels
                .insert(id, BackendBusChannelId::from_raw(channel.id.raw()));
            channels.push(BackendSessionBusChannel {
                source_id: channel.id.raw(),
                label: channel.label.clone(),
                output_port: backend_port(
                    &channel.port,
                    BackendPortOwner::Bus(BackendBusId::from_raw(session.master.id.raw())),
                )?,
            });
        }
        let mut global_ports = Vec::new();
        for port in &session.global_ports {
            if port.state.owner != ApplicationPortOwner::GlobalFxControl {
                return Err(anyhow!("invalid global port owner"));
            }
            candidate.port(
                service,
                port,
                ApplicationPortOwner::GlobalFxControl,
                &mut port_ids,
            )?;
            global_ports.push(backend_port(port, BackendPortOwner::GlobalFxControl)?);
        }
        for route in &session.routes {
            let source = *port_ids
                .get(&route.source_port_id)
                .ok_or_else(|| anyhow!("route references a missing source"))?;
            let destination = *channel_ids
                .get(&route.destination_channel_id)
                .ok_or_else(|| anyhow!("route references a missing destination"))?;
            candidate.routes.insert(
                MixerRouteState {
                    source_port_id: source,
                    destination_channel_id: destination,
                },
                Intent {
                    connected: true,
                    pending: None,
                    error: None,
                },
            );
        }
        let sync = if session.global.sync {
            sync_source.map_or_else(BTreeMap::new, |source| {
                session
                    .tracks
                    .iter()
                    .filter(|track| !track.sync)
                    .flat_map(|track| {
                        track
                            .loops
                            .iter()
                            .map(move |content| (content.id.raw(), source))
                    })
                    .collect()
            })
        } else {
            BTreeMap::new()
        };
        Ok((
            candidate,
            BackendSessionData {
                sample_rate: session.sample_rate,
                tracks,
                buses: vec![BackendSessionBus {
                    source_id: session.master.id.raw(),
                    name: session.master.name.clone(),
                    channels,
                    gain_db: session.master.gain_db,
                    balance: session.master.balance,
                    muted: session.master.muted,
                    processor_type: None,
                    processor_state: None,
                    builtin_fx_midi_cc_assignments: Vec::new(),
                }],
                mixer_routes: session
                    .routes
                    .iter()
                    .map(|route| BackendSessionMixerRoute {
                        source_port_id: route.source_port_id.raw(),
                        destination_channel_id: route.destination_channel_id.raw(),
                    })
                    .collect(),
                global_ports,
                use_legacy_browser_default_routes: false,
            },
            sync,
        ))
    }
    pub(super) fn publish(
        mut self,
        service: &mut Service,
        mapping: &BackendSessionReplacement,
    ) -> Result<()> {
        let epoch = service
            .persistence_epoch
            .checked_add(1)
            .ok_or_else(|| anyhow!("session epoch exhausted"))?;
        for track in self.tracks.values_mut() {
            track.backend = mapping
                .tracks
                .get(&track.backend.raw())
                .ok_or_else(|| anyhow!("backend omitted track mapping"))?
                .track_id;
        }
        for entry in self.loops.values_mut() {
            entry.backend = *mapping
                .loops
                .get(&entry.backend.raw())
                .ok_or_else(|| anyhow!("backend omitted loop mapping"))?;
        }
        for port in self.ports.values_mut() {
            port.backend = *mapping
                .ports
                .get(&port.backend.raw())
                .or_else(|| mapping.global_ports.get(&port.backend.raw()))
                .ok_or_else(|| anyhow!("backend omitted port mapping"))?;
        }
        for channel in self.channels.values_mut() {
            *channel = *mapping
                .bus_channels
                .get(&channel.raw())
                .ok_or_else(|| anyhow!("backend omitted Master mapping"))?;
        }
        service.tracks = self.tracks;
        service.track_order = self.order;
        service.loops = self.loops;
        service.ports = self.ports;
        service.channels = self.channels;
        service.connection_intent = self.connections;
        service.route_intent = self.routes;
        service.global = self.global;
        service.channel_annotations = self.annotations;
        service.piano_notes.clear();
        service.released_notes.clear();
        service.media_loop = None;
        service.media_pending = None;
        service.persistence_epoch = epoch;
        service.bootstrap_track = None;
        service.bootstrap_loop = None;
        Ok(())
    }
}

pub(super) fn update(image: &image::Image) -> Result<BackendLoopContentUpdate> {
    let image::Manifest::Update {
        frames,
        audio,
        midi,
        ..
    } = &image.manifest
    else {
        return Err(anyhow!("expected a loop update"));
    };
    Ok(BackendLoopContentUpdate {
        length: frames.map(|frames| frames as u32),
        audio: audio
            .iter()
            .map(|channel| {
                if channel.content.gain != 1. {
                    return Err(anyhow!("audio imports preserve the existing channel gain"));
                }
                Ok(BackendAudioChannelUpdate {
                    channel: channel.channel as usize,
                    samples: audio_samples(image, &channel.content)?,
                    start_offset: Some(channel.content.start_offset as i32),
                    capture_alignment_frames: Some(channel.content.capture_alignment as i32),
                    preplay: Some(channel.content.preplay as u32),
                })
            })
            .collect::<Result<_>>()?,
        midi: midi
            .iter()
            .map(|channel| {
                if channel.content.channel.gain != 1. {
                    return Err(anyhow!("MIDI channel gain is unsupported"));
                }
                Ok(BackendMidiChannelUpdate {
                    channel: channel.channel as usize,
                    length: channel.content.channel.frames as u32,
                    start_state: channel.content.initial.clone(),
                    events: midi_events(image, &channel.content.channel)?,
                    start_offset: Some(channel.content.channel.start_offset as i32),
                    capture_alignment_frames: Some(
                        channel.content.channel.capture_alignment as i32,
                    ),
                    preplay: Some(channel.content.channel.preplay as u32),
                })
            })
            .collect::<Result<_>>()?,
    })
}
