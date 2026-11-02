use crate::{ExactMidi, ExactMidiEvent, SessionError};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{fmt, io::Read};

pub(crate) struct LimitedWriter<W> {
    inner: W,
    remaining: u64,
}
impl<W> LimitedWriter<W> {
    pub fn new(inner: W, remaining: u64) -> Self {
        Self { inner, remaining }
    }
}
impl<W: std::io::Write> std::io::Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("output byte budget exceeded"));
        }
        let n = self.inner.write(bytes)?;
        self.remaining -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub(crate) struct OutputCursor {
    inner: std::io::Cursor<Vec<u8>>,
    limit: u64,
}
impl OutputCursor {
    pub fn new(limit: u64) -> Self {
        Self {
            inner: std::io::Cursor::new(Vec::new()),
            limit,
        }
    }
    pub fn into_inner(self) -> Vec<u8> {
        self.inner.into_inner()
    }
}
impl std::io::Write for OutputCursor {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .inner
            .position()
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= self.limit)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| std::io::Error::other("output file budget exceeded"))?;
        let buffer = self.inner.get_mut();
        if end > buffer.capacity() {
            let maximum = usize::try_from(self.limit).unwrap_or(usize::MAX);
            let capacity = end.max(buffer.capacity().saturating_mul(2)).min(maximum);
            buffer
                .try_reserve_exact(capacity - buffer.len())
                .map_err(std::io::Error::other)?;
        }
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Seek for OutputCursor {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CodecLimits {
    pub file_bytes: u64,
    pub metadata_bytes: u64,
    pub expanded_bytes: u64,
    pub media_bytes: u64,
    pub entries: usize,
    pub midi_events: u64,
    pub packet_bytes: u64,
    pub initial_packets: usize,
    pub audio_channels: usize,
}

pub(crate) struct Budget {
    pub limits: CodecLimits,
    bytes: u64,
    events: u64,
}
impl Budget {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            limits,
            bytes: 0,
            events: 0,
        }
    }
    pub fn charge(&mut self, bytes: u64) -> Result<(), SessionError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.media_bytes)
            .ok_or_else(|| SessionError::ResourceLimit("decoded media budget exceeded".into()))?;
        Ok(())
    }
    pub fn event(&mut self) -> Result<(), SessionError> {
        self.events += 1;
        if self.events > self.limits.midi_events {
            return Err(SessionError::ResourceLimit(
                "MIDI event budget exceeded".into(),
            ));
        }
        self.charge(12)
    }
    pub fn midi(&mut self, midi: &ExactMidi) -> Result<(), SessionError> {
        if midi.start_state.len() > self.limits.initial_packets {
            return Err(SessionError::ResourceLimit(
                "initial MIDI packet budget exceeded".into(),
            ));
        }
        for packet in &midi.start_state {
            self.packet(packet)?;
            self.charge(4)?;
        }
        for event in &midi.events {
            self.event()?;
            self.packet(&event.data)?;
        }
        Ok(())
    }
    fn packet(&mut self, packet: &[u8]) -> Result<(), SessionError> {
        if packet.is_empty() || packet.len() as u64 > self.limits.packet_bytes {
            return Err(SessionError::ResourceLimit(
                "MIDI packet budget exceeded".into(),
            ));
        }
        self.charge(packet.len() as u64)
    }
}

pub(crate) fn read_midi(reader: impl Read, budget: &mut Budget) -> Result<ExactMidi, SessionError> {
    let mut decoder = serde_json::Deserializer::from_reader(std::io::BufReader::new(
        JsonReader::new(reader, budget.limits.metadata_bytes),
    ));
    let midi = MidiSeed(budget)
        .deserialize(&mut decoder)
        .map_err(|e| SessionError::Manifest(e.to_string()))?;
    decoder
        .end()
        .map_err(|e| SessionError::Manifest(e.to_string()))?;
    Ok(midi)
}

pub(crate) struct JsonReader<R> {
    inner: R,
    string_limit: u64,
    string_bytes: u64,
    in_string: bool,
    escaped: bool,
}
impl<R> JsonReader<R> {
    pub fn new(inner: R, string_limit: u64) -> Self {
        Self {
            inner,
            string_limit,
            string_bytes: 0,
            in_string: false,
            escaped: false,
        }
    }
}
impl<R: Read> Read for JsonReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(out)?;
        for &byte in &out[..n] {
            if self.in_string {
                if byte == b'"' && !self.escaped {
                    self.in_string = false;
                } else {
                    self.string_bytes += 1;
                    if self.string_bytes > self.string_limit {
                        return Err(std::io::Error::other("MIDI JSON string budget exceeded"));
                    }
                }
                self.escaped = byte == b'\\' && !self.escaped;
            } else if byte == b'"' {
                self.in_string = true;
                self.string_bytes = 0;
                self.escaped = false;
            }
        }
        Ok(n)
    }
}

