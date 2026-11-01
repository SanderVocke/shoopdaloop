//! Canonical content and the bounded, normalized byte stream presented to the engine.
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use shoop_backend::{BackendChannelMode, BackendLoopContent, BackendMidiContent};
use shoop_rt_protocol::{ContentChunk, ContentDescription, MidiMessage};

pub(crate) const MAX_MEDIA_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_MIDI_EVENTS: usize = 1_000_000;

pub(crate) struct Upload {
    pub description: ContentDescription,
    content: Arc<BackendLoopContent>,
    channels: usize,
    offset: u64,
    event: usize,
    event_end: usize,
    event_byte: usize,
    initial: usize,
    initial_end: usize,
}

pub(crate) fn canonical_bytes(content: &BackendLoopContent) -> Result<u64> {
    let mut bytes = 0u64;
    for audio in &content.audio {
        if audio.mode != BackendChannelMode::Direct || !audio.gain.is_finite() || audio.preplay != 0
        {
            bail!("audio requires unsupported channel mode, gain or preplay");
        }
        if audio.samples.iter().any(|sample| !sample.is_finite()) {
            bail!("audio contains a non-finite sample");
        }
        bytes = bytes
            .checked_add(audio.samples.len() as u64 * 4)
            .ok_or_else(|| anyhow!("media size overflow"))?;
    }
    for midi in &content.midi {
        if midi.mode != BackendChannelMode::Direct
            || midi.start_state.len() > 16_384
            || midi.preplay != 0
            || midi.events.len() > MAX_MIDI_EVENTS
        {
            bail!("MIDI requires unsupported channel mode, preplay or event count");
        }
        let mut previous = 0;
        for event in &midi.events {
            if event.time < previous || event.time >= midi.length || event.data.is_empty() {
                bail!("invalid MIDI event order, position or packet");
            }
            previous = event.time;
            bytes = bytes
                .checked_add(event.data.len() as u64 + 12)
                .ok_or_else(|| anyhow!("media size overflow"))?;
        }
        validate_initial_parameters(midi)?;
        for packet in &midi.start_state {
            bytes = bytes
                .checked_add(packet.len() as u64 + 4)
                .ok_or_else(|| anyhow!("media size overflow"))?;
        }
    }
    if bytes > MAX_MEDIA_BYTES {
        bail!("decoded content exceeds the 256 MiB media budget");
    }
    Ok(bytes)
}
fn initial_message(packet: &[u8]) -> Result<MidiMessage> {
    let valid = match packet {
        [0x80..=0xbf | 0xe0..=0xef, a, b] => *a < 128 && *b < 128,
        [0xc0..=0xdf, a] => *a < 128,
        [0xff] => true,
        _ => false,
    };
    if !valid {
        bail!("MIDI initial state requires an unsupported packet");
    }
    MidiMessage::new(packet).ok_or_else(|| anyhow!("invalid MIDI initial packet"))
}
fn validate_initial_parameters(channel: &BackendMidiContent) -> Result<()> {
    let origin = i64::from(channel.start_offset) + i64::from(channel.capture_alignment_frames);
    let prefix = channel
        .events
        .partition_point(|event| i64::from(event.time) < origin);
    if channel.start_state.len() + prefix > 16_384 {
        bail!("MIDI initial/preroll state exceeds its bounded packet count");
    }
    let mut history = false;
    for packet in channel.start_state.iter().map(Vec::as_slice).chain(
        channel.events[..prefix]
            .iter()
            .map(|event| event.data.as_slice()),
    ) {
        initial_message(packet)?;
        if packet == [0xff] {
            history = false;
        } else if matches!(packet, [0xb0..=0xbf, 6 | 38 | 96 | 97, _]) {
            history = true;
        }
    }
    if history {
        bail!("MIDI initial/preroll parameter history is not supported yet (P11)");
    }
    Ok(())
}
impl Upload {
    pub fn new(
        id: u64,
        midi: bool,
        content: Arc<BackendLoopContent>,
        channels: usize,
    ) -> Result<Self> {
        let (mut event, mut event_end, mut initial_end) = (0, 0, 0);
        let bytes = if midi {
            if let Some(channel) = content.midi.first() {
                let origin =
                    i64::from(channel.start_offset) + i64::from(channel.capture_alignment_frames);
                event = channel
                    .events
                    .partition_point(|event| i64::from(event.time) < origin);
                initial_end = event;
                event_end = channel.events.partition_point(|event| {
                    i64::from(event.time) < origin + i64::from(content.length)
                });
                validate_initial_parameters(channel)?;
                channel.events[event..event_end]
                    .iter()
                    .try_fold(0u64, |n, event| {
                        n.checked_add(12 + event.data.len() as u64)
                            .ok_or_else(|| anyhow!("MIDI size overflow"))
                    })?
            } else {
                0
            }
        } else {
            u64::from(content.length)
                .checked_mul(channels as u64 * 4)
                .ok_or_else(|| anyhow!("audio size overflow"))?
        };
        if bytes > MAX_MEDIA_BYTES {
            bail!("prepared content exceeds the 256 MiB media budget");
        }
        Ok(Self {
            description: ContentDescription {
                id,
                midi,
                frames: u64::from(content.length),
                bytes,
            },
            content,
            channels,
            offset: 0,
            event,
            event_end,
            event_byte: 0,
            initial: 0,
            initial_end,
        })
    }
    pub fn initial(&mut self) -> Result<Option<MidiMessage>> {
        if !self.description.midi {
            return Ok(None);
        }
        let Some(channel) = self.content.midi.first() else {
            return Ok(None);
        };
        let packet = if self.initial < channel.start_state.len() {
            Some(channel.start_state[self.initial].as_slice())
        } else {
            channel
                .events
                .get(self.initial - channel.start_state.len())
                .filter(|_| self.initial - channel.start_state.len() < self.initial_end)
                .map(|event| event.data.as_slice())
        };
        match packet {
            Some(packet) => {
                self.initial += 1;
                initial_message(packet).map(Some)
            }
            None => Ok(None),
        }
    }
    pub fn chunk(&mut self, maximum: usize) -> Option<(u64, ContentChunk)> {
        if self.offset == self.description.bytes {
            return None;
        }
        let offset = self.offset;
        let count = maximum.min((self.description.bytes - offset) as usize);
        let mut bytes = [0; shoop_rt_protocol::content::CONTENT_CHUNK_BYTES];
        if self.description.midi {
            let channel = self.content.midi.first().expect("nonempty MIDI upload");
            let origin =
                i64::from(channel.start_offset) + i64::from(channel.capture_alignment_frames);
            for byte in &mut bytes[..count] {
                let event = &channel.events[self.event];
                let frame = (i64::from(event.time) - origin) as u64;
                *byte = match self.event_byte {
                    0..=7 => frame.to_le_bytes()[self.event_byte],
                    8..=11 => (event.data.len() as u32).to_le_bytes()[self.event_byte - 8],
                    _ => event.data[self.event_byte - 12],
                };
                self.event_byte += 1;
                if self.event_byte == 12 + event.data.len() {
                    self.event += 1;
                    self.event_byte = 0;
                }
            }
            debug_assert!(self.event <= self.event_end);
        } else {
            for (index, word) in bytes[..count].chunks_exact_mut(4).enumerate() {
                let sample = (offset / 4) as usize + index;
                let frame = sample / self.channels;
                let sample = self
                    .content
                    .audio
                    .get(sample % self.channels)
                    .map_or(0., |channel| {
                        let raw = frame as i64
                            + i64::from(channel.start_offset)
                            + i64::from(channel.capture_alignment_frames);
                        usize::try_from(raw)
                            .ok()
                            .and_then(|raw| channel.samples.get(raw))
                            .copied()
                            .unwrap_or(0.)
                            * channel.gain
                    });
                word.copy_from_slice(&sample.to_le_bytes());
            }
        }
        self.offset += count as u64;
        Some((
            offset,
            ContentChunk::new(&bytes[..count]).expect("prepared chunk bound"),
        ))
    }
}
