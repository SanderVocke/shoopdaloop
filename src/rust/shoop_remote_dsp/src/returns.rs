use crate::{wire, Config, Error};
use alloc::{vec, vec::Vec};
use shoop_dsp::MidiBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Processed,
    Concealed,
    Silence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub frame: u64,
    pub outcome: Outcome,
    pub midi_reset: bool,
}
struct Slot {
    sequence: Option<u64>,
    audio: Vec<f32>,
    midi: MidiBuffer,
    flags: u8,
}
pub struct Returns {
    config: Config,
    epoch: u64,
    next: u64,
    slots: Vec<Slot>,
}
impl Returns {
    pub fn prepare(config: Config, epoch: u64) -> Result<Self, Error> {
        let config = config.validate()?;
        if epoch == 0 {
            return Err(Error::Config);
        }
        let samples = config.packet_frames as usize * config.channels as usize;
        let capacity = config.allowance as usize
            + config.multiplier as usize
            + config.reorder_packets as usize
            + 1;
        Ok(Self {
            config,
            epoch,
            next: 0,
            slots: (0..capacity)
                .map(|_| Slot {
                    sequence: None,
                    audio: vec![0.; samples],
                    midi: MidiBuffer::new(config.midi_per_packet as usize),
                    flags: 0,
                })
                .collect(),
        })
    }
    pub fn accept(&mut self, raw: &[u8], now: u64) -> Result<(), Error> {
        let packet = wire::decode(raw, self.config)?;
        let h = packet.header;
        if h.kind != wire::OUTPUT {
            return Err(Error::Packet);
        }
        if h.epoch != self.epoch || h.sequence < self.next {
            return Err(Error::Stale);
        }
        if h.sequence >= self.next.saturating_add(self.slots.len() as u64) {
            return Err(Error::Future);
        }
        if now > self.config.deadline_ns(h.sequence)? {
            return Err(Error::Deadline);
        }
        let length = self.slots.len();
        let slot = &mut self.slots[h.sequence as usize % length];
        if slot.sequence == Some(h.sequence) {
            return Err(Error::Duplicate);
        }
        slot.sequence = Some(h.sequence);
        slot.flags = h.flags;
        slot.midi.clear();
        for c in 0..self.config.channels as usize {
            for i in 0..self.config.packet_frames as usize {
                slot.audio[c * self.config.packet_frames as usize + i] = packet.sample(c, i);
            }
        }
        for i in 0..packet.midi_len() {
            slot.midi
                .push(packet.midi(i))
                .map_err(|_| Error::Capacity)?;
        }
        Ok(())
    }
    pub fn consume(
        &mut self,
        sequence: u64,
        now: u64,
        audio: &mut [f32],
        midi: &mut MidiBuffer,
    ) -> Result<Delivery, Error> {
        if sequence != self.next {
            return Err(Error::Stale);
        }
        if now < self.config.deadline_ns(sequence)? {
            return Err(Error::Deadline);
        }
        if audio.len() != self.config.packet_frames as usize * self.config.channels as usize {
            return Err(Error::Config);
        }
        let length = self.slots.len();
        let slot = &mut self.slots[sequence as usize % length];
        midi.clear();
        let outcome;
        let midi_reset;
        if slot.sequence == Some(sequence) {
            audio.copy_from_slice(&slot.audio);
            for event in slot.midi.as_slice() {
                midi.push(*event).map_err(|_| Error::Capacity)?;
            }
            outcome = if slot.flags & wire::CONCEALED != 0 {
                Outcome::Concealed
            } else {
                Outcome::Processed
            };
            midi_reset = slot.flags & wire::MIDI_RESET != 0;
            slot.sequence = None;
        } else {
            audio.fill(0.);
            outcome = Outcome::Silence;
            midi_reset = true;
        }
        self.next = self.next.checked_add(1).ok_or(Error::Capacity)?;
        Ok(Delivery {
            frame: self.config.start_frame + sequence * self.config.packet_frames as u64,
            outcome,
            midi_reset,
        })
    }
}
