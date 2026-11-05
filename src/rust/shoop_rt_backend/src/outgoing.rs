//! Requests built from application operations: identities are reserved here and the
//! requests queued until credits allow sending them.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use anyhow::{anyhow, Result};
use shoop_backend::{
    BackendBusChannelId, BackendBusControl, BackendConfirmedLink, BackendLoopId, BackendMixerLink,
    BackendPortDataType, BackendPortDescriptor, BackendPortDirection, BackendPortId,
    BackendPortOwner, BackendPortRole, BackendTrackControl, BackendTrackCreation, BackendTrackId,
    BackendTrackTopology,
};
use shoop_rt_protocol::{
    BusControl, BusPort, EnginePort, LoopSlotId, PortKind, Request, StripControl, StripId,
    StripPort, StripShape,
};
use shoop_rt_transport::carrier::Carrier;

use crate::mirror::{
    db_amplitude, host_data_type, host_direction, Controls, Loop, LoopView, Port, PortTarget, Track,
};
use crate::requests::Purpose;
use crate::{EngineClock, RtBackend};

/// The engine request for an application mixer link.
fn route_request(port: StripPort, bus: BusPort, connected: bool) -> Request {
    Request::Route {
        port,
        bus,
        connected,
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    /// Reserves a strip and its loops and queues their creation.
    pub(crate) fn create_strip(
        &mut self,
        port_name_base: &str,
        audio_channels: u32,
        midi: bool,
        initial_loops: usize,
    ) -> Result<BackendTrackCreation> {
        self.create_track_topology(
            port_name_base,
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            },
            initial_loops,
        )
    }

    pub(crate) fn create_track_topology(
        &mut self,
        port_name_base: &str,
        topology: BackendTrackTopology,
        initial_loops: usize,
    ) -> Result<BackendTrackCreation> {
        self.ready()?;
        let dry = topology.dry_audio_channels();
        let wet = topology.wet_audio_channels();
        let midi = topology.has_midi();
        let kind = match &topology {
            BackendTrackTopology::Direct { .. } => None,
            BackendTrackTopology::DryWetExternal { .. } => {
                Some(shoop_rt_protocol::ProcessorKind::External)
            }
            BackendTrackTopology::DryWetProcessor { processor_type, .. } => {
                Some(match processor_type.as_str() {
                    shoop_app_api::TrackProcessorTypeId::BUILTIN_FX => {
                        shoop_rt_protocol::ProcessorKind::BuiltInFx
                    }
                    shoop_app_api::TrackProcessorTypeId::OXISYNTH => {
                        shoop_rt_protocol::ProcessorKind::OxiSynth
                    }
                    _ => return Err(anyhow!("unsupported track processor {processor_type}")),
                })
            }
        };
        if kind.is_some_and(|kind| kind != shoop_rt_protocol::ProcessorKind::External) {
            let kind = kind.expect("hosted processor");
            if dry == 0
                || dry != wet
                || !midi
                || (kind == shoop_rt_protocol::ProcessorKind::OxiSynth && dry != 2)
            {
                return Err(anyhow!("invalid audio/MIDI shape for this track processor"));
            }
            let processors = self
                .processors
                .ok_or_else(|| anyhow!("host processor limits are unavailable"))?;
            let available = if kind == shoop_rt_protocol::ProcessorKind::BuiltInFx {
                processors.effective.fx_instances
            } else {
                processors.effective.synth_instances
            };
            let used = self
                .mirror
                .tracks
                .values()
                .filter(|track| match (&track.topology, &topology) {
                    (
                        BackendTrackTopology::DryWetProcessor {
                            processor_type: before,
                            ..
                        },
                        BackendTrackTopology::DryWetProcessor {
                            processor_type: after,
                            ..
                        },
                    ) => before == after,
                    _ => false,
                })
                .count();
            if available == 0 {
                let reason = if kind == shoop_rt_protocol::ProcessorKind::BuiltInFx {
                    processors.effective.fx_unavailable
                } else {
                    processors.effective.synth_unavailable
                };
                return Err(anyhow!(
                    "track processor unavailable: {}",
                    reason.map_or("no prepared instances", |reason| reason.message())
                ));
            }
            if used >= available as usize {
                return Err(anyhow!(
                    "the host's prepared processor capacity is exhausted"
                ));
            }
        }
        if kind == Some(shoop_rt_protocol::ProcessorKind::External) && dry == 0 && wet == 0 && !midi
        {
            return Err(anyhow!("an External track needs audio or MIDI"));
        }
        let audio_channels = wet;
        self.require_mutation_capacity(
            initial_loops
                .checked_add(1)
                .ok_or_else(|| anyhow!("too many initial loops"))?,
        )?;
        let limits = self.limits();
        if dry > u32::from(limits.strip_audio_channels)
            || wet > u32::from(limits.strip_audio_channels)
        {
            return Err(anyhow!(
                "tracks of this engine have at most {} audio channels",
                limits.strip_audio_channels
            ));
        }
        if self.mirror.strip_slots.free() == 0 {
            return Err(anyhow!(
                "all {} prepared track slots of the engine are in use",
                self.mirror.strip_slots.capacity()
            ));
        }
        self.require_loop_slots(initial_loops)?;
        let strip = StripId(self.mirror.strip_slots.reserve().expect("a free slot"));
        let track_id = self.mirror.next_track_id();
        let mut descriptors = Vec::new();
        let mut shapes = Vec::new();
        let mut audio = |name, direction, role, kind, index| {
            shapes.push((
                name,
                BackendPortDataType::Audio,
                direction,
                role,
                kind,
                index,
            ))
        };
        match &topology {
            BackendTrackTopology::Direct { .. } => {
                for index in 0..dry {
                    let suffix = if dry == 1 {
                        String::new()
                    } else {
                        format!("_{}", index + 1)
                    };
                    audio(
                        format!("{port_name_base}_direct_in{suffix}"),
                        BackendPortDirection::Input,
                        BackendPortRole::AudioInput,
                        PortKind::AudioIn,
                        index as u8,
                    );
                    audio(
                        format!("{port_name_base}_direct_out{suffix}"),
                        BackendPortDirection::Output,
                        BackendPortRole::AudioOutput,
                        PortKind::AudioOut,
                        index as u8,
                    );
                }
            }
            BackendTrackTopology::DryWetExternal { .. }
            | BackendTrackTopology::DryWetProcessor { .. } => {
                let external = matches!(topology, BackendTrackTopology::DryWetExternal { .. });
                for index in 0..dry {
                    audio(
                        format!("{port_name_base}_audio_dry_in_{}", index + 1),
                        BackendPortDirection::Input,
                        BackendPortRole::AudioInput,
                        PortKind::AudioIn,
                        index as u8,
                    );
                    if external {
                        audio(
                            format!("{port_name_base}_audio_dry_send_{}", index + 1),
                            BackendPortDirection::Output,
                            BackendPortRole::AudioSend,
                            PortKind::AudioSend,
                            index as u8,
                        );
                    }
                }
                for index in 0..wet {
                    if external {
                        audio(
                            format!("{port_name_base}_audio_wet_return_{}", index + 1),
                            BackendPortDirection::Input,
                            BackendPortRole::AudioReturn,
                            PortKind::AudioReturn,
                            index as u8,
                        );
                    }
                    audio(
                        format!("{port_name_base}_audio_wet_out_{}", index + 1),
                        BackendPortDirection::Output,
                        BackendPortRole::AudioOutput,
                        PortKind::AudioOut,
                        index as u8,
                    );
                }
            }
        }
        if midi {
            match &topology {
                BackendTrackTopology::Direct { .. } => {
                    shapes.push((
                        format!("{port_name_base}_direct_midi_in"),
                        BackendPortDataType::Midi,
                        BackendPortDirection::Input,
                        BackendPortRole::MidiInput,
                        PortKind::MidiIn,
                        0,
                    ));
                    shapes.push((
                        format!("{port_name_base}_direct_midi_out"),
                        BackendPortDataType::Midi,
                        BackendPortDirection::Output,
                        BackendPortRole::MidiOutput,
                        PortKind::MidiOut,
                        0,
                    ));
                }
                _ => {
                    shapes.push((
                        format!("{port_name_base}_dry_midi_in"),
                        BackendPortDataType::Midi,
                        BackendPortDirection::Input,
                        BackendPortRole::MidiInput,
                        PortKind::MidiIn,
                        0,
                    ));
                    if matches!(topology, BackendTrackTopology::DryWetExternal { .. }) {
                        shapes.push((
                            format!("{port_name_base}_dry_midi_send"),
                            BackendPortDataType::Midi,
                            BackendPortDirection::Output,
                            BackendPortRole::MidiSend,
                            PortKind::MidiSend,
                            0,
                        ));
                    }
                }
            }
        }
        for (name, data_type, direction, role, kind, index) in shapes {
            let id = self.mirror.next_port_id();
            let descriptor = BackendPortDescriptor {
                id,
                owner: BackendPortOwner::Track,
                name,
                data_type,
                direction,
                role,
            };
            self.mirror.ports.insert(
                id,
                Port {
                    descriptor: descriptor.clone(),
                    target: PortTarget::Strip {
                        track: track_id,
                        kind,
                        index,
                    },
                },
            );
            descriptors.push(descriptor);
        }
        self.mirror.tracks.insert(
            track_id,
            Track {
                processor_visible: false,
                processor_snapshot: None,
                default_playback_mode: Default::default(),
                port_name_base: port_name_base.to_owned(),
                topology: topology.clone(),
                processor_slot: None,
                strip,
                audio_channels,
                midi,
                ports: descriptors.iter().map(|port| port.id).collect(),
                loops: Vec::new(),
                controls: Controls::default(),
                confirmed: false,
                removing: false,
                input_meter: Default::default(),
                output_meter: Default::default(),
                midi_in: false,
                midi_out: false,
                latest_input_midi: None,
            },
        );
        self.mirror.strip_tracks.insert(strip, track_id);
        let shape = StripShape {
            audio_inputs: dry as u8,
            audio_outputs: wet as u8,
            midi_input: midi,
            midi_output: kind.is_none() && midi,
        };
        let request = kind.map_or(Request::CreateStrip { strip, shape }, |kind| {
            Request::CreateProcessedStrip { strip, shape, kind }
        });
        self.requests.push(request, Purpose::CreateStrip(track_id));
        let loops = (0..initial_loops)
            .map(|_| self.add_loop(track_id))
            .collect::<Result<Vec<_>>>()?;
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports: descriptors,
        })
    }

    fn require_loop_slots(&self, count: usize) -> Result<()> {
        let free = self.mirror.loop_slots.free();
        if free < count {
            return Err(anyhow!(
                "{free} of the engine's {} prepared loop slots are free; {count} are needed",
                self.mirror.loop_slots.capacity()
            ));
        }
        Ok(())
    }

    pub(crate) fn add_loop(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        self.ready()?;
        let strip = self
            .mirror
            .live_track(track_id)
            .ok_or_else(|| anyhow!("unknown track {track_id:?}"))?
            .strip;
        self.require_loop_slots(1)?;
        let slot = LoopSlotId(self.mirror.loop_slots.reserve().expect("a free slot"));
        let id = self.mirror.next_loop_id();
        self.mirror.loops.insert(
            id,
            Loop {
                recorded: None,
                timing: Default::default(),
                slot,
                track: track_id,
                confirmed: false,
                sync: None,
                gain: 1.0,
                balance: 0.0,
                view: LoopView::default(),
                meter: Default::default(),
                content: 0,
                rt_content_revision: 0,
            },
        );
        self.mirror.slot_loops.insert(slot, id);
        if let Some(track) = self.mirror.tracks.get_mut(&track_id) {
            track.loops.push(id);
        }
        self.requests.push(
            Request::CreateLoop {
                loop_id: slot,
                strip,
            },
            Purpose::CreateLoop(id),
        );
        Ok(id)
    }

    /// Queues a track's removal and releases its slots: the engine handles requests in order,
    /// so later reservations of these slots follow the removal.
    pub(crate) fn remove_strip(&mut self, track_id: BackendTrackId) {
        let Some(track) = self.mirror.tracks.get_mut(&track_id) else {
            return;
        };
        if track.removing {
            return;
        }
        track.removing = true;
        let strip = track.strip;
        let loops = track.loops.clone();
        self.mirror.strip_slots.release(strip.0);
        for loop_id in loops {
            if let Some(entry) = self.mirror.loops.get(&loop_id) {
                self.mirror.loop_slots.release(entry.slot.0);
            }
            self.reads.forget(loop_id);
        }
        self.requests.push(
            Request::RemoveStrip { strip },
            Purpose::RemoveStrip(track_id),
        );
    }

    pub(crate) fn queue_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        let strip = self
            .mirror
            .live_track(track_id)
            .ok_or_else(|| anyhow!("unknown track {track_id:?}"))?
            .strip;
        let finite = |value: f32| {
            if value.is_finite() {
                Ok(value)
            } else {
                Err(anyhow!("track gains and balances must be finite"))
            }
        };
        let (control, engine) = match control {
            BackendTrackControl::OutputGainDb(value) => (
                control,
                StripControl::OutputGain(db_amplitude(finite(value)?)),
            ),
            BackendTrackControl::InputGainDb(value) => (
                control,
                StripControl::InputGain(db_amplitude(finite(value)?)),
            ),
            BackendTrackControl::OutputBalance(value) => {
                let value = finite(value)?.clamp(-1.0, 1.0);
                (
                    BackendTrackControl::OutputBalance(value),
                    StripControl::OutputBalance(value),
                )
            }
            BackendTrackControl::InputBalance(value) => {
                let value = finite(value)?.clamp(-1.0, 1.0);
                (
                    BackendTrackControl::InputBalance(value),
                    StripControl::InputBalance(value),
                )
            }
            BackendTrackControl::OutputMute(value) => (control, StripControl::OutputMute(value)),
            BackendTrackControl::InputMonitoring(value) => {
                (control, StripControl::Monitoring(value))
            }
        };
        self.requests.push(
            Request::StripControl {
                strip,
                control: engine,
            },
            Purpose::StripControl(track_id, control),
        );
        Ok(())
    }

    pub(crate) fn queue_connection(
        &mut self,
        port_id: BackendPortId,
        host: &str,
        connected: bool,
    ) -> Result<()> {
        self.ready()?;
        if !connected
            && self.mirror.host_by_name(host).is_none()
            && self.mirror.live_port(port_id).is_some()
        {
            self.mirror.desired_links.remove(&BackendConfirmedLink {
                application_port_id: port_id,
                host_port_id: host.to_owned(),
            });
            return Ok(());
        }
        let port = self
            .mirror
            .live_port(port_id)
            .ok_or_else(|| anyhow!("unknown port {port_id:?}"))?;
        if port.target == PortTarget::GlobalFx
            && !self
                .capabilities
                .contains(shoop_rt_protocol::Capabilities::GLOBAL_FX_CONTROL)
        {
            return Err(anyhow!(
                "global FX MIDI control is unavailable in this host"
            ));
        }
        let host_port = self
            .mirror
            .host_by_name(host)
            .ok_or_else(|| anyhow!("unknown host port {host}"))?;
        if connected && !self.mirror.host_available(host_port) {
            return Err(anyhow!("host port {host} is disconnected"));
        }
        if host_data_type(host_port.kind) != port.descriptor.data_type
            || host_direction(host_port.direction) == port.descriptor.direction
        {
            return Err(anyhow!(
                "host port {host} is incompatible with {}",
                port.descriptor.name
            ));
        }
        let host_id = host_port.id;
        let target = port.target;
        let Some(engine_port) = self.mirror.engine_port(target) else {
            let link = BackendConfirmedLink {
                application_port_id: port_id,
                host_port_id: host.to_owned(),
            };
            if connected {
                self.mirror.links.insert(link);
            } else {
                self.mirror.links.remove(&link);
            }
            self.mirror.connection_revision += 1;
            return Ok(());
        };
        self.requests.push(
            Request::Connect {
                port: engine_port,
                host: host_id,
                connected,
            },
            Purpose::Connect {
                port: port_id,
                host: host.to_owned(),
                connected,
            },
        );
        Ok(())
    }

    pub(crate) fn queue_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        self.ready()?;
        let port = self
            .mirror
            .live_port(source_port_id)
            .filter(|port| {
                port.descriptor.owner == BackendPortOwner::Track
                    && port.descriptor.role == BackendPortRole::AudioOutput
            })
            .ok_or_else(|| anyhow!("mixer source is not a track audio output"))?;
        let bus = self
            .mirror
            .bus_for_channel(destination_channel_id)
            .ok_or_else(|| anyhow!("unknown bus channel"))?;
        let Some(EnginePort::Strip(strip_port)) = self.mirror.engine_port(port.target) else {
            return Err(anyhow!("mixer source is not a track audio output"));
        };
        self.requests.push(
            route_request(strip_port, bus, connected),
            Purpose::Route {
                link: BackendMixerLink {
                    source_port_id,
                    destination_channel_id,
                },
                connected,
            },
        );
        Ok(())
    }

    pub(crate) fn queue_bus_control(
        &mut self,
        id: shoop_backend::BackendBusId,
        control: BackendBusControl,
    ) -> Result<()> {
        let bus = self
            .mirror
            .buses
            .get(&id)
            .filter(|b| !b.removing)
            .ok_or_else(|| anyhow!("unknown bus"))?;
        let control = control.normalized(bus.channels.len())?;
        let slot = bus.slot;
        let engine = match control {
            BackendBusControl::GainDb(value) => BusControl::Gain(db_amplitude(value)),
            BackendBusControl::Balance(value) => BusControl::Balance(value),
            BackendBusControl::Mute(value) => BusControl::Mute(value),
        };
        self.requests.push(
            Request::BusControl {
                bus: slot,
                control: engine,
            },
            Purpose::BusControl(id, control),
        );
        Ok(())
    }
}

