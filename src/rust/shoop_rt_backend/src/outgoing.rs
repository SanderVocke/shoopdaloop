//! Requests built from application operations: identities are reserved here and the
//! requests queued until credits allow sending them.

use anyhow::{anyhow, Result};
use shoop_backend::{
    BackendBusChannelId, BackendBusControl, BackendConfirmedLink, BackendLoopId, BackendMixerLink,
    BackendPortDataType, BackendPortDescriptor, BackendPortDirection, BackendPortId,
    BackendPortOwner, BackendPortRole, BackendTrackControl, BackendTrackCreation, BackendTrackId,
    MASTER_BUS_CHANNEL_IDS,
};
use shoop_rt_protocol::carrier::Carrier;
use shoop_rt_protocol::{
    BusControl, BusId, BusPort, EnginePort, LoopSlotId, PortKind, Request, StripControl, StripId,
    StripPort, StripShape,
};

use crate::mirror::{
    db_amplitude, host_data_type, host_direction, Controls, Loop, LoopView, Port, PortTarget, Track,
};
use crate::requests::Purpose;
use crate::{EngineClock, RtBackend};

/// The engine request for an application mixer link.
fn route_request(port: StripPort, channel: u8, connected: bool) -> Request {
    Request::Route {
        port,
        bus: BusPort {
            bus: BusId::Master,
            index: channel,
        },
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
        self.ready()?;
        let limits = self.limits();
        if audio_channels > u32::from(limits.strip_audio_channels) {
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
        for index in 0..audio_channels {
            let suffix = if audio_channels == 1 {
                String::new()
            } else {
                format!("_{}", index + 1)
            };
            shapes.push((
                format!("{port_name_base}_direct_in{suffix}"),
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
                PortKind::AudioIn,
                index as u8,
            ));
            shapes.push((
                format!("{port_name_base}_direct_out{suffix}"),
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
                PortKind::AudioOut,
                index as u8,
            ));
        }
        if midi {
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
            },
        );
        self.mirror.strip_tracks.insert(strip, track_id);
        let channels = audio_channels as u8;
        self.requests.push(
            Request::CreateStrip {
                strip,
                shape: StripShape {
                    audio_inputs: channels,
                    audio_outputs: channels,
                    midi_input: midi,
                    midi_output: midi,
                },
            },
            Purpose::CreateStrip(track_id),
        );
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
                slot,
                track: track_id,
                confirmed: false,
                sync: None,
                gain: 1.0,
                balance: 0.0,
                view: LoopView::default(),
                meter: Default::default(),
                content: 0,
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
        let port = self
            .mirror
            .live_port(port_id)
            .ok_or_else(|| anyhow!("unknown port {port_id:?}"))?;
        let host_port = self
            .mirror
            .host_by_name(host)
            .ok_or_else(|| anyhow!("unknown host port {host}"))?;
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
        let channel = MASTER_BUS_CHANNEL_IDS
            .iter()
            .position(|id| *id == destination_channel_id)
            .ok_or_else(|| anyhow!("unknown bus channel {destination_channel_id:?}"))?;
        let Some(EnginePort::Strip(strip_port)) = self.mirror.engine_port(port.target) else {
            return Err(anyhow!("mixer source is not a track audio output"));
        };
        self.requests.push(
            route_request(strip_port, channel as u8, connected),
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

    pub(crate) fn queue_master_control(&mut self, control: BackendBusControl) -> Result<()> {
        let control = control.normalized(MASTER_BUS_CHANNEL_IDS.len())?;
        let engine = match control {
            BackendBusControl::GainDb(value) => BusControl::Gain(db_amplitude(value)),
            BackendBusControl::Balance(value) => BusControl::Balance(value),
            BackendBusControl::Mute(value) => BusControl::Mute(value),
        };
        self.requests.push(
            Request::BusControl {
                bus: BusId::Master,
                control: engine,
            },
            Purpose::BusControl(control),
        );
        Ok(())
    }
}
