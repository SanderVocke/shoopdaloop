use crate::wire::{Config, MediaLimits, DATAGRAM, HEADER, VERSION};
use rt_buffered::{AsyncOutput, BatchTag};
use rt_runtime::{buffer::MidiBuffer, Error, Frame};
use std::{
    io,
    net::UdpSocket,
    time::{Duration, Instant},
};

const MAGIC: [u8; 4] = [b'S', b'F', b'X', VERSION];
pub(crate) const POLL: Duration = Duration::from_micros(150);
pub(crate) const MAX_DATAGRAMS_PER_POLL: usize = 512;

pub(crate) struct Fragment<'a> {
    tag: BatchTag,
    pub(crate) first_sequence: u64,
    quantum: usize,
    fragment: usize,
    total: usize,
    data: &'a [u8],
}

impl<'a> Fragment<'a> {
    pub(crate) fn parse(datagram: &'a [u8], stream: u128, config: Config) -> Option<Self> {
        if datagram.len() < HEADER
            || datagram.len() > usize::from(config.datagram_bytes)
            || datagram[..4] != MAGIC
            || datagram[4..20] != stream.to_le_bytes()
        {
            return None;
        }
        let tag = BatchTag {
            generation: u64::from_le_bytes(datagram[20..28].try_into().ok()?),
            sequence: u64::from_le_bytes(datagram[28..36].try_into().ok()?),
            source: Frame(u64::from_le_bytes(datagram[36..44].try_into().ok()?)),
        };
        let quantum = u16::from_le_bytes(datagram[44..46].try_into().ok()?) as usize;
        let fragment = u16::from_le_bytes(datagram[46..48].try_into().ok()?) as usize;
        let total = u32::from_le_bytes(datagram[48..52].try_into().ok()?) as usize;
        let first_sequence = u64::from_le_bytes(datagram[52..60].try_into().ok()?);
        let offset = fragment * config.fragment_bytes();
        let data = &datagram[HEADER..];
        if first_sequence > tag.sequence
            || quantum >= usize::from(config.batch_quanta)
            || (total == 0 && fragment != 0)
            || (total > 0 && offset >= total)
            || data.len() != total.saturating_sub(offset).min(config.fragment_bytes())
        {
            return None;
        }
        Some(Self {
            tag,
            first_sequence,
            quantum,
            fragment,
            total,
            data,
        })
    }

    pub(crate) fn tag(&self) -> BatchTag {
        self.tag
    }
}

pub(crate) struct Encoder {
    config: Config,
    format: MediaLimits,
    payload: Vec<u8>,
    packet: [u8; DATAGRAM],
}

impl Encoder {
    pub(crate) fn new(config: Config, format: MediaLimits) -> Self {
        Self {
            config,
            format,
            payload: Vec::with_capacity(format.payload(config.quantum.into())),
            packet: [0; DATAGRAM],
        }
    }

