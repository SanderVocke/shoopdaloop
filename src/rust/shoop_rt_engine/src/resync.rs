//! The full-state record stream, emitted as reliable outbox space allows.

use crate::endpoint::ControlEndpoint;
use shoop_rt_protocol::{
    BusControl, BusPort, EngineMessage, EnginePort, HostPortId, Lane, PortKind, StateRecord,
    StripPort,
};
use shoop_rt_transport::carrier::Outbox;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Resync {
    stage: u8,
    index: usize,
    part: usize,
    channel: usize,
    bit: usize,
    epoch: u64,
}

/// The stage after `Begin` that reports generation floors.
const GENERATIONS: u8 = 7;

const PORT_KINDS: [PortKind; 7] = [
    PortKind::AudioIn,
    PortKind::AudioOut,
    PortKind::MidiIn,
    PortKind::MidiOut,
    PortKind::AudioSend,
    PortKind::AudioReturn,
    PortKind::MidiSend,
];

impl ControlEndpoint {
    fn recorded_record(
        &self,
        id: shoop_rt_protocol::LoopSlotId,
    ) -> Result<Option<StateRecord>, ()> {
        let entry = self.model.find_loop(id).ok_or(())?;
        let strip = self.model.strip(entry.strip).ok_or(())?;
        if strip.processor.and_then(|owner| owner.slot).is_none() {
            return Ok(None);
        }
        let cells = &self.cells.loops[id.0.index as usize];
        let current =
            cells.generation.load(core::sync::atomic::Ordering::Relaxed) == id.0.generation;
        let revision = if current {
            cells
                .content_revision
                .load(core::sync::atomic::Ordering::Relaxed)
        } else {
            0
        };
        let state = cells.recorded.read(id.0.generation, revision)?;
        Ok(Some(StateRecord::RecordedProcessorState {
            loop_id: id,
            content_revision: revision,
            state,
        }))
    }

    pub(crate) fn project_recorded(&mut self, out: &mut Outbox) {
        if !self.connected || self.resync.is_some() {
            return;
        }
        for index in 0..self.model.loops.len() {
            let Some(entry) = self.model.loops[index] else {
                self.projected_recorded[index] = None;
                continue;
            };
            let Ok(Some(
                record @ StateRecord::RecordedProcessorState {
                    loop_id,
                    content_revision,
                    state,
                },
            )) = self.recorded_record(entry.id)
            else {
                continue;
            };
            let key = (loop_id, content_revision, state.is_some());
            if self.projected_recorded[index] == Some(key) {
                continue;
            }
            if out.push(EngineMessage::State(record)).is_err() {
                return;
            }
            self.projected_recorded[index] = Some(key);
        }
    }

    pub(crate) fn processor_record(
        &self,
        strip: shoop_rt_protocol::StripId,
    ) -> Option<StateRecord> {
        let owner = self.model.strip(strip)?.processor?.slot?;
        let state = self.processor_cells.get(owner.index as usize)?.read()?;
        if state.generation != owner.generation {
            return None;
        }
        Some(StateRecord::ProcessorState {
            strip,
            slot: owner,
            snapshot: shoop_rt_protocol::processors::Snapshot {
                source_frame: state.frame.0,
                phase: state.phase,
                global_midi: state.global_midi,
                values: state.values,
            },
        })
    }

