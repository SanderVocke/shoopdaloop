use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::*;

impl Service {
    pub fn snapshot(&self) -> Arc<SessionSnapshot> {
        let _span =
            tracing::trace_span!("app.service.snapshot", revision = self.revision.0).entered();
        let tracks = self
            .track_order
            .iter()
            .filter_map(|id| self.tracks.get(id))
            .map(|track| {
                let mut view = track.view.clone();
                view.loops = track
                    .loops
                    .iter()
                    .filter_map(|id| {
                        self.loops.get(id).map(|entry| {
                            let mut view = entry.view.clone();
                            view.play_after_record = self.global.play_after_record;
                            view
                        })
                    })
                    .collect();
                view
            })
            .collect();
        let buses = self
            .observed
            .mixer
            .buses
            .get(&MASTER_BUS_ID)
            .map(|master| BusState {
                id: BusId::from_raw(1),
                name: master.name.clone(),
                structural_state: StructuralState::Confirmed,
                structural_error: None,
                channels: master
                    .channels
                    .iter()
                    .enumerate()
                    .filter_map(|(index, channel)| {
                        Some(BusChannelState {
                            id: BusChannelId::from_raw(index as u64 + 1),
                            label: channel.label.clone(),
                            output_port_id: self.app_port(channel.output_port_id)?,
                        })
                    })
                    .collect::<Vec<_>>()
                    .into(),
                gain_db: master.gain_db,
                balance: master.balance,
                muted: master.muted,
                output_peaks_db: master.output_peaks_db.clone().into(),
                control_pending: false,
                control_error: None,
                fx: None,
            })
            .into_iter()
            .collect::<Vec<_>>()
            .into();
        let connections = self.connections();
        Arc::new(SessionSnapshot {
            revision: self.revision,
            tracks,
            buses,
            global: self.global.clone(),
            master_auto_connect: self.options.master_auto_connect,
            loop_smoothing_ms: self.options.loop_smoothing_ms,
            status: status(&self.observed.status),
            audio_drivers: self.observed.audio_drivers.clone(),
            capabilities: BackendCapabilities::NONE.with(
                BackendCapability::MidiInjection,
                self.backend
                    .capabilities()
                    .supports(BackendCapability::MidiInjection),
            ),
            connections: Arc::new(connections),
            track_processors: Arc::from([]),
            bus_processors: Arc::from([]),
            pending: self
                .pending
                .values()
                .map(|pending| PendingOperation {
                    id: pending.id,
                    accepted_at: pending.accepted_at,
                })
                .collect(),
            results: self.results.iter().cloned().collect(),
        })
    }

    fn connections(&self) -> ConnectionViewState {
        let observed = &self.observed.connections;
        let confirmed_links = observed
            .confirmed_links
            .iter()
            .filter_map(|link| {
                Some(ConfirmedConnectionState {
                    application_port_id: self.app_port(link.application_port_id)?,
                    host_port_id: HostPortId::new(&link.host_port_id),
                })
            })
            .collect::<Vec<_>>();
        let mixer_links: Vec<_> = self
            .observed
            .mixer
            .confirmed_links
            .iter()
            .filter_map(|link| {
                Some(MixerRouteState {
                    source_port_id: self.app_port(link.source_port_id)?,
                    destination_channel_id: self.app_channel(link.destination_channel_id)?,
                })
            })
            .collect();
        let pending_links = self
            .connection_intent
            .iter()
            .filter_map(|((port, host), intent)| {
                let matches = confirmed_links.iter().any(|link| {
                    link.application_port_id == *port && link.host_port_id.as_str() == host
                });
                (matches != intent.connected && intent.pending.is_some()).then(|| {
                    PendingConnectionState {
                        application_port_id: *port,
                        host_port_id: HostPortId::new(host),
                        desired_connected: intent.connected,
                    }
                })
            })
            .collect::<Vec<_>>();
        let pending_mixer_links = self
            .route_intent
            .iter()
            .filter_map(|(route, intent)| {
                (mixer_links.contains(route) != intent.connected && intent.pending.is_some())
                    .then_some(PendingMixerRouteState {
                        route: *route,
                        desired_connected: intent.connected,
                    })
            })
            .collect::<Vec<_>>();
        ConnectionViewState {
            revision: self.revision.0,
            loading: !self.is_ready(),
            backend_available: observed.available,
            application_ports: self
                .ports
                .values()
                .map(|port| port.view.clone())
                .collect::<Vec<_>>()
                .into(),
            host_ports: observed
                .host_ports
                .values()
                .map(|port| HostPortState {
                    id: HostPortId::new(&port.id),
                    name: port.name.clone(),
                    data_type: data_type(port.data_type),
                    direction: direction(port.direction),
                })
                .collect::<Vec<_>>()
                .into(),
            confirmed_links: confirmed_links.into(),
            pending_links: pending_links.into(),
            mixer_links: mixer_links.into(),
            pending_mixer_links: pending_mixer_links.into(),
            errors: self
                .connection_intent
                .iter()
                .filter_map(|((port, host), intent)| {
                    Some(ConnectionErrorState {
                        port_id: Some(*port),
                        external_port: Some(host.clone()),
                        kind: ConnectionErrorKind::BackendRejected,
                        message: intent.error.clone()?,
                    })
                })
                .collect::<Vec<_>>()
                .into(),
            mixer_errors: self
                .route_intent
                .iter()
                .filter_map(|(route, intent)| {
                    Some(MixerRouteErrorState {
                        route: *route,
                        message: intent.error.clone()?,
                    })
                })
                .collect::<Vec<_>>()
                .into(),
        }
    }