    pub(crate) fn send(
        &mut self,
        socket: &UdpSocket,
        stream: u128,
        tag: BatchTag,
        first_sequence: u64,
        output: AsyncOutput<'_>,
    ) -> io::Result<u64> {
        let mut sent = 0;
        self.encode(stream, tag, first_sequence, output, |bytes| {
            match socket.send(bytes) {
                Ok(_) => {
                    sent += 1;
                    Ok(())
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
                Err(error) => Err(error),
            }
        })?;
        Ok(sent)
    }

    pub(crate) fn encode(
        &mut self,
        stream: u128,
        tag: BatchTag,
        first_sequence: u64,
        output: AsyncOutput<'_>,
        mut send: impl FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        let quantum = usize::from(self.config.quantum);
        if output.frames != quantum * usize::from(self.config.batch_quanta)
            || output.audio.len() != output.frames * usize::from(self.format.channels)
            || output.midi.len() != usize::from(self.format.midi)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.packet[..4].copy_from_slice(&MAGIC);
        self.packet[4..20].copy_from_slice(&stream.to_le_bytes());
        self.packet[20..28].copy_from_slice(&tag.generation.to_le_bytes());
        self.packet[28..36].copy_from_slice(&tag.sequence.to_le_bytes());
        self.packet[36..44].copy_from_slice(&tag.source.0.to_le_bytes());
        self.packet[52..60].copy_from_slice(&first_sequence.to_le_bytes());
        let midi = output.midi.first();
        let mut next_event = 0;
        for index in 0..self.config.batch_quanta {
            let start = usize::from(index) * quantum;
            let end = start + quantum;
            self.payload.clear();
            for channel in output.audio.chunks_exact(output.frames) {
                for sample in &channel[start..end] {
                    self.payload.extend_from_slice(&sample.to_le_bytes());
                }
            }
            if let Some(midi) = midi {
                let mut events = 0;
                let mut bytes = 0;
                while let Some((offset, data)) = midi.get(next_event) {
                    if offset as usize >= end {
                        break;
                    }
                    if (offset as usize) < start || data.is_empty() {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    events += 1;
                    bytes += data.len();
                    if events > usize::from(self.format.midi_events)
                        || bytes > self.format.midi_bytes as usize
                    {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    self.payload
                        .extend_from_slice(&((offset as usize - start) as u16).to_le_bytes());
                    self.payload
                        .extend_from_slice(&(data.len() as u16).to_le_bytes());
                    self.payload.extend_from_slice(data);
                    next_event += 1;
                }
            }
            self.packet[44..46].copy_from_slice(&index.to_le_bytes());
            self.packet[48..52].copy_from_slice(&(self.payload.len() as u32).to_le_bytes());
            let fragments = self
                .payload
                .len()
                .div_ceil(self.config.fragment_bytes())
                .max(1);
            for part in 0..fragments {
                let offset = part * self.config.fragment_bytes();
                let data = &self.payload
                    [offset..(offset + self.config.fragment_bytes()).min(self.payload.len())];
                self.packet[46..48].copy_from_slice(&(part as u16).to_le_bytes());
                self.packet[HEADER..HEADER + data.len()].copy_from_slice(data);
                for _ in 0..self.config.copies {
                    send(&self.packet[..HEADER + data.len()])?;
                }
            }
        }
        if midi.is_some_and(|midi| next_event != midi.len()) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
}

struct QuantumData {
    bytes: Vec<u8>,
    fragments: Vec<bool>,
    total: Option<usize>,
    received: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Receipt {
    Pending,
    Complete,
    Duplicate,
    Invalid,
}

pub(crate) struct Assembly {
    tag: Option<BatchTag>,
    quanta: Vec<QuantumData>,
    complete: usize,
    created: Option<Instant>,
    config: Config,
    format: MediaLimits,
}

impl Assembly {
    pub(crate) fn new(config: Config, format: MediaLimits) -> Self {
        let capacity = format.payload(config.quantum.into());
        Self {
            tag: None,
            quanta: (0..config.batch_quanta)
                .map(|_| QuantumData {
                    bytes: vec![0; capacity],
                    fragments: vec![false; capacity.div_ceil(config.fragment_bytes()).max(1)],
                    total: None,
                    received: 0,
                })
                .collect(),
            complete: 0,
            created: None,
            config,
            format,
        }
    }

    pub(crate) fn reset(&mut self) {
        if self.tag.take().is_some() {
            for quantum in &mut self.quanta {
                quantum.fragments.fill(false);
                quantum.total = None;
                quantum.received = 0;
            }
        }
        self.created = None;
        self.complete = 0;
    }

    pub(crate) fn tag(&self) -> Option<BatchTag> {
        self.tag
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.complete == self.quanta.len()
    }

    pub(crate) fn started(&self) -> Option<Instant> {
        self.created
    }

    pub(crate) fn push(&mut self, fragment: Fragment<'_>, now: Instant) -> Receipt {
        let quantum = &mut self.quanta[fragment.quantum];
        let audio_bytes = usize::from(self.config.quantum) * usize::from(self.format.channels) * 4;
        if fragment.total < audio_bytes
            || fragment.total > quantum.bytes.len()
            || (!self.format.midi && fragment.total != audio_bytes)
            || self.tag.is_some_and(|tag| tag != fragment.tag)
            || quantum.total.is_some_and(|total| total != fragment.total)
        {
            return Receipt::Invalid;
        }
        if quantum.fragments[fragment.fragment] {
            return Receipt::Duplicate;
        }
        self.tag = Some(fragment.tag);
        self.created.get_or_insert(now);
        let offset = fragment.fragment * self.config.fragment_bytes();
        quantum.bytes[offset..offset + fragment.data.len()].copy_from_slice(fragment.data);
        quantum.total = Some(fragment.total);
        quantum.fragments[fragment.fragment] = true;
        quantum.received += 1;
        if quantum.received == fragment.total.div_ceil(self.config.fragment_bytes()).max(1) {
            self.complete += 1;
        }
        if self.is_complete() {
            Receipt::Complete
        } else {
            Receipt::Pending
        }
    }

    pub(crate) fn decode(&self, audio: &mut [f32], midi: &mut [MidiBuffer]) -> Result<(), Error> {
        if !self.is_complete() {
            return Err(Error::Incomplete);
        }
        let quantum = usize::from(self.config.quantum);
        let frames = quantum * self.quanta.len();
        if audio.len() != frames * usize::from(self.format.channels)
            || midi.len() != usize::from(self.format.midi)
        {
            return Err(Error::InvalidShape);
        }
        let mut midi = midi.first_mut();
        if let Some(midi) = &mut midi {
            midi.clear();
        }
        for (index, source) in self.quanta.iter().enumerate() {
            let mut data = &source.bytes[..source.total.ok_or(Error::Incomplete)?];
            for destination in audio.chunks_exact_mut(frames) {
                let (samples, rest) = data.split_at(quantum * 4);
                data = rest;
                for (target, sample) in destination[index * quantum..][..quantum]
                    .iter_mut()
                    .zip(samples.chunks_exact(4))
                {
                    let value =
                        f32::from_le_bytes(sample.try_into().map_err(|_| Error::InvalidShape)?);
                    if !value.is_finite() {
                        return Err(Error::InvalidParameter);
                    }
                    *target = value;
                }
            }
            if let Some(midi) = &mut midi {
                let mut events = 0;
                let mut bytes = 0;
                while !data.is_empty() {
                    let (header, rest) = data.split_at_checked(4).ok_or(Error::InvalidEvent)?;
                    let offset = u16::from_le_bytes([header[0], header[1]]) as usize;
                    let length = u16::from_le_bytes([header[2], header[3]]) as usize;
                    let (packet, rest) =
                        rest.split_at_checked(length).ok_or(Error::InvalidEvent)?;
                    data = rest;
                    events += 1;
                    bytes += length;
                    if offset >= quantum
                        || length == 0
                        || events > usize::from(self.format.midi_events)
                        || bytes > self.format.midi_bytes as usize
                    {
                        return Err(Error::InvalidEvent);
                    }
                    midi.push((index * quantum + offset) as u32, frames, packet)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn wide_quantum_is_fragmented_reordered_deduplicated_and_decoded_as_one_unit() {
        let format = MediaLimits {
            channels: 16,
            midi: true,
            midi_events: 4,
            midi_bytes: 16,
        };
        let config = Config {
            sample_rate: 48_000,
            quantum: 32,
            batch_quanta: 2,
            slack_quanta: 4,
            input: format,
            output: format,
            datagram_bytes: DATAGRAM as u16,
            copies: 2,
        };
        let tag = BatchTag {
            generation: 1,
            sequence: 5,
            source: Frame(320),
        };
        let audio: Vec<_> = (0..1024).map(|sample| sample as f32 / 1024.).collect();
        let mut midi = MidiBuffer::new(8, 32).unwrap();
        midi.push(3, 64, &[0x90, 60, 127]).unwrap();
        midi.push(55, 64, &[0x80, 60, 0]).unwrap();
        let mut packets = Vec::new();
        Encoder::new(config, format)
            .encode(
                42,
                tag,
                1,
                AsyncOutput {
                    frames: 64,
                    audio: &audio,
                    midi: &[midi],
                },
                |bytes| {
                    packets.push(bytes.to_vec());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(packets.len(), 8);
        assert!(packets.chunks_exact(2).all(|copies| copies[0] == copies[1]));
        let mut assembly = Assembly::new(config, format);
        let now = Instant::now();
        let mut result = Receipt::Pending;
        for (index, packet) in packets.iter().rev().enumerate() {
            if index == 6 {
                assert_eq!(result, Receipt::Duplicate);
                assert!(!assembly.is_complete());
            }
            let fragment = Fragment::parse(packet, 42, config).unwrap();
            result = assembly.push(fragment, now);
        }
        assert_eq!(result, Receipt::Duplicate);
        let mut returned = vec![0.; 1024];
        let mut events = [MidiBuffer::new(8, 32).unwrap()];
        assembly.decode(&mut returned, &mut events).unwrap();
        assert_eq!(returned, audio);
        assert_eq!(events[0].get(0), Some((3, &[0x90, 60, 127][..])));
        assert_eq!(events[0].get(1), Some((55, &[0x80, 60, 0][..])));
        assert!(Fragment::parse(&packets[0][..packets[0].len() - 1], 42, config).is_none());
        assert!(Fragment::parse(&packets[0], 43, config).is_none());
    }
}