    pub(crate) fn project_processors(&mut self, out: &mut Outbox) {
        if !self.connected || self.resync.is_some() {
            return;
        }
        for index in 0..self.model.strips.len() {
            let Some(strip) = self.model.strips[index] else {
                self.projected_processors[index] = None;
                continue;
            };
            let Some(StateRecord::ProcessorState {
                strip,
                slot,
                snapshot,
            }) = self.processor_record(strip.id)
            else {
                continue;
            };
            if self.projected_processors[index].is_some_and(
                |(previous_strip, previous_slot, previous)| {
                    previous_strip == strip
                        && previous_slot == slot
                        && previous.phase == snapshot.phase
                        && previous.global_midi == snapshot.global_midi
                        && previous
                            .values
                            .iter()
                            .zip(snapshot.values)
                            .all(|(a, b)| a.to_bits() == b.to_bits())
                },
            ) {
                continue;
            }
            if out
                .push(EngineMessage::State(StateRecord::ProcessorState {
                    strip,
                    slot,
                    snapshot,
                }))
                .is_err()
            {
                return;
            }
            self.projected_processors[index] = Some((strip, slot, snapshot));
        }
    }
    fn host_port_id(&self, kind: PortKind, index: usize) -> HostPortId {
        let [audio_in, audio_out, midi_in, _] = self.settings.host;
        let first = match kind {
            PortKind::AudioIn | PortKind::AudioReturn => 0,
            PortKind::AudioOut | PortKind::AudioSend => audio_in,
            PortKind::MidiIn => audio_in + audio_out,
            PortKind::MidiOut | PortKind::MidiSend => audio_in + audio_out + midi_in,
        };
        HostPortId((first + index) as u32)
    }

    /// The next connection of the current strip at or after the cursor.
    fn next_connection(&self, cursor: &mut Resync) -> Option<StateRecord> {
        let strip = self.model.strips[cursor.index]?;
        while cursor.part < PORT_KINDS.len() {
            let kind = PORT_KINDS[cursor.part];
            let external = strip
                .processor
                .is_some_and(|owner| owner.kind == shoop_rt_protocol::ProcessorKind::External);
            let (channels, masks): (usize, &[crate::model::HostMask]) = match kind {
                PortKind::AudioSend => (
                    if external {
                        strip.shape.audio_inputs as usize
                    } else {
                        0
                    },
                    strip.sends.current(),
                ),
                PortKind::AudioReturn => (
                    if external {
                        strip.shape.audio_outputs as usize
                    } else {
                        0
                    },
                    strip.returns.current(),
                ),
                PortKind::MidiSend => (
                    usize::from(external && strip.shape.midi_input),
                    core::slice::from_ref(&strip.midi_send),
                ),
                PortKind::AudioIn => (strip.shape.audio_inputs as usize, strip.inputs.current()),
                PortKind::AudioOut => (strip.shape.audio_outputs as usize, strip.outputs.current()),
                PortKind::MidiIn => (
                    usize::from(strip.shape.midi_input),
                    core::slice::from_ref(&strip.midi_inputs),
                ),
                PortKind::MidiOut => (
                    usize::from(strip.shape.midi_output),
                    core::slice::from_ref(&strip.midi_outputs),
                ),
            };
            while cursor.channel < channels {
                let mask = masks[cursor.channel];
                while cursor.bit < crate::MAX_HOST_PORTS {
                    let bit = cursor.bit;
                    cursor.bit += 1;
                    if mask.contains(bit) {
                        return Some(StateRecord::Connection {
                            port: EnginePort::Strip(StripPort {
                                strip: strip.id,
                                kind,
                                index: cursor.channel as u8,
                            }),
                            host: self.host_port_id(kind, bit),
                        });
                    }
                }
                cursor.bit = 0;
                cursor.channel += 1;
            }
            cursor.channel = 0;
            cursor.part += 1;
        }
        None
    }