    fn app_port(&self, backend: BackendPortId) -> Option<PortId> {
        self.ports
            .iter()
            .find_map(|(id, port)| (port.backend == backend).then_some(*id))
    }
    fn app_channel(&self, backend: BackendBusChannelId) -> Option<BusChannelId> {
        self.channels
            .iter()
            .find_map(|(id, channel)| (*channel == backend).then_some(*id))
    }

    pub(super) fn register_master(&mut self) -> Result<()> {
        let descriptors: Vec<_> = self
            .observed
            .connections
            .application_ports
            .values()
            .cloned()
            .collect();
        for descriptor in descriptors {
            let owner = match descriptor.owner {
                BackendPortOwner::Bus(_) => ApplicationPortOwner::Bus {
                    bus_id: BusId::from_raw(1),
                },
                BackendPortOwner::GlobalFxControl => ApplicationPortOwner::GlobalFxControl,
                BackendPortOwner::Track => continue,
            };
            self.register_port(descriptor, owner)?;
        }
        if let Some(master) = self.observed.mixer.buses.get(&MASTER_BUS_ID) {
            for (index, channel) in master.channels.iter().enumerate() {
                self.channels
                    .insert(BusChannelId::from_raw(index as u64 + 1), channel.id);
            }
        }
        Ok(())
    }

    fn register_port(
        &mut self,
        descriptor: BackendPortDescriptor,
        owner: ApplicationPortOwner,
    ) -> Result<PortId> {
        if let Some(id) = self.app_port(descriptor.id) {
            return Ok(id);
        }
        let id = PortId::from_raw(allocate(&mut self.next_port)?);
        let role = match descriptor.role {
            BackendPortRole::AudioInput => PortRole::AudioInput,
            BackendPortRole::AudioOutput => PortRole::AudioOutput,
            BackendPortRole::AudioSend => PortRole::AudioSend,
            BackendPortRole::AudioReturn => PortRole::AudioReturn,
            BackendPortRole::MidiInput => PortRole::MidiInput,
            BackendPortRole::MidiOutput => PortRole::MidiOutput,
            BackendPortRole::MidiSend => PortRole::MidiSend,
        };
        self.ports.insert(
            id,
            Port {
                backend: descriptor.id,
                view: ApplicationPortState {
                    id,
                    owner,
                    name: descriptor.name,
                    data_type: data_type(descriptor.data_type),
                    direction: direction(descriptor.direction),
                    role,
                    connection_policy: ConnectionPolicy::UserManaged,
                },
            },
        );
        Ok(id)
    }

    pub(super) fn insert_track(
        &mut self,
        created: BackendTrackCreation,
        name: String,
        audio_channels: u32,
        midi: bool,
        sync: bool,
    ) -> Result<(TrackId, Vec<LoopId>)> {
        let id = TrackId::from_raw(allocate(&mut self.next_track)?);
        let mut ports = Vec::new();
        for descriptor in created.ports {
            ports.push(self.register_port(
                descriptor,
                ApplicationPortOwner::Track {
                    track_id: id,
                    kind: if sync {
                        TrackPortOwnerKind::Sync
                    } else {
                        TrackPortOwnerKind::Main
                    },
                },
            )?);
        }
        let mut loops = Vec::new();
        for (index, backend) in created.loops.into_iter().enumerate() {
            let loop_id = self.insert_loop(id, backend, audio_channels, midi, sync)?;
            if !sync {
                self.loops.get_mut(&loop_id).unwrap().view.name = format!("Loop {}", index + 1);
            }
            loops.push(loop_id);
        }
        self.tracks.insert(
            id,
            Track {
                backend: created.track_id,
                audio_channels,
                midi,
                monitoring_intent: None,
                view: TrackState {
                    id,
                    name,
                    is_sync: sync,
                    structural_state: StructuralState::Creating,
                    port_ids: ports.into(),
                    ..Default::default()
                },
                loops: loops.clone(),
            },
        );
        self.track_order.push(id);
        Ok((id, loops))
    }