pub(crate) struct MidiSeed<'a>(pub &'a mut Budget);
#[derive(serde::Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum MidiField {
    SampleRate,
    LengthFrames,
    StartState,
    Events,
    Frame,
    Order,
    Data,
    #[serde(other)]
    Other,
}
impl<'de> DeserializeSeed<'de> for MidiSeed<'_> {
    type Value = ExactMidi;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for MidiSeed<'_> {
    type Value = ExactMidi;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("exact MIDI")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let (mut rate, mut frames, mut initial, mut events) = (None, None, None, None);
        while let Some(key) = map.next_key::<MidiField>()? {
            match key {
                MidiField::SampleRate if rate.is_none() => rate = Some(map.next_value()?),
                MidiField::LengthFrames if frames.is_none() => frames = Some(map.next_value()?),
                MidiField::StartState if initial.is_none() => {
                    initial = Some(map.next_value_seed(InitialSeed(self.0))?)
                }
                MidiField::Events if events.is_none() => {
                    events = Some(map.next_value_seed(EventsSeed(self.0))?)
                }
                MidiField::SampleRate
                | MidiField::LengthFrames
                | MidiField::StartState
                | MidiField::Events => return Err(de::Error::custom("duplicate MIDI field")),
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(ExactMidi {
            sample_rate: rate.ok_or_else(|| de::Error::missing_field("sample_rate"))?,
            length_frames: frames.ok_or_else(|| de::Error::missing_field("length_frames"))?,
            start_state: initial.ok_or_else(|| de::Error::missing_field("start_state"))?,
            events: events.ok_or_else(|| de::Error::missing_field("events"))?,
        })
    }
}

struct PacketSeed<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for PacketSeed<'_> {
    type Value = Vec<u8>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for PacketSeed<'_> {
    type Value = Vec<u8>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("MIDI packet")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut bytes = Vec::new();
        while let Some(byte) = seq.next_element::<u8>()? {
            if bytes.len() as u64 >= self.0.limits.packet_bytes {
                return Err(de::Error::custom("MIDI packet budget exceeded"));
            }
            self.0.charge(1).map_err(de::Error::custom)?;
            bytes.push(byte);
        }
        if bytes.is_empty() {
            return Err(de::Error::custom("empty MIDI packet"));
        }
        Ok(bytes)
    }
}

struct InitialSeed<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for InitialSeed<'_> {
    type Value = Vec<Vec<u8>>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for InitialSeed<'_> {
    type Value = Vec<Vec<u8>>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("initial MIDI packets")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut packets = Vec::new();
        while let Some(packet) = seq.next_element_seed(PacketSeed(self.0))? {
            if packets.len() >= self.0.limits.initial_packets {
                return Err(de::Error::custom("initial MIDI packet budget exceeded"));
            }
            self.0.charge(4).map_err(de::Error::custom)?;
            packets.push(packet);
        }
        Ok(packets)
    }
}

struct EventsSeed<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for EventsSeed<'_> {
    type Value = Vec<ExactMidiEvent>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for EventsSeed<'_> {
    type Value = Vec<ExactMidiEvent>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("MIDI events")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut events = Vec::new();
        while let Some(event) = seq.next_element_seed(EventSeed(self.0))? {
            events.push(event);
        }
        Ok(events)
    }
}
struct EventSeed<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for EventSeed<'_> {
    type Value = ExactMidiEvent;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.event().map_err(de::Error::custom)?;
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for EventSeed<'_> {
    type Value = ExactMidiEvent;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("MIDI event")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let (mut frame, mut order, mut data) = (None, None, None);
        while let Some(key) = map.next_key::<MidiField>()? {
            match key {
                MidiField::Frame if frame.is_none() => frame = Some(map.next_value()?),
                MidiField::Order if order.is_none() => order = Some(map.next_value()?),
                MidiField::Data if data.is_none() => {
                    data = Some(map.next_value_seed(PacketSeed(self.0))?)
                }
                MidiField::Frame | MidiField::Order | MidiField::Data => {
                    return Err(de::Error::custom("duplicate MIDI event field"))
                }
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(ExactMidiEvent {
            frame: frame.ok_or_else(|| de::Error::missing_field("frame"))?,
            order: order.ok_or_else(|| de::Error::missing_field("order"))?,
            data: data.ok_or_else(|| de::Error::missing_field("data"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    #[shoop_wasm_test_support::shoop_test]
    fn output_capacity_never_grows_past_its_byte_budget() {
        let mut output = OutputCursor::new(70);
        output.write_all(&[1; 45]).unwrap();
        output.write_all(&[2; 20]).unwrap();
        assert!(output.inner.get_ref().capacity() <= 70);
        assert!(output.write_all(&[3; 6]).is_err());
        assert_eq!(output.inner.get_ref().len(), 65);
        output.seek(SeekFrom::Start(u64::MAX)).unwrap();
        assert!(output.write_all(&[4]).is_err());
        assert_eq!(output.inner.get_ref().len(), 65);
    }
}