#[cfg(test)]
mod track_role_tests {
    use super::*;
    use crate::Mirror;
    use core::time::Duration;
    use shoop_backend::Backend;
    use shoop_rt_engine::{
        testing::{OfflineClient, OfflineRender},
        ControlEndpoint, EngineDescription, HostPorts, Segment,
    };
    use shoop_rt_transport::carrier::InProcess;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
    struct Clock(OfflineRender);
    impl EngineClock for Clock {
        fn advance(&mut self, _: Duration) {
            assert_no_heap(|| self.0.process(16)).unwrap();
        }
        fn wait(&mut self) -> bool {
            self.advance(Duration::ZERO);
            true
        }
    }
    type Stack = RtBackend<InProcess<ControlEndpoint>, Clock>;
    fn stack() -> Stack {
        let d = EngineDescription {
            external_dry_wet: true,
            sample_rate: 8000,
            quantum: 16,
            max_frames: 128,
            strip_channels: 8,
            host: HostPorts::numbered(16, 16, 1),
            segments: alloc::vec![Segment {
                strips: 4,
                loops: 8
            }],
            resource_pages: 2048,
            page_bytes: 512,
            ramp_frames: 0,
            processors: shoop_rt_protocol::ProcessorSettings {
                fx_instances: 1,
                synth_instances: 1,
                memory_mb: 256,
                ..Default::default()
            },
            ..EngineDescription::default()
        };
        let (render, client): (_, OfflineClient) =
            OfflineRender::new_with_processors(&d, shoop_host_processors::prepare(&d).unwrap())
                .unwrap();
        RtBackend::connect(client, Clock(render)).unwrap()
    }
    fn refresh(stack: &mut Stack) {
        stack.link = crate::Link::Resyncing;
        stack.resync = Some(Vec::new());
        stack.client.resync().unwrap();
        for _ in 0..4000 {
            stack.pump();
            if stack.is_ready() {
                return;
            }
            assert!(
                !matches!(stack.link, crate::Link::Disconnected(_)),
                "{:?}",
                stack.link
            );
            stack.clock.wait();
        }
        panic!("resynchronization did not finish");
    }
    #[shoop_test]
    fn real_host_roles_round_trip_connections_and_keep_hosted_endpoints_private() {
        let mut stack = stack();
        let topology = BackendTrackTopology::DryWetExternal {
            dry_audio_channels: 1,
            wet_audio_channels: 3,
            dry_midi: true,
        };
        let external = stack
            .create_track_topology("external", topology.clone(), 1)
            .unwrap();
        stack.wait_idle();
        assert!(stack.poll().unwrap().mutation_failures.is_empty());
        assert_eq!(external.ports.len(), 10);
        assert_eq!(
            Mirror::track_state(stack.mirror.tracks.get_mut(&external.track_id).unwrap()).topology,
            topology
        );
        for port in &external.ports {
            let target = stack.mirror.ports[&port.id].target;
            let PortTarget::Strip { kind, index, .. } = target else {
                panic!("strip port");
            };
            let name = match kind {
                PortKind::AudioIn => format!("audio_in_{}", index + 1),
                PortKind::AudioSend => format!("audio_out_{}", index + 9),
                PortKind::AudioReturn => format!("audio_in_{}", index + 9),
                PortKind::AudioOut => format!("audio_out_{}", index + 1),
                PortKind::MidiIn => "midi_in_1".to_owned(),
                PortKind::MidiSend => "midi_out_1".to_owned(),
                PortKind::MidiOut => panic!("no wet MIDI"),
            };
            stack.set_port_connected(port.id, &name, true).unwrap();
            assert_eq!(
                stack
                    .mirror
                    .port_of(stack.mirror.engine_port(target).unwrap()),
                Some(port.id)
            );
        }
        stack.wait_idle();
        assert!(stack.poll().unwrap().mutation_failures.is_empty());
        let links = stack.mirror.links.clone();
        refresh(&mut stack);
        assert_eq!(stack.mirror.links, links);
        for (kind, channels) in [
            (shoop_app_api::TrackProcessorTypeId::BUILTIN_FX, 1),
            (shoop_app_api::TrackProcessorTypeId::OXISYNTH, 2),
        ] {
            let created = stack
                .create_track_topology(
                    kind,
                    BackendTrackTopology::DryWetProcessor {
                        processor_type: kind.to_owned(),
                        dry_audio_channels: channels,
                        wet_audio_channels: channels,
                        dry_midi: true,
                    },
                    1,
                )
                .unwrap();
            stack.wait_idle();
            assert!(stack.poll().unwrap().mutation_failures.is_empty());
            assert_eq!(created.ports.len(), (2 * channels + 1) as usize);
            assert!(created.ports.iter().all(|port| matches!(
                port.role,
                BackendPortRole::AudioInput
                    | BackendPortRole::AudioOutput
                    | BackendPortRole::MidiInput
            )));
            refresh(&mut stack);
            assert!(stack.mirror.tracks[&created.track_id]
                .processor_slot
                .is_some());
        }
    }
    #[shoop_test]
    fn global_control_connections_use_the_engine_model_and_survive_resynchronization() {
        let mut stack = stack();
        stack
            .create_track_topology(
                "fx",
                BackendTrackTopology::DryWetProcessor {
                    processor_type: shoop_app_api::TrackProcessorTypeId::BUILTIN_FX.into(),
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                1,
            )
            .unwrap();
        stack.wait_idle();
        stack.capabilities = stack
            .capabilities
            .union(shoop_rt_protocol::Capabilities::GLOBAL_FX_CONTROL);
        let port = shoop_backend::GLOBAL_FX_PORT_ID;
        assert!(stack.set_port_connected(port, "midi_out_1", true).is_err());
        for connected in [true, false] {
            stack
                .set_port_connected(port, "midi_in_1", connected)
                .unwrap();
            stack.wait_idle();
            assert!(stack.poll().unwrap().mutation_failures.is_empty());
            let link = shoop_backend::BackendConfirmedLink {
                application_port_id: port,
                host_port_id: "midi_in_1".into(),
            };
            assert_eq!(stack.mirror.links.contains(&link), connected);
            refresh(&mut stack);
            assert_eq!(stack.mirror.links.contains(&link), connected);
        }
    }
    #[shoop_test]
    fn fx_editor_controls_drive_real_audio_and_project_midi_changes_without_client_dsp() {
        use shoop_app_api::{
            BuiltInFxControl as Fx, BuiltInFxMidiCcAssignment, BuiltInFxParameter as Parameter,
            BuiltInFxStage, TrackProcessorEditorState,
        };
        use shoop_backend::BackendTrackFxControl as Control;
        let mut stack = stack();
        let created = stack
            .create_track_topology(
                "fx",
                BackendTrackTopology::DryWetProcessor {
                    processor_type: shoop_app_api::TrackProcessorTypeId::BUILTIN_FX.into(),
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                1,
            )
            .unwrap();
        stack.wait_idle();
        for (port, host) in [
            (created.ports[0].id, "audio_in_1"),
            (created.ports[1].id, "audio_out_1"),
            (created.ports[2].id, "midi_in_1"),
        ] {
            stack.set_port_connected(port, host, true).unwrap();
        }
        for control in [
            Fx::SetReverbEnabled(false),
            Fx::SetStageEnabled(BuiltInFxStage::Drive, true),
            Fx::SetParameter(Parameter::DriveMix, 0.),
            Fx::SetParameter(Parameter::DriveOutput, -6.),
            Fx::AssignMidiCc(BuiltInFxMidiCcAssignment {
                parameter: Parameter::Drive,
                channel: 0,
                controller: 7,
            }),
        ] {
            stack
                .set_track_fx_control(created.track_id, Control::BuiltInFx(control))
                .unwrap();
        }
        stack
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        stack.clock.0.input_mut(0).fill(1.);
        stack.wait_idle();
        for _ in 0..16 {
            stack.clock.wait();
            stack.pump();
        }
        assert!(stack.clock.0.output(0)[..16]
            .iter()
            .all(|&sample| (sample - 0.5011872).abs() < 1e-5));
        stack
            .clock
            .0
            .midi_input_mut(0)
            .push(3, 16, &[0xb0, 7, 127])
            .unwrap();
        for _ in 0..8 {
            stack.clock.wait();
            stack.pump();
        }
        let mut snapshot = stack.poll().unwrap();
        assert!(snapshot.mutation_failures.is_empty());
        let fx = snapshot
            .tracks
            .remove(&created.track_id)
            .unwrap()
            .fx
            .unwrap();
        let Some(TrackProcessorEditorState::BuiltInFx(editor)) = fx.editor else {
            panic!("FX editor");
        };
        assert!(fx.active);
        assert_eq!(editor.drive_db, 36.);
        assert_eq!(editor.drive_output_db, -6.);
        assert_eq!(editor.midi_cc_assignments.len(), 1);
        stack
            .set_track_fx_control(created.track_id, Control::SetActive(false))
            .unwrap();
        stack.wait_idle();
        for _ in 0..8 {
            stack.clock.wait();
            stack.pump();
        }
        assert_eq!(&stack.clock.0.output(0)[..16], &[0.; 16]);
        assert!(
            !stack.poll().unwrap().tracks[&created.track_id]
                .fx
                .as_ref()
                .unwrap()
                .active
        );
        stack
            .set_track_fx_control(created.track_id, Control::SetActive(true))
            .unwrap();
        stack.wait_idle();
        for _ in 0..8 {
            stack.clock.wait();
            stack.pump();
        }
        assert!(stack.clock.0.output(0)[..16]
            .iter()
            .any(|&sample| sample > 0.4));
        refresh(&mut stack);
        assert_eq!(
            stack.mirror.tracks[&created.track_id]
                .processor_snapshot
                .unwrap()
                .values[5],
            36.
        );
        let mut state =
            shoop_processor_protocol::state::State::new(shoop_processor_protocol::Kind::BuiltInFx)
                .unwrap();
        state.values[5] = 9.;
        state.values[28] = 0.;
        stack
            .set_track_fx_control(created.track_id, Control::RestoreState(state.encode()))
            .unwrap();
        stack.wait_idle();
        let values = stack.mirror.tracks[&created.track_id]
            .processor_snapshot
            .unwrap()
            .values;
        assert_eq!(values[5], 9.);
        assert_eq!(values[37], 7., "envelope restoration keeps live mappings");
        assert!(stack
            .set_track_fx_control(
                created.track_id,
                Control::BuiltInFx(Fx::SetParameter(Parameter::Drive, f32::NAN))
            )
            .is_err());
        assert!(stack
            .set_track_fx_control(created.track_id, Control::RestoreState("broken".into()))
            .is_err());
        stack
            .set_track_fx_control(created.track_id, Control::SetVisible(true))
            .unwrap();
        assert!(
            stack.poll().unwrap().tracks[&created.track_id]
                .fx
                .as_ref()
                .unwrap()
                .visible
        );
        stack
            .set_track_fx_control(
                created.track_id,
                Control::BuiltInFx(Fx::RemoveMidiCc(Parameter::Drive)),
            )
            .unwrap();
        stack.wait_idle();
        assert_eq!(
            stack.mirror.tracks[&created.track_id]
                .processor_snapshot
                .unwrap()
                .values[37],
            -1.
        );
    }
    #[shoop_test]
    fn synth_editor_uses_host_presets_and_confirms_sends_mappings_and_panic() {
        use shoop_app_api::{
            OxiSynthControl as Synth, OxiSynthMidiCcAssignment, OxiSynthParameter,
            TrackProcessorEditorState,
        };
        use shoop_backend::BackendTrackFxControl as Control;
        let mut stack = stack();
        let created = stack
            .create_track_topology(
                "synth",
                BackendTrackTopology::DryWetProcessor {
                    processor_type: shoop_app_api::TrackProcessorTypeId::OXISYNTH.into(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                1,
            )
            .unwrap();
        stack.wait_idle();
        let preset = *stack.processor_presets.last().unwrap();
        for control in [
            Synth::SelectPreset(format!("{}:{}", preset.bank, preset.program)),
            Synth::SetReverbSend(0.75),
            Synth::SetChorusSend(0.25),
            Synth::AssignMidiCc(OxiSynthMidiCcAssignment {
                parameter: OxiSynthParameter::ChorusSend,
                channel: 15,
                controller: 127,
            }),
            Synth::Panic,
        ] {
            stack
                .set_track_fx_control(created.track_id, Control::OxiSynth(control))
                .unwrap();
            stack.wait_idle();
        }
        let mut snapshot = stack.poll().unwrap();
        assert!(snapshot.mutation_failures.is_empty());
        let Some(TrackProcessorEditorState::OxiSynth(editor)) = snapshot
            .tracks
            .remove(&created.track_id)
            .unwrap()
            .fx
            .unwrap()
            .editor
        else {
            panic!("synth editor");
        };
        assert_eq!(
            editor.selected_preset_id,
            format!("{}:{}", preset.bank, preset.program)
        );
        assert_eq!(editor.reverb_send, 0.75);
        assert_eq!(editor.chorus_send, 0.25);
        assert_eq!(editor.midi_cc_assignments.len(), 1);
        assert!(stack
            .set_track_fx_control(
                created.track_id,
                Control::OxiSynth(Synth::SelectPreset("unknown".into()))
            )
            .is_err());
        assert!(stack
            .set_track_fx_control(
                created.track_id,
                Control::OxiSynth(Synth::SetReverbSend(-1.))
            )
            .is_err());
        assert!(stack
            .set_track_fx_control(
                created.track_id,
                Control::BuiltInFx(shoop_app_api::BuiltInFxControl::SetReverbEnabled(false))
            )
            .is_err());
        stack
            .set_track_fx_control(
                created.track_id,
                Control::OxiSynth(Synth::ClearMidiCcAssignments),
            )
            .unwrap();
        stack.wait_idle();
        refresh(&mut stack);
        assert_eq!(
            stack.mirror.tracks[&created.track_id]
                .processor_snapshot
                .unwrap()
                .values[4],
            -1.
        );
    }
    #[shoop_test]
    fn processed_default_playback_is_confirmed_resynced_and_independent_of_loop_mode() {
        use shoop_backend::BackendDefaultPlaybackMode;
        let mut stack = stack();
        let created = stack
            .create_track_topology(
                "external",
                BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 2,
                    dry_midi: false,
                },
                1,
            )
            .unwrap();
        stack.wait_idle();
        for mode in [
            BackendDefaultPlaybackMode::DryThroughWet,
            BackendDefaultPlaybackMode::Regular,
        ] {
            stack
                .set_track_default_playback_mode(created.track_id, mode)
                .unwrap();
            stack.wait_idle();
            assert!(stack.poll().unwrap().mutation_failures.is_empty());
            assert_eq!(
                stack.mirror.tracks[&created.track_id].default_playback_mode,
                mode
            );
            refresh(&mut stack);
            assert_eq!(
                stack.mirror.tracks[&created.track_id].default_playback_mode,
                mode
            );
            assert_eq!(
                stack.mirror.loops[&created.loops[0]].view.mode,
                shoop_rt_protocol::LoopMode::Stopped
            );
        }
        let direct = stack.create_strip("direct", 1, false, 1).unwrap();
        stack.wait_idle();
        assert!(stack
            .set_track_default_playback_mode(
                direct.track_id,
                BackendDefaultPlaybackMode::DryThroughWet
            )
            .is_err());
        assert_eq!(
            stack.mirror.tracks[&direct.track_id].default_playback_mode,
            BackendDefaultPlaybackMode::Regular
        );
    }
    #[shoop_test]
    fn rerecord_monitoring_is_confirmed_and_survives_resync_and_a_later_user_change() {
        let mut stack = stack();
        let created = stack
            .create_track_topology(
                "external",
                BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                },
                1,
            )
            .unwrap();
        stack.wait_idle();
        stack
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        stack.wait_idle();
        assert!(
            stack.mirror.tracks[&created.track_id]
                .controls
                .input_monitoring
        );
        let loop_id = stack.mirror.loops[&created.loops[0]].slot;
        let mut transition = |mode| {
            stack
                .client
                .request(Request::LoopTransition {
                    loop_id,
                    mode,
                    delay: None,
                })
                .unwrap();
            for _ in 0..8 {
                stack.clock.wait();
                stack.pump();
            }
        };
        transition(shoop_rt_protocol::LoopMode::Recording);
        transition(shoop_rt_protocol::LoopMode::Playing);
        transition(shoop_rt_protocol::LoopMode::RecordingDryIntoWet);
        assert!(
            !stack.mirror.tracks[&created.track_id]
                .controls
                .input_monitoring
        );
        refresh(&mut stack);
        assert!(
            !stack.mirror.tracks[&created.track_id]
                .controls
                .input_monitoring
        );
        stack
            .client
            .request(Request::LoopTransition {
                loop_id,
                mode: shoop_rt_protocol::LoopMode::Playing,
                delay: None,
            })
            .unwrap();
        for _ in 0..8 {
            stack.clock.wait();
            stack.pump();
        }
        stack
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        stack.wait_idle();
        assert!(
            stack.mirror.tracks[&created.track_id]
                .controls
                .input_monitoring
        );
        refresh(&mut stack);
        assert!(
            stack.mirror.tracks[&created.track_id]
                .controls
                .input_monitoring
        );
    }
    #[shoop_test]
    fn a_processor_type_limit_covers_different_track_widths_without_reserving_failed_ids() {
        let mut stack = stack();
        let topology = |channels| BackendTrackTopology::DryWetProcessor {
            processor_type: shoop_app_api::TrackProcessorTypeId::BUILTIN_FX.to_owned(),
            dry_audio_channels: channels,
            wet_audio_channels: channels,
            dry_midi: true,
        };
        stack.create_track_topology("mono", topology(1), 1).unwrap();
        stack.wait_idle();
        let free = stack.mirror.strip_slots.free();
        assert!(stack
            .create_track_topology("stereo", topology(2), 1)
            .is_err());
        assert_eq!(stack.mirror.strip_slots.free(), free);
        assert_eq!(stack.mirror.tracks.len(), 1);
    }

    #[shoop_test]
    fn rerecord_projects_retained_raw_dry_and_midi_windows_after_import_and_reconnect() {
        use shoop_backend::{
            BackendAudioChannelUpdate, BackendLoopContentUpdate, BackendLoopMode,
            BackendMidiChannelUpdate, BackendMidiEvent,
        };
        let mut stack = stack();
        let created = stack
            .create_track_topology(
                "windows",
                BackendTrackTopology::DryWetProcessor {
                    processor_type: shoop_app_api::TrackProcessorTypeId::BUILTIN_FX.into(),
                    dry_audio_channels: 3,
                    wet_audio_channels: 3,
                    dry_midi: true,
                },
                1,
            )
            .unwrap();
        stack.wait_idle();
        let id = created.loops[0];
        stack
            .replace_loop_content(
                id,
                &BackendLoopContentUpdate {
                    length: Some(64),
                    audio: [96, 80, 96, 128, 96, 112]
                        .into_iter()
                        .enumerate()
                        .map(|(channel, frames)| BackendAudioChannelUpdate {
                            channel,
                            samples: alloc::vec![channel as f32 + 0.25; frames].into(),
                            start_offset: Some(if channel == 0 { -3 } else { 0 }),
                            capture_alignment_frames: Some(0),
                            preplay: Some(7),
                        })
                        .collect(),
                    midi: alloc::vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 96,
                        start_state: alloc::vec![alloc::vec![0xb0, 7, 91]],
                        events: alloc::vec![
                            BackendMidiEvent {
                                frame: 4,
                                data: alloc::vec![0x90, 60, 90].into()
                            },
                            BackendMidiEvent {
                                frame: 12,
                                data: alloc::vec![0x80, 60, 0].into()
                            }
                        ]
                        .into(),
                        start_offset: Some(2),
                        capture_alignment_frames: Some(0),
                        preplay: Some(5),
                    }],
                },
            )
            .unwrap();
        stack.wait_idle();
        stack.canonical_content.remove(&id);
        stack.prepared_content.remove(&id);
        stack.canonical_imports.remove(&id);
        stack.reads.forget(id);
        let before = stack.capture_session().unwrap().tracks[0].loops[0].clone();
        stack
            .transition_loop(id, BackendLoopMode::RecordingDryIntoWet, None)
            .unwrap();
        stack.wait_idle();
        for _ in 0..8 {
            stack.clock.wait();
        }
        stack
            .transition_loop(id, BackendLoopMode::Stopped, None)
            .unwrap();
        stack.wait_idle();
        refresh(&mut stack);
        stack.canonical_content.remove(&id);
        stack.prepared_content.remove(&id);
        stack.canonical_imports.remove(&id);
        stack.reads.forget(id);
        let after = stack.capture_session().unwrap().tracks[0].loops[0].clone();
        assert_eq!(after.length, before.length);
        assert_eq!(&after.audio[..3], &before.audio[..3]);
        assert_eq!(after.midi, before.midi);
        for (a, b) in after.audio[3..].iter().zip(&before.audio[3..]) {
            assert_eq!(a.samples.len(), b.samples.len());
            assert_eq!(a.start_offset, b.start_offset);
            assert_eq!(a.capture_alignment_frames, 0);
            assert_eq!(a.preplay, b.preplay);
        }
        assert!(after.recorded_processor_state.is_some());
        let state = stack.poll().unwrap().loops[&id].clone();
        assert_eq!(state.channel_timings.len(), 7);
        assert_eq!(state.channel_timings[0].start_offset, -3);
        assert_eq!(state.channel_timings[6].frames, 96);
    }