    pub(super) fn insert_loop(
        &mut self,
        track: TrackId,
        backend: BackendLoopId,
        audio_channels: u32,
        midi: bool,
        sync: bool,
    ) -> Result<LoopId> {
        let id = LoopId::from_raw(allocate(&mut self.next_loop)?);
        self.loops.insert(
            id,
            Loop {
                backend,
                track,
                view: LoopState {
                    id,
                    name: if sync {
                        "Sync".to_owned()
                    } else {
                        format!(
                            "Loop {}",
                            self.tracks
                                .get(&track)
                                .map_or(1, |track| track.loops.len() + 1)
                        )
                    },
                    structural_state: StructuralState::Creating,
                    mode: LoopMode::Stopped,
                    sync,
                    has_audio: audio_channels > 0,
                    has_midi: midi,
                    show_gain: audio_channels > 0,
                    stereo: audio_channels == 2,
                    ..Default::default()
                },
            },
        );
        Ok(id)
    }

    pub(super) fn apply_observation(&mut self, observed: BackendSnapshot) {
        if observed.connections.revision != self.observed.connections.revision
            || observed.mixer.revision != self.observed.mixer.revision
        {
            self.bump();
        }
        for track in self.tracks.values_mut() {
            if let Some(state) = observed.tracks.get(&track.backend) {
                if track.view.structural_state == StructuralState::Creating {
                    track.view.structural_state = StructuralState::Confirmed;
                }
                track.view.controls = controls(state);
            }
        }
        for entry in self.loops.values_mut() {
            if let Some(state) = observed.loops.get(&entry.backend) {
                let view = &mut entry.view;
                view.structural_state = StructuralState::Confirmed;
                view.content_revision = state.content_revision;
                view.mode = mode(state.mode);
                view.next_mode = state.next_mode.map(mode).unwrap_or(LoopMode::Unknown);
                view.next_transition_delay = state.next_transition_delay;
                view.length_frames = u64::from(state.length);
                view.position = if state.length == 0 {
                    0.0
                } else {
                    state.position as f32 / state.length as f32
                };
                view.empty = state.length == 0;
                view.gain = state.gain;
                view.balance = state.balance;
                view.midi_activity = state.midi_activity;
                view.peak_left_db = state.audio_peaks.first().copied().unwrap_or(-200.0);
                view.peak_right_db = state
                    .audio_peaks
                    .get(1)
                    .copied()
                    .unwrap_or(view.peak_left_db);
                view.capture_alignment_frames = state.capture_alignment_frames;
                view.processor_alignment_frames = state.processor_alignment_frames;
            }
        }
        self.xruns = self.xruns.saturating_add(observed.status.xruns);
        self.observed = observed;
        self.observed.status.xruns = self.xruns;
    }

    pub(super) fn settle_effect(&mut self, effect: &Effect, success: bool) {
        match effect {
            Effect::SessionReset(tracks) => {
                for id in tracks {
                    if self
                        .tracks
                        .get(id)
                        .is_some_and(|track| !self.observed.tracks.contains_key(&track.backend))
                    {
                        self.forget_track(*id);
                    }
                }
            }
            Effect::TrackRemoved(id) => {
                if success {
                    self.forget_track(*id);
                } else if let Some(track) = self.tracks.get_mut(id) {
                    track.view.structural_state = StructuralState::Confirmed;
                }
            }
            Effect::TrackCreated(id, _) if !success => {
                if !self
                    .tracks
                    .get(id)
                    .is_some_and(|track| self.observed.tracks.contains_key(&track.backend))
                {
                    self.forget_track(*id);
                } else {
                    self.prune_failed_loops(*id);
                }
            }
            Effect::LoopCreated(track, id) if !success => {
                if !self
                    .loops
                    .get(id)
                    .is_some_and(|entry| self.observed.loops.contains_key(&entry.backend))
                {
                    self.loops.remove(id);
                    if let Some(track) = self.tracks.get_mut(track) {
                        track.loops.retain(|loop_id| loop_id != id);
                    }
                }
            }
            _ => {}
        }
    }

    fn prune_failed_loops(&mut self, id: TrackId) {
        let failed: Vec<_> = self
            .loops
            .iter()
            .filter_map(|(loop_id, entry)| {
                (entry.track == id && !self.observed.loops.contains_key(&entry.backend))
                    .then_some(*loop_id)
            })
            .collect();
        for loop_id in &failed {
            self.loops.remove(loop_id);
        }
        if let Some(track) = self.tracks.get_mut(&id) {
            track.loops.retain(|id| !failed.contains(id));
        }
    }