    fn next_state(&self, cursor: &mut Resync) -> Option<StateRecord> {
        loop {
            match cursor.stage {
                0 => {
                    cursor.stage = GENERATIONS;
                    cursor.epoch = self.model.revision;
                    return Some(StateRecord::Begin {
                        epoch: cursor.epoch,
                    });
                }
                12 => {
                    while cursor.bit < crate::MAX_HOST_PORTS {
                        let bit = cursor.bit;
                        cursor.bit += 1;
                        if self.model.global_midi.contains(bit) {
                            return Some(StateRecord::Connection {
                                port: EnginePort::GlobalFxControl,
                                host: self.host_port_id(PortKind::MidiIn, bit),
                            });
                        }
                    }
                    cursor.stage = 5;
                }
                GENERATIONS => {
                    cursor.stage = 8;
                    let floor = |generations: &[u32]| generations.iter().copied().max();
                    return Some(StateRecord::Generations {
                        strips: floor(&self.model.strip_generations).unwrap_or(0),
                        loops: floor(&self.model.loop_generations).unwrap_or(0),
                    });
                }
                8 => {
                    cursor.stage = 9;
                    return Some(StateRecord::BusGenerations {
                        generation: self
                            .model
                            .bus_generations
                            .iter()
                            .copied()
                            .max()
                            .unwrap_or(0),
                    });
                }
                9 => {
                    cursor.stage = 10;
                    cursor.index = 0;
                    return Some(StateRecord::Processors(
                        shoop_rt_protocol::ProcessorLimits {
                            effective: self.processor_catalog.effective,
                            reserved_bytes: self.processor_catalog.reserved_bytes,
                            channels: self.settings.channels as u8,
                            builtin_latency_frames: self.limits.quantum,
                        },
                    ));
                }
                10 => {
                    if let Some(preset) = self.processor_catalog.presets.get(cursor.index) {
                        cursor.index += 1;
                        return Some(StateRecord::ProcessorPreset(*preset));
                    }
                    cursor.stage = 1;
                    cursor.index = 0;
                }
                1 => {
                    if let Some(port) = self.host_ports.get(cursor.index) {
                        cursor.index += 1;
                        return Some(StateRecord::HostPort(*port));
                    }
                    *cursor = Resync {
                        stage: 2,
                        epoch: cursor.epoch,
                        ..Resync::default()
                    };
                }
                2 => {
                    let Some(slot) = self.model.strips.get(cursor.index) else {
                        *cursor = Resync {
                            stage: 3,
                            epoch: cursor.epoch,
                            ..Resync::default()
                        };
                        continue;
                    };
                    let Some(strip) = slot else {
                        cursor.index += 1;
                        continue;
                    };
                    let controls = strip.controls.all();
                    match cursor.part {
                        0 => {
                            cursor.part = 1;
                            return Some(StateRecord::Strip {
                                strip: strip.id,
                                shape: strip.shape,
                            });
                        }
                        part if part <= controls.len() => {
                            cursor.part += 1;
                            return Some(StateRecord::StripControl {
                                strip: strip.id,
                                control: controls[part - 1],
                            });
                        }
                        part if part == controls.len() + 1 => {
                            let routes = &strip.outputs.buses[strip.outputs.active];
                            while cursor.channel < strip.shape.audio_outputs as usize {
                                while cursor.bit
                                    < self.model.buses.len() * shoop_rt_protocol::MAX_STRIP_CHANNELS
                                {
                                    let bit = cursor.bit;
                                    cursor.bit += 1;
                                    if routes[cursor.channel] & (1 << bit) != 0 {
                                        return Some(StateRecord::Route {
                                            port: StripPort {
                                                strip: strip.id,
                                                kind: PortKind::AudioOut,
                                                index: cursor.channel as u8,
                                            },
                                            bus: BusPort {
                                                bus: self.model.buses
                                                    [bit / shoop_rt_protocol::MAX_STRIP_CHANNELS]?
                                                    .id,
                                                index: (bit % shoop_rt_protocol::MAX_STRIP_CHANNELS)
                                                    as u8,
                                            },
                                        });
                                    }
                                }
                                cursor.bit = 0;
                                cursor.channel += 1;
                            }
                            cursor.channel = 0;
                            cursor.part += 1;
                        }
                        part if part == controls.len() + 2 => {
                            cursor.part += 1;
                            if let Some(owner) = strip.processor {
                                return Some(StateRecord::StripProcessor {
                                    strip: strip.id,
                                    kind: owner.kind,
                                    slot: owner.slot,
                                });
                            }
                        }
                        part if part == controls.len() + 3 => {
                            cursor.part += 1;
                            return Some(StateRecord::StripDefaultPlayback {
                                strip: strip.id,
                                dry_through_wet: strip.default_dry,
                            });
                        }
                        part if part == controls.len() + 4 => {
                            if strip.processor.and_then(|owner| owner.slot).is_some() {
                                let record = self.processor_record(strip.id)?;
                                cursor.part += 1;
                                return Some(record);
                            }
                            cursor.part += 1;
                        }
                        _ => {
                            let mut connections = Resync {
                                index: cursor.index,
                                part: cursor.part - controls.len() - 5,
                                ..*cursor
                            };
                            let record = self.next_connection(&mut connections);
                            cursor.part = connections.part + controls.len() + 5;
                            cursor.channel = connections.channel;
                            cursor.bit = connections.bit;
                            if record.is_some() {
                                return record;
                            }
                            cursor.index += 1;
                            cursor.part = 0;
                            cursor.channel = 0;
                            cursor.bit = 0;
                        }
                    }
                }
                3 => {
                    let Some(slot) = self.model.loops.get(cursor.index) else {
                        *cursor = Resync {
                            stage: 4,
                            epoch: cursor.epoch,
                            ..Resync::default()
                        };
                        continue;
                    };
                    if let Some(entry) = slot {
                        if cursor.part == 0 {
                            cursor.part = 1;
                            return Some(StateRecord::Loop {
                                loop_id: entry.id,
                                strip: entry.strip,
                                sync: entry.sync,
                                gain: entry.gain,
                                balance: entry.balance,
                            });
                        }
                        let record = self.recorded_record(entry.id).ok()?;
                        cursor.index += 1;
                        cursor.part = 0;
                        if record.is_some() {
                            return record;
                        }
                    } else {
                        cursor.index += 1;
                    }
                }
                4 => {
                    let Some(slot) = self.model.buses.get(cursor.index) else {
                        cursor.stage = 12;
                        cursor.bit = 0;
                        continue;
                    };
                    let Some(bus) = slot else {
                        cursor.index += 1;
                        continue;
                    };
                    if cursor.part == 0 {
                        cursor.part += 1;
                        return Some(StateRecord::BusShape {
                            bus: bus.id,
                            channels: bus.channels,
                        });
                    }
                    let controls = [
                        BusControl::Gain(bus.gain),
                        BusControl::Balance(bus.balance),
                        BusControl::Mute(bus.mute),
                    ];
                    if let Some(control) = controls.get(cursor.part - 1) {
                        cursor.part += 1;
                        return Some(StateRecord::Bus {
                            bus: bus.id,
                            control: *control,
                        });
                    }
                    while cursor.channel < bus.channels as usize {
                        let mask = bus.outputs.current()[cursor.channel];
                        while cursor.bit < crate::MAX_HOST_PORTS {
                            let bit = cursor.bit;
                            cursor.bit += 1;
                            if mask.contains(bit) {
                                return Some(StateRecord::Connection {
                                    port: EnginePort::Bus(BusPort {
                                        bus: bus.id,
                                        index: cursor.channel as u8,
                                    }),
                                    host: self.host_port_id(PortKind::AudioOut, bit),
                                });
                            }
                        }
                        cursor.bit = 0;
                        cursor.channel += 1;
                    }
                    cursor.index += 1;
                    cursor.part = 0;
                    cursor.channel = 0;
                    cursor.bit = 0;
                }
                5 => {
                    cursor.stage = 6;
                    return Some(StateRecord::End {
                        epoch: cursor.epoch,
                    });
                }
                _ => return None,
            }
        }
    }

    pub(crate) fn stream_resync(&mut self, out: &mut Outbox) {
        let Some(mut cursor) = self.resync else {
            return;
        };
        while out.available(Lane::Reliable) > 0 {
            match self.next_state(&mut cursor) {
                Some(record) => {
                    let _ = out.push(EngineMessage::State(record));
                }
                None => {
                    self.resync = (cursor.stage < 6).then_some(cursor);
                    return;
                }
            }
        }
        self.resync = Some(cursor);
    }
}