    #[shoop_test]
    fn media_updates_keep_independent_dry_wet_banks_and_midi_in_the_rendered_take() {
        use shoop_backend::{
            BackendAudioChannelUpdate, BackendChannelMode, BackendLoopContentUpdate,
            BackendMidiChannelUpdate, BackendMidiEvent,
        };
        for (topology, dry, wet) in [
            (
                BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 3,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                3,
                1,
            ),
            (
                BackendTrackTopology::DryWetProcessor {
                    processor_type: shoop_app_api::TrackProcessorTypeId::BUILTIN_FX.into(),
                    dry_audio_channels: 3,
                    wet_audio_channels: 3,
                    dry_midi: true,
                },
                3,
                3,
            ),
            (
                BackendTrackTopology::DryWetProcessor {
                    processor_type: shoop_app_api::TrackProcessorTypeId::OXISYNTH.into(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                2,
                2,
            ),
        ] {
            let mut stack = stack();
            let created = stack.create_track_topology("media", topology, 1).unwrap();
            stack.wait_idle();
            let id = created.loops[0];
            use shoop_app_api::{
                BuiltInFxControl, BuiltInFxMidiCcAssignment, BuiltInFxParameter, OxiSynthControl,
                OxiSynthMidiCcAssignment, OxiSynthParameter,
            };
            use shoop_backend::BackendTrackFxControl;
            let kind =
                crate::processor_controls::track_kind(&stack.mirror.tracks[&created.track_id]);
            let controls = match kind {
                Some(shoop_rt_protocol::ProcessorKind::BuiltInFx) => alloc::vec![
                    BackendTrackFxControl::BuiltInFx(BuiltInFxControl::SetParameter(
                        BuiltInFxParameter::DriveOutput,
                        -6.
                    )),
                    BackendTrackFxControl::BuiltInFx(BuiltInFxControl::AssignMidiCc(
                        BuiltInFxMidiCcAssignment {
                            parameter: BuiltInFxParameter::DriveOutput,
                            channel: 2,
                            controller: 19
                        }
                    )),
                ],
                Some(shoop_rt_protocol::ProcessorKind::OxiSynth) => alloc::vec![
                    BackendTrackFxControl::OxiSynth(OxiSynthControl::SelectPreset("0:40".into())),
                    BackendTrackFxControl::OxiSynth(OxiSynthControl::SetReverbSend(0.5)),
                    BackendTrackFxControl::OxiSynth(OxiSynthControl::AssignMidiCc(
                        OxiSynthMidiCcAssignment {
                            parameter: OxiSynthParameter::ReverbSend,
                            channel: 2,
                            controller: 19
                        }
                    )),
                ],
                _ => Vec::new(),
            };
            for control in controls {
                stack
                    .set_track_fx_control(created.track_id, control)
                    .unwrap();
            }
            stack
                .set_track_default_playback_mode(
                    created.track_id,
                    shoop_backend::BackendDefaultPlaybackMode::DryThroughWet,
                )
                .unwrap();
            stack.wait_idle();
            let recorded = if wet == dry {
                stack
                    .transition_loop(id, shoop_backend::BackendLoopMode::Recording, None)
                    .unwrap();
                stack.wait_idle();
                for _ in 0..4 {
                    stack.clock.wait();
                }
                stack
                    .transition_loop(id, shoop_backend::BackendLoopMode::Stopped, None)
                    .unwrap();
                stack.wait_idle();
                let captured = stack.capture_session().unwrap();
                let state = captured.tracks[0].loops[0].recorded_processor_state.clone();
                assert!(state.is_some());
                state
            } else {
                None
            };
            let events = alloc::vec![
                BackendMidiEvent {
                    frame: 3,
                    data: alloc::vec![0x90, 60, 99].into()
                },
                BackendMidiEvent {
                    frame: 31,
                    data: alloc::vec![0x80, 60, 0].into()
                }
            ];
            stack
                .replace_loop_content(
                    id,
                    &BackendLoopContentUpdate {
                        length: Some(64),
                        audio: (0..dry + wet)
                            .map(|channel| BackendAudioChannelUpdate {
                                channel,
                                samples: alloc::vec![channel as f32 + 0.25; 64].into(),
                                start_offset: None,
                                capture_alignment_frames: None,
                                preplay: None,
                            })
                            .collect(),
                        midi: alloc::vec![BackendMidiChannelUpdate {
                            channel: 0,
                            length: 64,
                            start_state: alloc::vec![alloc::vec![0xb0, 7, 99]],
                            events: events.clone().into(),
                            start_offset: None,
                            capture_alignment_frames: None,
                            preplay: None
                        }],
                    },
                )
                .unwrap();
            for (channel, value) in [(dry, 9.5), (0, -0.5)] {
                stack
                    .replace_loop_content(
                        id,
                        &BackendLoopContentUpdate {
                            audio: alloc::vec![BackendAudioChannelUpdate {
                                channel,
                                samples: alloc::vec![value; 64].into(),
                                start_offset: None,
                                capture_alignment_frames: None,
                                preplay: None
                            }],
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            // Force an immutable export from runtime storage, beyond the canonical import cache.
            stack.canonical_content.remove(&id);
            stack.prepared_content.remove(&id);
            stack.canonical_imports.remove(&id);
            stack.reads.forget(id);
            let saved = stack.capture_session().unwrap();
            let content = &saved.tracks[0].loops[0];
            assert_eq!(content.recorded_processor_state, recorded);
            assert_eq!(content.audio.len(), dry + wet);
            for (channel, audio) in content.audio.iter().enumerate() {
                assert_eq!(
                    audio.mode,
                    if channel < dry {
                        BackendChannelMode::Dry
                    } else {
                        BackendChannelMode::Wet
                    }
                );
                let expected = if channel == 0 {
                    -0.5
                } else if channel == dry {
                    9.5
                } else {
                    channel as f32 + 0.25
                };
                assert_eq!(audio.samples.as_ref(), &alloc::vec![expected; 64]);
            }
            assert_eq!(content.midi[0].mode, BackendChannelMode::Dry);
            assert_eq!(content.midi[0].events.as_ref(), &events);
            assert_eq!(content.midi[0].start_state.len(), 65);
            assert_eq!(content.midi[0].start_state[0], alloc::vec![0xb0, 7, 99]);
            assert!(content.midi[0].start_state[1..]
                .iter()
                .all(|packet| matches!(
                    packet.as_slice(),
                    [0xe0..=0xef, 0, 64] | [0xb0..=0xbf, 64 | 66 | 69, 0]
                )));
            let before = content.clone();
            for changed in [true, false] {
                let state = stack.poll().unwrap().loops[&id].clone();
                let mut next = before.clone();
                if changed {
                    next.audio[0].samples = alloc::vec![-3.75; 64].into();
                }
                let family = shoop_backend::BackendLoopFamilyUpdate {
                    root: id,
                    sample_rate: 8000,
                    quantum_frames: 16,
                    members: alloc::vec![shoop_backend::BackendLoopFamilyMember {
                        loop_id: id,
                        sync_source: state.sync_source,
                        expected_revision: state.content_revision,
                        expected_frames: before.length,
                        fence_content: true,
                        restart: true,
                        content: Some(next.clone()),
                    }],
                };
                let mut finished = false;
                for _ in 0..10_000 {
                    match stack.replace_loop_family_async(&family).unwrap() {
                        shoop_backend::BackendAsyncResult::Ready(()) => {
                            finished = true;
                            break;
                        }
                        shoop_backend::BackendAsyncResult::Pending(_) => {
                            stack.clock.wait();
                        }
                    }
                }
                assert!(finished, "processed family replacement stalled");
                stack.wait_idle();
                stack.canonical_content.remove(&id);
                stack.prepared_content.remove(&id);
                stack.canonical_imports.remove(&id);
                stack.reads.forget(id);
                let after = stack.capture_session().unwrap();
                assert_eq!(after.tracks[0].loops[0], next);
            }
            assert!(stack
                .replace_loop_content(
                    id,
                    &BackendLoopContentUpdate {
                        audio: alloc::vec![BackendAudioChannelUpdate {
                            channel: dry,
                            samples: alloc::vec![f32::NAN; 64].into(),
                            start_offset: None,
                            capture_alignment_frames: None,
                            preplay: None
                        }],
                        ..Default::default()
                    }
                )
                .is_err());
            assert_eq!(stack.capture_session().unwrap().tracks[0].loops[0], before);
            let mut previous = id;
            for _ in 0..6 {
                let mapping = stack.replace_session(&saved).unwrap();
                let loaded = mapping.loops[&id.raw()];
                assert_ne!(loaded, previous);
                stack.wait_idle();
                let mut after = stack.capture_session().unwrap();
                let track = &mut after.tracks[0];
                assert_eq!(track.topology, saved.tracks[0].topology);
                assert_eq!(
                    track.state.default_playback_mode,
                    saved.tracks[0].state.default_playback_mode
                );
                assert_eq!(track.processor_state, saved.tracks[0].processor_state);
                assert_eq!(
                    track.builtin_fx_midi_cc_assignments,
                    saved.tracks[0].builtin_fx_midi_cc_assignments
                );
                assert_eq!(
                    track.oxisynth_midi_cc_assignments,
                    saved.tracks[0].oxisynth_midi_cc_assignments
                );
                track.loops[0].source_id = id.raw();
                assert_eq!(track.loops[0], before);
                // The prepared replacement has distinct runtime owners, including metadata.
                stack.canonical_content.remove(&loaded);
                stack.prepared_content.remove(&loaded);
                stack.canonical_imports.remove(&loaded);
                stack.reads.forget(loaded);
                let mut rendered = stack
                    .capture_session()
                    .unwrap()
                    .tracks
                    .remove(0)
                    .loops
                    .remove(0);
                rendered.source_id = id.raw();
                assert_eq!(rendered, before);
                previous = loaded;
            }
            if let Some(kind) = kind {
                let before = stack.capture_session().unwrap();
                let active = stack
                    .mirror
                    .tracks
                    .values()
                    .next()
                    .unwrap()
                    .processor_slot
                    .unwrap();
                for _ in 0..3 {
                    stack.start_replacement(&saved).unwrap();
                    let mut prepared = false;
                    for _ in 0..10_000 {
                        stack.clock.wait();
                        let endpoint = stack.client.carrier_mut().endpoint();
                        prepared = (0..endpoint.processor_catalog().kinds.len()).any(|slot| {
                            slot != active.index as usize
                                && endpoint.processor_state(slot).is_some_and(|state| {
                                    state.kind == kind
                                        && state.phase == shoop_rt_engine::ProcessorPhase::Owned
                                })
                        });
                        if prepared {
                            break;
                        }
                        stack.pump();
                    }
                    assert!(
                        prepared,
                        "candidate did not claim its prepared processor bank"
                    );
                    stack.cancel_session_replacement().unwrap();
                    assert!(stack
                        .replace_session(&saved)
                        .unwrap_err()
                        .to_string()
                        .contains("cancelled"));
                    assert_eq!(stack.capture_session().unwrap(), before);
                }
                if kind == shoop_rt_protocol::ProcessorKind::OxiSynth {
                    let mut invalid = saved.clone();
                    let mut state = shoop_processor_protocol::state::State::new(kind).unwrap();
                    state.values[2] = 1024. * 128.;
                    invalid.tracks[0].processor_state = Some(state.encode());
                    assert!(stack.replace_session(&invalid).is_err());
                    assert_eq!(stack.capture_session().unwrap(), before);
                    stack.replace_session(&saved).unwrap();
                }
            }
        }
    }
}
