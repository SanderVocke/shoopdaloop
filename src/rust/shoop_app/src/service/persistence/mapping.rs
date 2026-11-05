use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::*;
use shoop_app_protocol::persistence_image as image;

pub(in crate::service) struct Candidate {
    tracks: BTreeMap<TrackId, Track>,
    order: Vec<TrackId>,
    loops: BTreeMap<LoopId, Loop>,
    ports: BTreeMap<PortId, Port>,
    channels: BTreeMap<BusChannelId, BackendBusChannelId>,
    buses: BTreeMap<BusId, Bus>,
    bus_order: Vec<BusId>,
    preview_bus: Option<BusId>,
    connections: BTreeMap<(PortId, String), Intent>,
    routes: BTreeMap<MixerRouteState, Intent>,
    global: GlobalControlState,
    annotations: BTreeMap<(LoopId, bool, u32), (u64, Option<String>)>,
    composites: BTreeMap<LoopId, shoop_app_protocol::composite::Content>,
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
                PortRole::AudioSend => BackendPortRole::AudioSend,
                PortRole::AudioReturn => BackendPortRole::AudioReturn,
                PortRole::MidiSend => BackendPortRole::MidiSend,
            },
        },
        external_connections: port.external_connections.clone(),
    })
}
fn audio_samples(image: &image::Image, channel: &image::Channel) -> Result<Arc<Vec<f32>>> {
    match channel.object {
        Some(object) => match image.payloads.get(object as usize - 1) {
            Some(image::Payload::Audio(samples)) => Ok(Arc::clone(samples)),
            _ => Err(anyhow!("missing audio payload")),
        },
        None => Ok(Arc::new(Vec::new())),
    }
}
fn midi_events(
    image: &image::Image,
    channel: &image::Channel,
) -> Result<Arc<Vec<BackendMidiEvent>>> {
    match channel.object {
        Some(object) => match image.payloads.get(object as usize - 1) {
            Some(image::Payload::Midi(events)) => Ok(Arc::clone(events)),
            _ => Err(anyhow!("missing MIDI payload")),
        },
        None => Ok(Arc::new(Vec::new())),
    }
}

