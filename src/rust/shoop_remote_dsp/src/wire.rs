use crate::{Config, Error};
use shoop_dsp::{MidiEvent, Transport};

pub const VERSION: u16 = 1;
pub const HEADER_BYTES: usize = 64;
pub const MIDI_BYTES: usize = 72;
pub const INPUT: u8 = 1;
pub const OUTPUT: u8 = 2;
pub const CONCEALED: u8 = 1;
pub const PLAYING: u8 = 2;
pub const MIDI_RESET: u8 = 4;

#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub kind: u8,
    pub flags: u8,
    pub epoch: u64,
    pub sequence: u64,
    pub frame: u64,
    pub transport: Transport,
    pub process_ns: u64,
    pub missing_mask: u64,
}
impl Header {
    pub fn input(epoch: u64, sequence: u64, config: Config) -> Self {
        Self {
            kind: INPUT,
            flags: 0,
            epoch,
            sequence,
            frame: sequence * config.packet_frames as u64,
            transport: Transport::default(),
            process_ns: 0,
            missing_mask: 0,
        }
    }
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

pub struct Packet<'a> {
    pub header: Header,
    audio: &'a [u8],
    midi: &'a [u8],
    config: Config,
}
impl Packet<'_> {
    pub fn sample(&self, channel: usize, frame: usize) -> f32 {
        let start = (channel * self.config.packet_frames as usize + frame) * 4;
        f32::from_le_bytes(self.audio[start..start + 4].try_into().unwrap())
    }
    pub fn midi_len(&self) -> usize {
        self.midi.len() / MIDI_BYTES
    }
    pub fn midi(&self, index: usize) -> MidiEvent {
        let raw = &self.midi[index * MIDI_BYTES..(index + 1) * MIDI_BYTES];
        MidiEvent {
            frame: u32::from_le_bytes(raw[..4].try_into().unwrap()),
            port: u16_at(raw, 4),
            len: u16_at(raw, 6),
            data: raw[8..72].try_into().unwrap(),
        }
    }
}

pub fn decode(raw: &[u8], config: Config) -> Result<Packet<'_>, Error> {
    if raw.len() < HEADER_BYTES
        || &raw[..4] != b"SDSP"
        || u16_at(raw, 4) != VERSION
        || ![INPUT, OUTPUT].contains(&raw[6])
        || raw[7] & !7 != 0
        || u16_at(raw, 32) != config.packet_frames
        || u16_at(raw, 34) != config.channels
        || u16_at(raw, 38) != 0
    {
        return Err(Error::Packet);
    }
    let events = u16_at(raw, 36) as usize;
    let samples = config.packet_frames as usize * config.channels as usize;
    if events > config.midi_per_packet as usize
        || raw.len() != HEADER_BYTES + samples * 4 + events * MIDI_BYTES
    {
        return Err(Error::Packet);
    }
    let sequence = u64_at(raw, 16);
    if sequence.checked_mul(config.packet_frames as u64) != Some(u64_at(raw, 24)) {
        return Err(Error::Packet);
    }
    let bpm = f64::from_bits(u64_at(raw, 40));
    if !bpm.is_finite() || (u64_at(raw, 40) != 0 && bpm <= 0.0) {
        return Err(Error::Packet);
    }
    let missing_mask = u64_at(raw, 56);
    if config.multiplier < 64 && missing_mask >> config.multiplier != 0 {
        return Err(Error::Packet);
    }
    if raw[6] == INPUT
        && (raw[7] & (CONCEALED | MIDI_RESET) != 0 || missing_mask != 0 || u64_at(raw, 48) != 0)
    {
        return Err(Error::Packet);
    }
    if raw[6] == OUTPUT && ((raw[7] & CONCEALED != 0) != (missing_mask != 0)) {
        return Err(Error::Packet);
    }
    let packet = Packet {
        header: Header {
            kind: raw[6],
            flags: raw[7],
            epoch: u64_at(raw, 8),
            sequence,
            frame: u64_at(raw, 24),
            transport: Transport {
                playing: raw[7] & PLAYING != 0,
                tempo: (bpm != 0.0).then_some(bpm),
            },
            process_ns: u64_at(raw, 48),
            missing_mask,
        },
        audio: &raw[HEADER_BYTES..HEADER_BYTES + samples * 4],
        midi: &raw[HEADER_BYTES + samples * 4..],
        config,
    };
    for i in 0..packet.midi_len() {
        let event = packet.midi(i);
        if event.frame >= config.packet_frames as u32
            || event.port != 0
            || event.bytes().is_err()
            || event.data[event.len as usize..].iter().any(|b| *b != 0)
        {
            return Err(Error::Packet);
        }
    }
    Ok(packet)
}

pub fn encode(
    header: Header,
    config: Config,
    audio: &[f32],
    midi: &[MidiEvent],
    raw: &mut [u8],
) -> Result<usize, Error> {
    let samples = config.packet_frames as usize * config.channels as usize;
    let length = HEADER_BYTES + samples * 4 + midi.len() * MIDI_BYTES;
    if audio.len() != samples || midi.len() > config.midi_per_packet as usize || raw.len() < length
    {
        return Err(Error::Capacity);
    }
    raw[..length].fill(0);
    raw[..4].copy_from_slice(b"SDSP");
    raw[4..6].copy_from_slice(&VERSION.to_le_bytes());
    raw[6] = header.kind;
    raw[7] = (header.flags & !PLAYING) | if header.transport.playing { PLAYING } else { 0 };
    raw[8..16].copy_from_slice(&header.epoch.to_le_bytes());
    raw[16..24].copy_from_slice(&header.sequence.to_le_bytes());
    raw[24..32].copy_from_slice(&header.frame.to_le_bytes());
    raw[32..34].copy_from_slice(&config.packet_frames.to_le_bytes());
    raw[34..36].copy_from_slice(&config.channels.to_le_bytes());
    raw[36..38].copy_from_slice(&(midi.len() as u16).to_le_bytes());
    raw[40..48].copy_from_slice(
        &header
            .transport
            .tempo
            .unwrap_or(0.0)
            .to_bits()
            .to_le_bytes(),
    );
    raw[48..56].copy_from_slice(&header.process_ns.to_le_bytes());
    raw[56..64].copy_from_slice(&header.missing_mask.to_le_bytes());
    for (bytes, sample) in raw[HEADER_BYTES..HEADER_BYTES + samples * 4]
        .chunks_exact_mut(4)
        .zip(audio)
    {
        bytes.copy_from_slice(&sample.to_le_bytes());
    }
    for (slot, event) in raw[HEADER_BYTES + samples * 4..length]
        .chunks_exact_mut(MIDI_BYTES)
        .zip(midi)
    {
        let bytes = event.bytes().map_err(|_| Error::Packet)?;
        if event.frame >= config.packet_frames as u32 || event.port != 0 {
            return Err(Error::Packet);
        }
        slot[..4].copy_from_slice(&event.frame.to_le_bytes());
        slot[4..6].copy_from_slice(&event.port.to_le_bytes());
        slot[6..8].copy_from_slice(&event.len.to_le_bytes());
        slot[8..8 + bytes.len()].copy_from_slice(bytes);
    }
    decode(&raw[..length], config)?;
    Ok(length)
}
