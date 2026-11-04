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
}
impl Cut {
    pub(super) fn new(service: &Service) -> Self {
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
        }
    }
    fn port(&self, id: PortId) -> Result<image::Port> {
        Ok(image::Port {
            state: self
                .state
                .connections
                .application_ports
                .iter()
                .find(|port| port.id == id)
                .ok_or_else(|| anyhow!("capture omitted port {id}"))?
                .clone(),
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
            if channel.mode != BackendChannelMode::Direct {
                return Err(anyhow!("capture requires unsupported audio channel modes"));
            }
            let frames = channel.samples.len() as u64;
            let object = if channel.samples.is_empty() {
                None
            } else {
                payloads.push(image::Payload::Audio(channel.samples.into()));
                Some(payloads.len() as u32)
            };
            audio.push(image::Channel {
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
            if channel.mode != BackendChannelMode::Direct {
                return Err(anyhow!("capture requires unsupported MIDI channel modes"));
            }
            let object = if channel.events.is_empty() {
                None
            } else {
                payloads.push(image::Payload::Midi(channel.events));
                Some(payloads.len() as u32)
            };
            midi.push(image::MidiChannel {
                channel: image::Channel {
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
            id: state.id,
            name: state.name.clone(),
            sync: state.sync,
            frames: u64::from(content.length),
            gain: state.gain,
            balance: state.balance,
            audio,
            midi,
        })
    }
    pub(super) fn finish(
        self,
        mut capture: BackendSessionData,
        target: persistence::Target,
    ) -> Result<Arc<image::Image>> {
        let mut payloads = Vec::new();
        let mut tracks = Vec::new();
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
            tracks.push(image::Track {
                id: track.id,
                name: track.name.clone(),
                port_name_base: captured.port_name_base,
                sync: track.is_sync,
                audio_channels: match captured.topology {
                    BackendTrackTopology::Direct { audio_channels, .. } => audio_channels,
                    _ => return Err(anyhow!("capture requires unsupported track topology")),
                },
                midi: captured.state.midi,
                controls: image::Controls {
                    input_gain_db: controls.input_gain_db,
                    input_balance: controls.input_balance,
                    monitoring: controls.input_monitoring,
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
