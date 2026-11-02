use crate::{wire, Config, Error};
use alloc::{vec, vec::Vec};
use shoop_dsp::{Block, MidiBuffer, MidiEvent, Processor, Transport};

#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Stats {
    pub received: u64,
    pub duplicates: u64,
    pub stale: u64,
    pub invalid: u64,
    pub future: u64,
    pub concealed_packets: u64,
    pub blocks: u64,
    pub sent: u64,
    pub send_dropped: u64,
    pub midi_resets: u64,
    pub process_ns: u64,
    pub diagnostics_truncated: bool,
    pub retired: bool,
    pub fault: Option<Error>,
}
struct Slot {
    sequence: Option<u64>,
    audio: Vec<f32>,
    midi: MidiBuffer,
    transport: Transport,
}

pub struct Stream {
    config: Config,
    epoch: u64,
    slots: Vec<Slot>,
    next: u64,
    highest: Option<u64>,
    future_streak: usize,
    gap_since: Option<u64>,
    stats: Stats,
    concealment: Vec<(u64, u64)>,
    input: Vec<f32>,
    output: Vec<f32>,
    midi_in: MidiBuffer,
    midi_out: MidiBuffer,
    packet_audio: Vec<f32>,
    packet_midi: MidiBuffer,
    encoded: Vec<u8>,
}
impl Stream {
    pub fn prepare(config: Config, epoch: u64) -> Result<Self, Error> {
        let config = config.validate()?;
        if epoch == 0 {
            return Err(Error::Config);
        }
        let packet_samples = config.packet_frames as usize * config.channels as usize;
        let samples = config.dsp().frames * config.channels as usize;
        let slots = (0..config.multiplier as usize + config.reorder_packets as usize + 1)
            .map(|_| Slot {
                sequence: None,
                audio: vec![0.; packet_samples],
                midi: MidiBuffer::new(config.midi_per_packet as usize),
                transport: Transport::default(),
            })
            .collect();
        let midi_capacity = config.multiplier as usize * config.midi_per_packet as usize + 32;
        Ok(Self {
            config,
            epoch,
            slots,
            next: 0,
            highest: None,
            future_streak: 0,
            gap_since: None,
            stats: Stats::default(),
            concealment: Vec::with_capacity(1024),
            input: vec![0.; samples],
            output: vec![0.; samples],
            midi_in: MidiBuffer::new(midi_capacity),
            midi_out: MidiBuffer::new(midi_capacity),
            packet_audio: vec![0.; packet_samples],
            packet_midi: MidiBuffer::new(config.midi_per_packet as usize),
            encoded: vec![0; config.max_packet_bytes()],
        })
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn concealment(&self) -> &[(u64, u64)] {
        &self.concealment
    }
    pub fn next_sequence(&self) -> u64 {
        self.next
    }
    pub fn retire(&mut self) {
        self.retire_with(Error::Retired);
    }
    pub fn retire_with(&mut self, error: Error) {
        self.stats.fault.get_or_insert(error);
        self.stats.retired = true;
    }
    pub fn accept(&mut self, bytes: &[u8], now: u64) -> Result<(), Error> {
        if self.stats.retired {
            return Err(Error::Retired);
        }
        let packet = match wire::decode(bytes, self.config) {
            Ok(p) => p,
            Err(e) => {
                self.stats.invalid += 1;
                return Err(e);
            }
        };
        let h = packet.header;
        if h.kind != wire::INPUT {
            self.stats.invalid += 1;
            return Err(Error::Packet);
        }
        if h.epoch != self.epoch || h.sequence < self.next {
            self.stats.stale += 1;
            return Err(Error::Stale);
        }
        if h.sequence >= self.next.saturating_add(self.slots.len() as u64) {
            self.stats.future += 1;
            self.future_streak += 1;
            if self.future_streak > self.slots.len() {
                self.retire_with(Error::Future);
            }
            return Err(Error::Future);
        }
        let index = crate::slot_index(h.sequence, self.slots.len());
        let slot = &mut self.slots[index];
        if slot.sequence == Some(h.sequence) {
            self.stats.duplicates += 1;
            return Err(Error::Duplicate);
        }
        slot.sequence = Some(h.sequence);
        slot.transport = h.transport;
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
        self.highest = Some(self.highest.unwrap_or(0).max(h.sequence));
        self.gap_since.get_or_insert(now);
        self.future_streak = 0;
        self.stats.received += 1;
        Ok(())
    }
    pub fn process_one(
        &mut self,
        now: u64,
        dsp: &mut dyn Processor,
        mut clock: impl FnMut() -> u64,
        mut emit: impl FnMut(&[u8]) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        if self.stats.retired {
            return Err(Error::Retired);
        }
        if self.highest.is_none_or(|highest| highest < self.next) {
            return Ok(false);
        }
        let p = self.config.packet_frames as usize;
        let k = self.config.multiplier as usize;
        let cframes = p * k;
        let mut missing = 0u64;
        for j in 0..k {
            if self.slots[crate::slot_index(self.next + j as u64, self.slots.len())].sequence
                != Some(self.next + j as u64)
            {
                missing |= 1 << j;
            }
        }
        if missing != 0 {
            let since = *self.gap_since.get_or_insert(now);
            let wait =
                (k as u64 - 1) * self.config.packet_ns() + self.config.gap_timeout_us as u64 * 1000;
            if now.saturating_sub(since) < wait {
                return Ok(false);
            }
        }
        self.input.fill(0.);
        self.midi_in.clear();
        let mut transport = None;
        if missing != 0 {
            self.stats.concealed_packets += missing.count_ones() as u64;
            if self.concealment.len() < self.concealment.capacity() {
                self.concealment.push((self.next, missing));
            } else {
                self.stats.diagnostics_truncated = true;
            }
            for channel in 0..16 {
                for control in [120, 123] {
                    self.midi_in
                        .push(
                            MidiEvent::new(0, 0, &[0xb0 | channel, control, 0])
                                .map_err(|_| Error::Dsp)?,
                        )
                        .map_err(|_| Error::Dsp)?;
                }
            }
        }
        for j in 0..k {
            let length = self.slots.len();
            let slot = &mut self.slots[crate::slot_index(self.next + j as u64, length)];
            if slot.sequence != Some(self.next + j as u64) {
                continue;
            }
            if transport.is_some_and(|t| t != slot.transport) {
                self.retire_with(Error::Packet);
                return Err(Error::Packet);
            }
            transport = Some(slot.transport);
            for c in 0..self.config.channels as usize {
                self.input[c * cframes + j * p..c * cframes + (j + 1) * p]
                    .copy_from_slice(&slot.audio[c * p..(c + 1) * p]);
            }
            for event in slot.midi.as_slice() {
                let mut event = *event;
                event.frame += (j * p) as u32;
                self.midi_in.push(event).map_err(|_| Error::Capacity)?;
            }
            slot.sequence = None;
        }
        let transport = transport.unwrap_or_default();
        let started = clock();
        if dsp
            .process(Block {
                frame: self.config.start_frame + self.next * p as u64,
                frames: cframes,
                input: &self.input,
                output: &mut self.output,
                midi_input: self.midi_in.as_slice(),
                midi_output: &mut self.midi_out,
                transport,
            })
            .is_err()
        {
            self.retire_with(Error::Dsp);
            return Err(Error::Dsp);
        }
        let elapsed = clock().saturating_sub(started);
        self.stats.blocks += 1;
        self.stats.process_ns += elapsed;
        for j in 0..k {
            let mut flags = if missing != 0 {
                wire::CONCEALED | wire::MIDI_RESET
            } else {
                0
            };
            self.packet_midi.clear();
            for event in self.midi_out.as_slice() {
                if event.frame as usize >= cframes || event.port != 0 || event.bytes().is_err() {
                    flags |= wire::MIDI_RESET;
                    continue;
                }
                if event.frame as usize / p == j {
                    let mut event = *event;
                    event.frame %= p as u32;
                    if self.packet_midi.push(event).is_err() {
                        flags |= wire::MIDI_RESET;
                    }
                }
            }
            if flags & wire::MIDI_RESET != 0 {
                self.packet_midi.clear();
                self.stats.midi_resets += 1;
            }
            for c in 0..self.config.channels as usize {
                self.packet_audio[c * p..(c + 1) * p]
                    .copy_from_slice(&self.output[c * cframes + j * p..c * cframes + (j + 1) * p]);
            }
            let sequence = self.next + j as u64;
            let header = wire::Header {
                kind: wire::OUTPUT,
                flags,
                epoch: self.epoch,
                sequence,
                frame: self.config.start_frame + sequence * p as u64,
                transport,
                process_ns: elapsed,
                missing_mask: missing,
            };
            let len = wire::encode(
                header,
                self.config,
                &self.packet_audio,
                self.packet_midi.as_slice(),
                &mut self.encoded,
            )?;
            if emit(&self.encoded[..len]).is_ok() {
                self.stats.sent += 1;
            } else {
                self.stats.send_dropped += 1;
            }
        }
        self.next = self.next.checked_add(k as u64).ok_or(Error::Capacity)?;
        self.gap_since = None;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_dsp::DelayCopy;

    #[shoop_wasm_test_support::shoop_test]
    fn reordered_slots_survive_the_32_bit_sequence_boundary() {
        let config = Config {
            packet_frames: 1,
            multiplier: 4,
            allowance: 4,
            channels: 1,
            midi_per_packet: 0,
            reorder_packets: 4,
            ..Config::default()
        };
        let base = (1u64 << 32) - 4;
        let mut stream = Stream::prepare(config, 1).unwrap();
        stream.next = base;
        let mut dsp = DelayCopy::prepare(config.dsp(), 0).unwrap();
        let mut raw = [0; 68];
        for offset in (0..9).rev() {
            let n = wire::encode(
                wire::Header::input(1, base + offset, config),
                config,
                &[offset as f32 + 1.],
                &[],
                &mut raw,
            )
            .unwrap();
            stream.accept(&raw[..n], 0).unwrap();
        }
        let mut received = 0;
        for _ in 0..2 {
            assert!(stream
                .process_one(
                    0,
                    &mut dsp,
                    || 0,
                    |raw| {
                        let packet = wire::decode(raw, config)?;
                        assert_eq!(packet.header.sequence, base + received);
                        assert_eq!(packet.sample(0, 0), received as f32 + 1.);
                        received += 1;
                        Ok(())
                    }
                )
                .unwrap());
        }
        assert_eq!(received, 8);
        assert_eq!(stream.stats().concealed_packets, 0);
    }
}