    fn forget_track(&mut self, id: TrackId) {
        self.tracks.remove(&id);
        self.track_order.retain(|track| *track != id);
        self.loops.retain(|_, entry| entry.track != id);
        let ports: BTreeSet<_> = self.ports.iter().filter_map(|(port, entry)| matches!(entry.view.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == id).then_some(*port)).collect();
        self.ports.retain(|id, _| !ports.contains(id));
        self.connection_intent
            .retain(|(port, _), _| !ports.contains(port));
        self.route_intent
            .retain(|route, _| !ports.contains(&route.source_port_id));
        for tracks in self.piano_notes.values_mut() {
            tracks.remove(&id);
        }
    }
}

pub(super) fn data_type(value: BackendPortDataType) -> PortDataType {
    match value {
        BackendPortDataType::Audio => PortDataType::Audio,
        BackendPortDataType::Midi => PortDataType::Midi,
    }
}
fn direction(value: BackendPortDirection) -> PortDirection {
    match value {
        BackendPortDirection::Input => PortDirection::Input,
        BackendPortDirection::Output => PortDirection::Output,
    }
}
fn mode(value: BackendLoopMode) -> LoopMode {
    match value {
        BackendLoopMode::Stopped => LoopMode::Stopped,
        BackendLoopMode::Playing => LoopMode::Playing,
        BackendLoopMode::Recording => LoopMode::Recording,
        BackendLoopMode::Replacing => LoopMode::Replacing,
        BackendLoopMode::PlayingDryThroughWet => LoopMode::PlayingDryThroughWet,
        BackendLoopMode::RecordingDryIntoWet => LoopMode::RecordingDryIntoWet,
        BackendLoopMode::Unknown => LoopMode::Unknown,
    }
}
fn status(state: &BackendStatus) -> StatusState {
    let audio_driver = match state.driver_state {
        BackendDriverState::Dummy => AudioDriverState::Dummy,
        BackendDriverState::AwaitingGesture => AudioDriverState::AwaitingGesture,
        BackendDriverState::RequestingPermission => AudioDriverState::RequestingPermission,
        BackendDriverState::Starting => AudioDriverState::Starting,
        BackendDriverState::Running => AudioDriverState::Running,
        BackendDriverState::Suspended => AudioDriverState::Suspended,
        BackendDriverState::Denied => AudioDriverState::Denied,
        BackendDriverState::Unsupported => AudioDriverState::Unsupported,
        BackendDriverState::Failed => AudioDriverState::Failed,
        BackendDriverState::Stopped => AudioDriverState::Stopped,
    };
    StatusState {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        dsp_load_percent: state.dsp_load_percent,
        xruns: state.xruns,
        sample_rate: state.sample_rate,
        buffer_size: state.buffer_size,
        audio_driver,
        callback_count: state.callback_count,
        processed_frames: state.processed_frames,
        input_peak: state.input_peak,
        output_peak: state.output_peak,
        render_discontinuities: state.render_discontinuities,
        memory_growths: state.memory_growths,
        render_memory_growths: state.render_memory_growths,
        command_overflows: state.command_overflows,
        storage_low_channels: state.storage_low_channels,
        storage_exhaustions: state.storage_exhaustions,
    }
}
fn controls(state: &BackendTrackState) -> TrackControlState {
    let audio = state.audio_channels > 0;
    let stereo = state.audio_channels == 2;
    TrackControlState {
        has_input: audio || state.midi,
        has_output: audio || state.midi,
        has_input_audio: audio,
        has_output_audio: audio,
        input_stereo: stereo,
        output_stereo: stereo,
        input_gain_db: state.input_gain_db,
        input_balance: state.input_balance,
        input_monitoring: state.input_monitoring,
        output_gain_db: state.output_gain_db,
        output_balance: state.output_balance,
        output_muted: state.output_muted,
        input_midi_activity: state.input_midi_activity,
        output_midi_activity: state.output_midi_activity,
        latest_input_midi_message: state
            .latest_input_midi_message
            .and_then(|message| LatestMidiMessage::new(message.bytes, message.len)),
        input_peak_left_db: state.input_peaks.first().copied().unwrap_or(-200.0),
        input_peak_right_db: state.input_peaks.get(1).copied().unwrap_or(-200.0),
        output_peak_left_db: state.output_peaks.first().copied().unwrap_or(-200.0),
        output_peak_right_db: state.output_peaks.get(1).copied().unwrap_or(-200.0),
    }
}
