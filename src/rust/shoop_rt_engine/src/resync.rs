//! The full-state record stream, emitted as reliable outbox space allows.

use crate::endpoint::ControlEndpoint;
use shoop_rt_protocol::carrier::Outbox;
use shoop_rt_protocol::{
    BusControl, BusId, EngineMessage, HostPortId, Lane, PortKind, StateRecord, StripPort,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Resync {
    stage: u8,
    index: usize,
    part: usize,
    channel: usize,
    bit: usize,
    epoch: u64,
}

const PORT_KINDS: [PortKind; 4] = [
    PortKind::AudioIn,
    PortKind::AudioOut,
    PortKind::MidiIn,
    PortKind::MidiOut,
];

impl ControlEndpoint {
    fn host_port_id(&self, kind: PortKind, index: usize) -> HostPortId {
        let [audio_in, audio_out, midi_in, _] = self.settings.host;
        let first = match kind {
            PortKind::AudioIn => 0,
            PortKind::AudioOut => audio_in,
            PortKind::MidiIn => audio_in + audio_out,
            PortKind::MidiOut => audio_in + audio_out + midi_in,
        };
        HostPortId((first + index) as u32)
    }

    /// The next connection of the current strip at or after the cursor.
    fn next_connection(&self, cursor: &mut Resync) -> Option<StateRecord> {
        let strip = self.model.strips[cursor.index]?;
        while cursor.part < PORT_KINDS.len() {
            let kind = PORT_KINDS[cursor.part];
            let (channels, masks): (usize, &[u64]) = match kind {
                PortKind::AudioIn => (strip.shape.audio_inputs as usize, strip.inputs.current()),
                PortKind::AudioOut => (strip.shape.audio_outputs as usize, strip.outputs.current()),
                PortKind::MidiIn => (
                    usize::from(strip.shape.midi_input),
                    std::slice::from_ref(&strip.midi_inputs),
                ),
                PortKind::MidiOut => (
                    usize::from(strip.shape.midi_output),
                    std::slice::from_ref(&strip.midi_outputs),
                ),
            };
            while cursor.channel < channels {
                let mask = masks[cursor.channel];
                while cursor.bit < 64 {
                    let bit = cursor.bit;
                    cursor.bit += 1;
                    if mask & (1 << bit) != 0 {
                        return Some(StateRecord::Connection {
                            port: StripPort {
                                strip: strip.id,
                                kind,
                                index: cursor.channel as u8,
                            },
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
                    cursor.stage = 1;
                    cursor.epoch = self.model.revision;
                    return Some(StateRecord::Begin {
                        epoch: cursor.epoch,
                    });
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
                            cursor.part += 1;
                            if strip.outputs.master[strip.outputs.active] {
                                return Some(StateRecord::Route {
                                    strip: strip.id,
                                    bus: BusId::Master,
                                });
                            }
                        }
                        _ => {
                            let mut connections = Resync {
                                index: cursor.index,
                                part: cursor.part - controls.len() - 2,
                                ..*cursor
                            };
                            let record = self.next_connection(&mut connections);
                            cursor.part = connections.part + controls.len() + 2;
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
                    cursor.index += 1;
                    if let Some(entry) = slot {
                        return Some(StateRecord::Loop {
                            loop_id: entry.id,
                            strip: entry.strip,
                            sync: entry.sync,
                            gain: entry.gain,
                            balance: entry.balance,
                        });
                    }
                }
                4 => {
                    let master = self.model.master;
                    let controls = [
                        BusControl::Gain(master.gain),
                        BusControl::Balance(master.balance),
                        BusControl::Mute(master.mute),
                    ];
                    if let Some(control) = controls.get(cursor.index) {
                        cursor.index += 1;
                        return Some(StateRecord::Bus {
                            bus: BusId::Master,
                            control: *control,
                        });
                    }
                    cursor.stage = 5;
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
                    self.resync = None;
                    return;
                }
            }
        }
        self.resync = Some(cursor);
    }
}
