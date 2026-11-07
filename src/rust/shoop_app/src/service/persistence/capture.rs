use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use super::*;
use shoop_app_protocol::persistence_image as image;

pub(in crate::service) struct Cut {
    state: Arc<SessionSnapshot>,
    tracks: BTreeMap<TrackId, BackendTrackId>,
    loops: BTreeMap<LoopId, BackendLoopId>,
    external: BTreeMap<PortId, BTreeSet<String>>,
    routes: BTreeSet<MixerRouteState>,
    annotations: BTreeMap<(LoopId, bool, u32), (u64, Option<String>)>,
    recorded: BTreeMap<LoopId, Option<String>>,
    manual_monitoring: BTreeMap<TrackId, bool>,
    sync: BTreeMap<LoopId, LoopId>,
}
impl Cut {
    pub(in crate::service) fn matches_composites(&self, capture: &BackendSessionData) -> bool {
        self.state.composites.iter().all(|view| {
            view.active.as_ref().is_none_or(|active| {
                capture
                    .composites
                    .iter()
                    .find(|c| c.source_id == self.loops[&view.loop_id].raw())
                    .is_some_and(|c| c.version == active.version)
            })
        })
    }
    pub(in crate::service) fn new(service: &Service) -> Self {
        let state = service.snapshot();
        let mut external = BTreeMap::<PortId, BTreeSet<String>>::new();
        for link in state.connections.confirmed_links.iter() {
            external
                .entry(link.application_port_id)
                .or_default()
                .insert(link.host_port_id.as_str().to_owned());
        }
        for ((id, host), intent) in &service.connection_intent {
            let links = external.entry(*id).or_default();
            if intent.connected {
                links.insert(host.clone());
            } else {
                links.remove(host);
            }
        }
        let mut routes = state
            .connections
            .mixer_links
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for (route, intent) in &service.route_intent {
            if intent.connected {
                routes.insert(*route);
            } else {
                routes.remove(route);
            }
        }
        Self {
            state,
            tracks: service
                .tracks
                .iter()
                .map(|(id, track)| (*id, track.backend))
                .collect(),
            loops: service
                .loops
                .iter()
                .map(|(id, entry)| (*id, entry.backend))
                .collect(),
            external,
            routes,
            annotations: service.channel_annotations.clone(),
            manual_monitoring: service
                .tracks
                .iter()
                .map(|(id, track)| {
                    (
                        *id,
                        service
                            .observed
                            .tracks
                            .get(&track.backend)
                            .is_some_and(|state| state.input_monitoring),
                    )
                })
                .collect(),
            sync: service
                .loops
                .iter()
                .filter_map(|(id, entry)| {
                    let source = service
                        .observed
                        .loops
                        .get(&entry.backend)
                        .and_then(|state| state.sync_source)
                        .or_else(|| {
                            service
                                .observed
                                .composites
                                .get(&BackendCompositeId::from_raw(entry.backend.raw()))
                                .and_then(|state| state.sync_source)
                        })?;
                    service.loops.iter().find_map(|(source_id, source_entry)| {
                        (source_entry.backend == source).then_some((*id, *source_id))
                    })
                })
                .collect(),
            recorded: service
                .loops
                .iter()
                .map(|(id, entry)| {
                    (
                        *id,
                        service
                            .observed
                            .loops
                            .get(&entry.backend)
                            .and_then(|s| s.recorded_processor_state.clone()),
                    )
                })
                .collect(),
        }
    }
    fn port(&self, id: PortId) -> Result<image::Port> {
        let mut state = self
            .state
            .connections
            .application_ports
            .iter()
            .find(|port| port.id == id)
            .ok_or_else(|| anyhow!("capture omitted port {id}"))?
            .clone();
        state.publication = None;
        Ok(image::Port {
            state,
            external_connections: self
                .external
                .get(&id)
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        })
    }
    fn content(
        &self,
        state: &LoopState,
        content: BackendLoopContent,
        payloads: &mut Vec<image::Payload>,
    ) -> Result<image::Loop> {
        let mut audio = Vec::new();
        let mut midi = Vec::new();
        let annotation = |midi, index| {
            self.annotations
                .get(&(state.id, midi, index))
                .filter(|(revision, _)| *revision == state.content_revision)
                .and_then(|(_, value)| value.clone())
        };
        for (index, channel) in content.audio.into_iter().enumerate() {
            let frames = channel.samples.len() as u64;
            let object = if channel.samples.is_empty() {
                None
            } else {
                payloads.push(image::Payload::Audio(channel.samples.into()));
                Some(payloads.len() as u32)
            };
            audio.push(image::Channel {
                mode: match channel.mode {
                    BackendChannelMode::Direct => image::ChannelMode::Direct,
                    BackendChannelMode::Dry => image::ChannelMode::Dry,
                    BackendChannelMode::Wet => image::ChannelMode::Wet,
                },
                object,
                frames,
                gain: channel.gain,
                start_offset: i64::from(channel.start_offset),
                capture_alignment: i64::from(channel.capture_alignment_frames),
                preplay: u64::from(channel.preplay),
                recording_started_at: annotation(false, index as u32),
            });
        }
        for (index, channel) in content.midi.into_iter().enumerate() {
            let object = if channel.events.is_empty() {
                None
            } else {
                payloads.push(image::Payload::Midi(channel.events));
                Some(payloads.len() as u32)
            };
            midi.push(image::MidiChannel {
                channel: image::Channel {
                    mode: match channel.mode {
                        BackendChannelMode::Direct => image::ChannelMode::Direct,
                        BackendChannelMode::Dry => image::ChannelMode::Dry,
                        BackendChannelMode::Wet => return Err(anyhow!("MIDI cannot be wet")),
                    },
                    object,
                    frames: u64::from(channel.length),
                    gain: 1.,
                    start_offset: i64::from(channel.start_offset),
                    capture_alignment: i64::from(channel.capture_alignment_frames),
                    preplay: u64::from(channel.preplay),
                    recording_started_at: annotation(true, index as u32),
                },
                initial: channel.start_state,
            });
        }
        Ok(image::Loop {
            recorded_processor_state: self.recorded.get(&state.id).cloned().flatten(),
            sync_source: self.sync.get(&state.id).copied(),
            repeat_sync: state.repeat_sync,
            composite: self
                .state
                .composites
                .iter()
                .find(|view| view.loop_id == state.id)
                .and_then(|view| view.active.as_ref())
                .map(|active| shoop_app_protocol::composite::Content {
                    document: active.document.clone(),
                    prepared: active.prepared.clone(),
                }),
            id: state.id,
            name: state.name.clone(),
            sync: state.sync,
            frames: self
                .state
                .composites
                .iter()
                .find(|view| view.loop_id == state.id)
                .and_then(|view| view.active.as_ref())
                .map_or(u64::from(content.length), |active| {
                    active.prepared.unit_frames * active.prepared.iterations
                }),
            gain: state.gain,
            balance: state.balance,
            audio,
            midi,
        })
    }
    pub(in crate::service) fn finish(
        self,
        mut capture: BackendSessionData,
        target: persistence::Target,
    ) -> Result<Arc<image::Image>> {
        let mut payloads = Vec::new();
        let mut tracks = Vec::new();
        for view in &self.state.composites {
            if let Some(active) = &view.active {
                let source = self.loops[&view.loop_id].raw();
                if capture
                    .composites
                    .iter()
                    .find(|c| c.source_id == source)
                    .map(|c| c.version)
                    != Some(active.version)
                {
                    return Err(anyhow!(
                        "an arrangement changed during capture; retry saving its confirmed version"
                    ));
                }
            }
        }
        for track in &self.state.tracks {
            let backend = self.tracks[&track.id];
            let position = capture
                .tracks
                .iter()
                .position(|candidate| candidate.source_id == backend.raw())
                .ok_or_else(|| anyhow!("backend capture omitted track {}", track.id))?;
            let mut captured = capture.tracks.swap_remove(position);
            let mut loops = Vec::new();
            for state in &track.loops {
                if matches!(target, persistence::Target::Loop { loop_id, .. } if loop_id != state.id)
                {
                    continue;
                }
                let position = captured
                    .loops
                    .iter()
                    .position(|content| content.source_id == self.loops[&state.id].raw())
                    .ok_or_else(|| anyhow!("backend capture omitted loop {}", state.id))?;
                loops.push(self.content(
                    state,
                    captured.loops.swap_remove(position),
                    &mut payloads,
                )?);
            }
            let controls = &track.controls;
            let editor = track.fx.as_ref().and_then(|fx| fx.editor.as_ref());
            let processor_state = editor
                .map(TrackProcessorEditorState::canonical_state)
                .transpose()?;
            let (builtin_fx_midi_cc_assignments, oxisynth_midi_cc_assignments) = match editor {
                Some(TrackProcessorEditorState::BuiltInFx(fx)) => {
                    (fx.midi_cc_assignments.to_vec(), Vec::new())
                }
                Some(TrackProcessorEditorState::OxiSynth(synth)) => {
                    (Vec::new(), synth.midi_cc_assignments.to_vec())
                }
                None => (Vec::new(), Vec::new()),
            };
            tracks.push(image::Track {
                topology: track.topology.clone(),
                default_playback_mode: track.default_playback_mode,
                processor_state,
                builtin_fx_midi_cc_assignments,
                oxisynth_midi_cc_assignments,
                id: track.id,
                name: track.name.clone(),
                port_name_base: captured.port_name_base,
                sync: track.is_sync,
                audio_channels: captured.topology.wet_audio_channels(),
                midi: captured.state.midi,
                controls: image::Controls {
                    input_gain_db: controls.input_gain_db,
                    input_balance: controls.input_balance,
                    monitoring: self.manual_monitoring[&track.id],
                    output_gain_db: controls.output_gain_db,
                    output_balance: controls.output_balance,
                    muted: controls.output_muted,
                },
                ports: track
                    .port_ids
                    .iter()
                    .map(|id| self.port(*id))
                    .collect::<Result<_>>()?,
                loops,
            });
        }
        let manifest = match target {
            persistence::Target::Loop { loop_id, .. } => image::Manifest::Loop {
                sample_rate: capture.sample_rate,
                content: tracks
                    .into_iter()
                    .flat_map(|track| track.loops)
                    .find(|content| content.id == loop_id)
                    .ok_or_else(|| anyhow!("capture omitted requested loop"))?,
            },
            persistence::Target::Session { .. } => image::Manifest::Session(image::Session {
                sample_rate: capture.sample_rate,
                global: self.state.global.clone(),
                tracks,
                buses: {
                    let mut buses = self.state.buses.iter().collect::<Vec<_>>();
                    buses.sort_by_key(|bus| bus.id);
                    buses
                        .into_iter()
                        .map(|bus| {
                            Ok(image::Bus {
                                fx: bus
                                    .fx
                                    .as_ref()
                                    .map(|fx| {
                                        let Some(TrackProcessorEditorState::BuiltInFx(editor)) =
                                            &fx.editor
                                        else {
                                            return Err(anyhow!("bus FX state unavailable"));
                                        };
                                        Ok(image::BusFx {
                                            processor_type: fx.processor_type.clone(),
                                            state: TrackProcessorEditorState::BuiltInFx(
                                                editor.clone(),
                                            )
                                            .canonical_state()?,
                                            midi_cc_assignments: editor
                                                .midi_cc_assignments
                                                .to_vec(),
                                        })
                                    })
                                    .transpose()?,
                                id: bus.id,
                                name: bus.name.clone(),
                                gain_db: bus.gain_db,
                                balance: bus.balance,
                                muted: bus.muted,
                                channels: bus
                                    .channels
                                    .iter()
                                    .map(|channel| {
                                        Ok(image::BusChannel {
                                            id: channel.id,
                                            label: channel.label.clone(),
                                            port: self.port(channel.output_port_id)?,
                                        })
                                    })
                                    .collect::<Result<_>>()?,
                            })
                        })
                        .collect::<Result<_>>()?
                },
                bus_display_order: self.state.buses.iter().map(|bus| bus.id).collect(),
                preview_bus: self.state.preview_bus,
                global_ports: self
                    .state
                    .connections
                    .application_ports
                    .iter()
                    .filter(|port| port.owner == ApplicationPortOwner::GlobalFxControl)
                    .map(|port| self.port(port.id))
                    .collect::<Result<_>>()?,
                routes: self.routes.iter().copied().collect(),
                selected: self
                    .state
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .filter(|entry| entry.selected)
                    .map(|entry| entry.id)
                    .collect(),
                target: self
                    .state
                    .tracks
                    .iter()
                    .flat_map(|track| &track.loops)
                    .find(|entry| entry.targeted)
                    .map(|entry| entry.id),
            }),
        };
        let image = Arc::new(image::Image { manifest, payloads });
        Ok(image)
    }
    pub(super) fn finish_loop(
        self,
        content: BackendLoopContent,
        loop_id: LoopId,
    ) -> Result<Arc<image::Image>> {
        if self.loops.get(&loop_id).map(|id| id.raw()) != Some(content.source_id) {
            return Err(anyhow!("capture returned another loop"));
        }
        let state = self
            .state
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|entry| entry.id == loop_id)
            .ok_or_else(|| anyhow!("capture target disappeared"))?;
        let mut payloads = Vec::new();
        let content = self.content(state, content, &mut payloads)?;
        let image = Arc::new(image::Image {
            manifest: image::Manifest::Loop {
                sample_rate: self.state.status.sample_rate,
                content,
            },
            payloads,
        });
        Ok(image)
    }
}
