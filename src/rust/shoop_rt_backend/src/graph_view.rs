//! Read-only semantic graph projection; cursors, meters and transport state stay private.
use crate::{
    mirror::{Mirror, PortTarget},
    EngineClock, RtBackend,
};
use alloc::borrow::Cow;
use shoop_backend::{
    BackendDefaultPlaybackMode, BackendPortDataType, BackendTrackTopology, MASTER_BUS_ID,
};
use shoop_graph::*;
use shoop_rt_protocol::{Direction, HostPortKind, PortKind, SlotId};
use shoop_rt_transport::carrier::Carrier;
fn id(value: SlotId) -> Id {
    Id {
        slot: value.index,
        generation: value.generation,
    }
}
fn layout(topology: &BackendTrackTopology) -> ChannelLayout {
    match *topology {
        BackendTrackTopology::Direct {
            audio_channels,
            midi,
        } => ChannelLayout::Direct {
            audio: audio_channels,
            midi,
        },
        BackendTrackTopology::DryWetExternal {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        }
        | BackendTrackTopology::DryWetProcessor {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
            ..
        } => ChannelLayout::DryWet {
            dry_audio: dry_audio_channels,
            wet_audio: wet_audio_channels,
            dry_midi,
        },
    }
}
impl Mirror {
    fn graph_port(&self, target: PortTarget, signal: SignalType) -> PortId {
        let (owner, role, index) = match target {
            PortTarget::Strip { track, kind, index } => (
                Owner::Strip(id(self.tracks[&track].strip.0)),
                match kind {
                    PortKind::AudioIn | PortKind::MidiIn => PortRole::Input,
                    PortKind::AudioOut | PortKind::MidiOut => PortRole::Output,
                    PortKind::AudioSend | PortKind::MidiSend => PortRole::Send,
                    PortKind::AudioReturn => PortRole::Return,
                },
                u32::from(index),
            ),
            PortTarget::Bus(bus, index) => (
                Owner::Bus(id(self.buses[&bus].slot.0)),
                PortRole::Output,
                u32::from(index),
            ),
            PortTarget::GlobalFx => (Owner::Global, PortRole::Control, 0),
        };
        PortId {
            owner,
            signal,
            role,
            index,
        }
    }
    fn host_graph_port(&self, host: &shoop_rt_protocol::HostPort) -> PortId {
        let signal = match host.kind {
            HostPortKind::Audio => SignalType::Audio,
            HostPortKind::Midi => SignalType::Midi,
        };
        let role = match host.direction {
            Direction::Input => PortRole::Input,
            Direction::Output => PortRole::Output,
        };
        let index = host.id.0;
        PortId {
            owner: Owner::Host,
            signal,
            role,
            index,
        }
    }
}
impl GraphView for Mirror {
    fn strips(&self) -> impl Iterator<Item = StripView<'_>> {
        self.tracks.values().filter(|t| !t.removing).map(|t| {
            let processor = match &t.topology {
                BackendTrackTopology::Direct { .. } => None,
                BackendTrackTopology::DryWetExternal { .. } => Some("external"),
                BackendTrackTopology::DryWetProcessor { processor_type, .. } => {
                    Some(processor_type.as_str())
                }
            }
            .map(|type_id| ProcessorView {
                type_id: Cow::Borrowed(type_id),
                parameters: Cow::Borrowed(
                    t.processor_snapshot
                        .as_ref()
                        .map_or(&[], |p| p.values.as_slice()),
                ),
            });
            let c = t.controls;
            StripView {
                id: id(t.strip.0),
                name: Cow::Borrowed(&t.port_name_base),
                layout: layout(&t.topology),
                controls: StripControls {
                    input_gain_db: c.input_gain_db,
                    input_balance: c.input_balance,
                    output_gain_db: c.output_gain_db,
                    output_balance: c.output_balance,
                    output_muted: c.output_muted,
                    input_monitoring: c.input_monitoring,
                    automatic_monitoring: t.automatic_enabled,
                },
                default_dry_playback: t.default_playback_mode
                    == BackendDefaultPlaybackMode::DryThroughWet,
                processor,
            }
        })
    }
    fn loops(&self) -> impl Iterator<Item = shoop_graph::LoopView<'_>> {
        self.loops
            .values()
            .filter(|l| l.track.is_none_or(|t| !self.tracks[&t].removing))
            .map(|l| {
                let composite = l.program.map(|p| CompositeView {
                    kind: match p.kind {
                        shoop_rt_protocol::CompositeKind::Regular => CompositeKind::Regular,
                        shoop_rt_protocol::CompositeKind::Explicit => CompositeKind::Explicit,
                    },
                    version: p.version,
                    unit_frames: p.unit_frames,
                    iterations: p.iterations,
                    sample_rate: p.sample_rate,
                    quantum: p.quantum,
                    spans: Cow::Owned(
                        l.program_spans
                            .iter()
                            .map(|s| CompositeSpan {
                                target: id(s.target.0),
                                start: s.start,
                                end: s.end,
                                occurrence_start: s.occurrence_start,
                                ordinal: s.ordinal,
                                mode: match s.mode {
                                    shoop_rt_protocol::LoopMode::Stopped => LoopMode::Stopped,
                                    shoop_rt_protocol::LoopMode::Playing => LoopMode::Playing,
                                    shoop_rt_protocol::LoopMode::Recording => LoopMode::Recording,
                                    shoop_rt_protocol::LoopMode::Replacing => LoopMode::Replacing,
                                    shoop_rt_protocol::LoopMode::PlayingDryThroughWet => {
                                        LoopMode::PlayingDryThroughWet
                                    }
                                    shoop_rt_protocol::LoopMode::RecordingDryIntoWet => {
                                        LoopMode::RecordingDryIntoWet
                                    }
                                },
                            })
                            .collect(),
                    ),
                });
                shoop_graph::LoopView {
                    id: id(l.slot.0),
                    strip: l.track.map(|t| id(self.tracks[&t].strip.0)),
                    layout: l.track.map_or(
                        ChannelLayout::Direct {
                            audio: 0,
                            midi: false,
                        },
                        |t| layout(&self.tracks[&t].topology),
                    ),
                    gain: l.gain,
                    balance: l.balance,
                    boundary_smoothing_frames: self.loop_smoothing_frames,
                    sync_source: l
                        .sync
                        .map(|s| id(self.loops[&s].slot.0))
                        .or_else(|| l.program.and_then(|p| p.sync).map(|s| id(s.0))),
                    repeat_sync: l.repeat_sync,
                    composite,
                }
            })
    }
    fn buses(&self) -> impl Iterator<Item = BusView<'_>> {
        self.buses
            .iter()
            .filter(|(_, b)| !b.removing)
            .map(|(&key, b)| {
                let type_id = b.processor_type.as_deref().or_else(|| {
                    b.processor_slot
                        .map(|_| shoop_app_api::TrackProcessorTypeId::BUILTIN_FX)
                });
                BusView {
                    id: id(b.slot.0),
                    name: Cow::Borrowed(&b.name),
                    channels: b.channels.len() as u32,
                    master: key == MASTER_BUS_ID,
                    gain_db: b.gain_db,
                    balance: b.balance,
                    muted: b.muted,
                    processor: type_id.map(|type_id| ProcessorView {
                        type_id: Cow::Borrowed(type_id),
                        parameters: Cow::Borrowed(
                            b.processor_snapshot
                                .as_ref()
                                .map_or(&[], |p| p.values.as_slice()),
                        ),
                    }),
                }
            })
    }
    fn ports(&self) -> impl Iterator<Item = PortView<'_>> {
        let local = self
            .ports
            .values()
            .filter(|p| match p.target {
                PortTarget::Strip { track, .. } => !self.tracks[&track].removing,
                PortTarget::Bus(bus, _) => !self.buses[&bus].removing,
                PortTarget::GlobalFx => true,
            })
            .map(|p| PortView {
                channel: match p.target {
                    PortTarget::Strip { index, .. } | PortTarget::Bus(_, index) => u32::from(index),
                    PortTarget::GlobalFx => 0,
                },
                id: self.graph_port(
                    p.target,
                    match p.descriptor.data_type {
                        BackendPortDataType::Audio => SignalType::Audio,
                        BackendPortDataType::Midi => SignalType::Midi,
                    },
                ),
                name: Cow::Borrowed(&p.descriptor.name),
            });
        let inputs = self.buses.values().filter(|b| !b.removing).flat_map(|b| {
            b.channels.iter().enumerate().map(move |(i, c)| PortView {
                channel: i as u32,
                id: PortId {
                    owner: Owner::Bus(id(b.slot.0)),
                    signal: SignalType::Audio,
                    role: PortRole::Input,
                    index: i as u32,
                },
                name: Cow::Borrowed(&c.label),
            })
        });
        local.chain(inputs).chain(self.host_ports.iter().map(|h| {
            PortView {
                channel: self
                    .host_ports
                    .iter()
                    .filter(|p| p.kind == h.kind && p.direction == h.direction)
                    .position(|p| p.id == h.id)
                    .unwrap() as u32,
                id: self.host_graph_port(h),
                name: Cow::Borrowed(self.host_key(h)),
            }
        }))
    }
    fn connections(&self) -> impl Iterator<Item = Connection> {
        let host = self.links.iter().map(|l| {
            let local = &self.ports[&l.application_port_id];
            let host = self
                .host_by_name(&l.host_port_id)
                .expect("confirmed host link");
            let port = self.graph_port(
                local.target,
                match local.descriptor.data_type {
                    BackendPortDataType::Audio => SignalType::Audio,
                    BackendPortDataType::Midi => SignalType::Midi,
                },
            );
            let other = self.host_graph_port(host);
            if other.is_source() {
                Connection {
                    source: other,
                    destination: port,
                }
            } else {
                Connection {
                    source: port,
                    destination: other,
                }
            }
        });
        host.chain(self.routes.iter().map(|r| {
            let port = &self.ports[&r.source_port_id];
            let (bus, index) = self
                .buses
                .values()
                .find_map(|b| {
                    b.channels
                        .iter()
                        .position(|c| c.id == r.destination_channel_id)
                        .map(|i| (b, i))
                })
                .expect("confirmed mixer link");
            Connection {
                source: self.graph_port(port.target, SignalType::Audio),
                destination: PortId {
                    owner: Owner::Bus(id(bus.slot.0)),
                    signal: SignalType::Audio,
                    role: PortRole::Input,
                    index: index as u32,
                },
            }
        }))
        .filter(|c| {
            let live = |owner| match owner {
                Owner::Strip(s) => self
                    .tracks
                    .values()
                    .any(|t| id(t.strip.0) == s && !t.removing),
                Owner::Bus(s) => self
                    .buses
                    .values()
                    .any(|b| id(b.slot.0) == s && !b.removing),
                _ => true,
            };
            live(c.source.owner) && live(c.destination.owner)
        })
    }
}
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub fn graph_view(&self) -> impl GraphView + '_ {
        &self.mirror
    }
}