pub(super) fn backend_loop(
    image: &image::Image,
    content: &image::Loop,
) -> Result<BackendLoopContent> {
    Ok(BackendLoopContent {
        recorded_processor_state: content.recorded_processor_state.clone(),
        source_id: content.id.raw(),
        length: if content.composite.is_some() {
            0
        } else {
            u32::try_from(content.frames)?
        },
        gain: content.gain,
        balance: content.balance,
        audio: content
            .audio
            .iter()
            .map(|channel| {
                Ok(BackendAudioContent {
                    mode: backend_mode(channel.mode),
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
                    mode: backend_mode(channel.channel.mode),
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
fn backend_mode(mode: image::ChannelMode) -> BackendChannelMode {
    match mode {
        image::ChannelMode::Direct => BackendChannelMode::Direct,
        image::ChannelMode::Dry => BackendChannelMode::Dry,
        image::ChannelMode::Wet => BackendChannelMode::Wet,
    }
}
fn topology(track: &image::Track) -> BackendTrackTopology {
    match &track.topology {
        TrackTopology::Direct => BackendTrackTopology::Direct {
            audio_channels: track.audio_channels,
            midi: track.midi,
        },
        TrackTopology::DryWet {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
            processor_type,
        } if processor_type.as_str() == TrackProcessorTypeId::EXTERNAL => {
            BackendTrackTopology::DryWetExternal {
                dry_audio_channels: *dry_audio_channels,
                wet_audio_channels: *wet_audio_channels,
                dry_midi: *dry_midi,
            }
        }
        TrackTopology::DryWet {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
            processor_type,
        } => BackendTrackTopology::DryWetProcessor {
            dry_audio_channels: *dry_audio_channels,
            wet_audio_channels: *wet_audio_channels,
            dry_midi: *dry_midi,
            processor_type: processor_type.as_str().into(),
        },
    }
}
fn controls(state: &image::Controls, dry: u32, channels: u32, midi: bool) -> TrackControlState {
    TrackControlState {
        has_output: channels > 0 || midi,
        has_output_audio: channels > 0,
        output_stereo: channels == 2,
        output_gain_db: state.output_gain_db,
        output_balance: state.output_balance,
        output_muted: state.muted,
        has_input: dry > 0 || midi,
        has_input_audio: dry > 0,
        input_stereo: dry == 2,
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
        view.publication = None;
        view.owner = owner;
        self.ports.insert(
            id,
            Port {
                backend: BackendPortId::from_raw(source.state.id.raw()),
                view,
            },
        );
        for external in &source.external_connections {
            let error = (!service.dynamic_ports()
                && !service
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
        let limits = service.backend.bus_limits();
        if session.buses.len() > limits.buses as usize || session.global_ports.len() != 1 {
            return Err(anyhow!(
                "session exceeds bus capacity or has invalid global ports"
            ));
        }
        let mut candidate = Self {
            tracks: BTreeMap::new(),
            order: Vec::new(),
            loops: BTreeMap::new(),
            ports: BTreeMap::new(),
            channels: BTreeMap::new(),
            buses: BTreeMap::new(),
            bus_order: Vec::new(),
            preview_bus: None,
            connections: BTreeMap::new(),
            routes: BTreeMap::new(),
            global: session.global.clone(),
            annotations: BTreeMap::new(),
            composites: BTreeMap::new(),
        };
        let mut track_ids = BTreeSet::new();
        let mut loop_ids = BTreeMap::new();
        let mut port_ids = BTreeMap::new();
        let mut tracks = Vec::new();
        let mut sync_source = None;
        for track in &session.tracks {
            let topology = topology(track);
            let dry = topology.dry_audio_channels();
            let wet = topology.wet_audio_channels();
            let processed = !matches!(track.topology, TrackTopology::Direct);
            let audio_count = if processed { dry + wet } else { wet } as usize;
            let spec_topology = match &track.topology {
                TrackTopology::Direct => TrackSpecTopology::Direct {
                    audio_channels: wet,
                    midi: track.midi,
                },
                TrackTopology::DryWet {
                    dry_audio_channels,
                    wet_audio_channels,
                    dry_midi,
                    processor_type,
                } => TrackSpecTopology::DryWet {
                    dry_audio_channels: *dry_audio_channels,
                    wet_audio_channels: *wet_audio_channels,
                    dry_midi: *dry_midi,
                    processor_type: processor_type.clone(),
                    default_playback_mode: track.default_playback_mode,
                },
            };
            TrackSpec {
                name: "loaded track".into(),
                topology: spec_topology,
                latency: Default::default(),
                initial_output_bus_names: Vec::new(),
                creation_request_id: None,
            }
            .validate(&service.track_processors)
            .map_err(|error| anyhow!("unsupported loaded track: {error:?}"))?;
            if !track.id.is_valid()
                || !track_ids.insert(track.id)
                || track.name.len() > 128
                || track.port_name_base.len() > 128
                || dry > 8
                || wet > 8
                || track.audio_channels != wet
                || track.midi != topology.has_midi()
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
                    || content.composite.is_none()
                        && (content.audio.len() != audio_count
                            || content.midi.len() != usize::from(track.midi)
                            || content.audio.iter().enumerate().any(|(i, c)| {
                                c.mode
                                    != if !processed {
                                        image::ChannelMode::Direct
                                    } else if i < dry as usize {
                                        image::ChannelMode::Dry
                                    } else {
                                        image::ChannelMode::Wet
                                    }
                            })
                            || content.midi.iter().any(|m| {
                                m.channel.mode
                                    != if processed {
                                        image::ChannelMode::Dry
                                    } else {
                                        image::ChannelMode::Direct
                                    }
                            }))
                    || content.composite.is_some() && (track.sync || processed)
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
                if let Some(composite) = &content.composite {
                    if composite.prepared.sample_rate != session.sample_rate
                        || composite.prepared.quantum != service.observed.status.quantum_frames
                    {
                        return Err(anyhow!(
                            "composite timing requires explicit format conversion"
                        ));
                    }
                    candidate.composites.insert(live, composite.clone());
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
                            has_audio: audio_count > 0,
                            has_midi: track.midi,
                            show_gain: audio_count > 0,
                            stereo: track.audio_channels == 2,
                            gain: content.gain,
                            balance: content.balance,
                            has_recorded_fx_state: content.recorded_processor_state.is_some(),
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
                topology,
                default_playback_mode: match track.default_playback_mode {
                    DefaultPlaybackMode::Regular => BackendDefaultPlaybackMode::Regular,
                    DefaultPlaybackMode::DryThroughWet => BackendDefaultPlaybackMode::DryThroughWet,
                },
                audio_channels: track.audio_channels,
                midi: track.midi,
                output_gain_db: track.controls.output_gain_db,
                output_balance: track.controls.output_balance,
                output_muted: track.controls.muted,
                input_gain_db: track.controls.input_gain_db,
                input_balance: track.controls.input_balance,
                input_monitoring: track.controls.monitoring,
                automatic_monitoring_enabled: session.global.auto_arm_track_inputs && !track.sync,
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
                processor_state: track.processor_state.clone(),
                builtin_fx_midi_cc_assignments: track
                    .builtin_fx_midi_cc_assignments
                    .iter()
                    .map(|a| BackendBuiltInFxMidiCcAssignment {
                        parameter: a.parameter,
                        channel: a.channel,
                        controller: a.controller,
                    })
                    .collect(),
                oxisynth_midi_cc_assignments: track
                    .oxisynth_midi_cc_assignments
                    .iter()
                    .map(|a| BackendOxiSynthMidiCcAssignment {
                        parameter: a.parameter,
                        channel: a.channel,
                        controller: a.controller,
                    })
                    .collect(),
            });
            candidate.tracks.insert(
                id,
                Track {
                    backend: BackendTrackId::from_raw(track.id.raw()),
                    audio_channels: track.audio_channels,
                    midi: track.midi,
                    monitoring_intent: None,
                    default_playback_intent: None,
                    view: TrackState {
                        id,
                        name: track.name.clone(),
                        is_sync: track.sync,
                        topology: track.topology.clone(),
                        default_playback_mode: track.default_playback_mode,
                        structural_state: StructuralState::Confirmed,
                        controls: controls(&track.controls, dry, wet, track.midi),
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
        let mut buses = Vec::new();
        let mut bus_ids = BTreeMap::new();
        let mut channel_ids = BTreeMap::new();
        for bus in &session.buses {
            BackendBusRequest {
                name: bus.name.clone(),
                channel_count: bus.channels.len() as u32,
                fx: None,
            }
            .normalized()?;
            if !bus.id.is_valid()
                || !bus.gain_db.is_finite()
                || !(-1.0..=1.0).contains(&bus.balance)
                || (bus.channels.len() != 2 && bus.balance != 0.)
                || bus.channels.len() > limits.channels as usize
            {
                return Err(anyhow!("invalid bus or unsupported channel capacity"));
            }
            let id = BusId::from_raw(next(&mut service.next_bus)?);
            if bus_ids.insert(bus.id, id).is_some() {
                return Err(anyhow!("duplicate bus identity"));
            }
            let mut channels = Vec::new();
            let mut view_channels = Vec::new();
            for channel in &bus.channels {
                let channel_id = BusChannelId::from_raw(next(&mut service.next_channel)?);
                if !channel.id.is_valid()
                    || channel.port.state.owner != (ApplicationPortOwner::Bus { bus_id: bus.id })
                    || channel.label.is_empty()
                    || channel.label.len() > 128
                    || channel_ids.insert(channel.id, channel_id).is_some()
                {
                    return Err(anyhow!("invalid bus channel"));
                }
                let port = candidate.port(
                    service,
                    &channel.port,
                    ApplicationPortOwner::Bus { bus_id: id },
                    &mut port_ids,
                )?;
                candidate
                    .channels
                    .insert(channel_id, BackendBusChannelId::from_raw(channel.id.raw()));
                view_channels.push(BusChannelState {
                    id: channel_id,
                    label: channel.label.clone(),
                    output_port_id: port,
                });
                channels.push(BackendSessionBusChannel {
                    source_id: channel.id.raw(),
                    label: channel.label.clone(),
                    output_port: backend_port(
                        &channel.port,
                        BackendPortOwner::Bus(BackendBusId::from_raw(bus.id.raw())),
                    )?,
                });
            }
            candidate.buses.insert(
                id,
                Bus {
                    backend: BackendBusId::from_raw(bus.id.raw()),
                    view: BusState {
                        id,
                        name: bus.name.clone(),
                        structural_state: StructuralState::Confirmed,
                        structural_error: None,
                        output_peaks_db: vec![-200.; view_channels.len()].into(),
                        channels: view_channels.into(),
                        gain_db: bus.gain_db,
                        balance: bus.balance,
                        muted: bus.muted,
                        control_pending: false,
                        control_error: None,
                        fx: None,
                    },
                },
            );
            buses.push(BackendSessionBus {
                source_id: bus.id.raw(),
                name: bus.name.clone(),
                channels,
                gain_db: bus.gain_db,
                balance: bus.balance,
                muted: bus.muted,
                processor_type: None,
                processor_state: None,
                builtin_fx_midi_cc_assignments: Vec::new(),
            });
        }
        if session.bus_display_order.len() != bus_ids.len()
            || session
                .bus_display_order
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != bus_ids.len()
        {
            return Err(anyhow!("invalid bus display order"));
        }
        for id in &session.bus_display_order {
            candidate.bus_order.push(
                *bus_ids
                    .get(id)
                    .ok_or_else(|| anyhow!("unknown bus order identity"))?,
            );
        }
        candidate.preview_bus = session
            .preview_bus
            .map(|id| {
                bus_ids
                    .get(&id)
                    .copied()
                    .ok_or_else(|| anyhow!("unknown preview bus identity"))
            })
            .transpose()?;
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
        let sync = session
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .filter_map(|content| {
                content
                    .sync_source
                    .map(|source| (content.id.raw(), source.raw()))
            })
            .collect::<BTreeMap<_, _>>();
        for composite in candidate.composites.values_mut() {
            for event in &mut composite.document.instances {
                event.loop_id = *loop_ids
                    .get(&event.loop_id)
                    .ok_or_else(|| anyhow!("unknown persisted composite child"))?;
            }
            for span in &mut composite.prepared.spans {
                span.target = *loop_ids
                    .get(&span.target)
                    .ok_or_else(|| anyhow!("unknown persisted composite target"))?;
            }
        }
        let composite_ids = session
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .filter(|content| content.composite.is_some())
            .map(|content| content.id)
            .collect::<BTreeSet<_>>();
        let composites = session
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .filter_map(|content| {
                content.composite.as_ref().map(|composite| {
                    Ok(BackendSessionComposite {
                        source_id: content.id.raw(),
                        version: 0,
                        plan: BackendPreparedCompositePlan {
                            kind: if composite.prepared.spans.is_empty()
                                || composite.document.kind == CompositeKind::Regular
                            {
                                BackendCompositeKind::Regular
                            } else {
                                BackendCompositeKind::Script
                            },
                            sync_source: content
                                .sync_source
                                .map(|id| BackendLoopId::from_raw(id.raw())),
                            unit_frames: composite.prepared.unit_frames,
                            iterations: composite.prepared.iterations,
                            entries: u32::try_from(composite.document.instances.len())?,
                            spans: composite
                                .prepared
                                .spans
                                .iter()
                                .map(|span| {
                                    Ok(BackendCompositeSpan {
                                        target: if composite_ids.contains(&span.target) {
                                            BackendCompositeTarget::Composite(
                                                BackendCompositeId::from_raw(span.target.raw()),
                                            )
                                        } else {
                                            BackendCompositeTarget::Loop(BackendLoopId::from_raw(
                                                span.target.raw(),
                                            ))
                                        },
                                        start: span.start,
                                        end: span.end,
                                        occurrence_start: span.occurrence_start,
                                        ordinal: span.ordinal,
                                        mode: match span.mode {
                                            LoopMode::Playing => BackendLoopMode::Playing,
                                            LoopMode::Stopped => BackendLoopMode::Stopped,
                                            LoopMode::Recording => BackendLoopMode::Recording,
                                            _ => {
                                                return Err(anyhow!(
                                                    "unsupported persisted composite event mode"
                                                ))
                                            }
                                        },
                                    })
                                })
                                .collect::<Result<_>>()?,
                        },
                    })
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if service.dynamic_ports() {
            for port in tracks
                .iter_mut()
                .flat_map(|track| track.ports.iter_mut())
                .chain(buses.iter_mut().flat_map(|bus| {
                    bus.channels
                        .iter_mut()
                        .map(|channel| &mut channel.output_port)
                }))
                .chain(global_ports.iter_mut())
            {
                port.external_connections
                    .retain(|host| service.observed.connections.host_ports.contains_key(host));
            }
        }
        Ok((
            candidate,
            BackendSessionData {
                composites,
                loop_sync: sync.clone(),
                repeat_sync: session
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .filter(|content| content.repeat_sync)
                    .map(|content| content.id.raw())
                    .collect(),
                sample_rate: session.sample_rate,
                tracks,
                buses,
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
        observed: &BackendSnapshot,
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
        for entry in self.loops.values_mut() {
            entry.view.content_revision = if self.composites.contains_key(&entry.view.id) {
                observed
                    .composites
                    .get(&BackendCompositeId::from_raw(entry.backend.raw()))
                    .ok_or_else(|| anyhow!("backend omitted restored composite"))?
                    .active_plan_version
            } else {
                observed
                    .loops
                    .get(&entry.backend)
                    .ok_or_else(|| anyhow!("backend omitted restored primitive"))?
                    .content_revision
            };
        }
        let mut owners = BTreeMap::new();
        for (id, content) in self.composites {
            let revisions = content
                .document
                .instances
                .iter()
                .map(|event| {
                    (
                        event.loop_id,
                        (
                            self.loops[&event.loop_id].view.content_revision,
                            self.loops[&event.loop_id].view.length_frames,
                        ),
                    )
                })
                .collect();
            let active = shoop_app_protocol::composite::Versioned {
                version: self.loops[&id].view.content_revision,
                document: content.document,
                prepared: content.prepared,
            };
            for event in &active.document.instances {
                service.next_composite_event = service.next_composite_event.max(
                    event
                        .instance_id
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("persisted event identity space exhausted"))?,
                );
            }
            owners.insert(
                id,
                super::super::composites::Owner::restored(active, revisions),
            );
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
                .ok_or_else(|| anyhow!("backend omitted bus channel mapping"))?;
        }
        for bus in self.buses.values_mut() {
            bus.backend = *mapping
                .buses
                .get(&bus.backend.raw())
                .ok_or_else(|| anyhow!("backend omitted bus mapping"))?;
        }
        service.buses = self.buses;
        service.bus_order = self.bus_order;
        service.preview_bus = self.preview_bus;
        service.tracks = self.tracks;
        service.track_order = self.order;
        service.automatic_monitoring_intent.clear();
        service.composites = owners;
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
