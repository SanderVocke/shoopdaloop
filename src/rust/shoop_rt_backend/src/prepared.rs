//! Canonical content and the bounded, normalized byte stream presented to the engine.
mod validation;
use std::sync::Arc;
pub(crate) use validation::{Prepared, Validation};

use anyhow::{anyhow, bail, Result};
use shoop_backend::BackendLoopContent;
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
    certified: bool,
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
impl Upload {
    pub fn new(
        id: u64,
        midi: bool,
        content: Arc<BackendLoopContent>,
        channels: usize,
    ) -> Result<Self> {
        let (event, event_end, initial_end) = (0, 0, 0);
        let bytes = if midi {
            0
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
            certified: false,
        })
    }
    pub fn source(&self) -> u64 {
        self.content.source_id
    }
    pub fn certify(&mut self, prepared: &Prepared) -> Result<()> {
        if self.certified {
            return Ok(());
        }
        if !prepared.matches(&self.content) {
            bail!("content proof belongs to another image");
        }
        if self.description.midi {
            self.description.bytes = prepared.rendered_midi_bytes();
            if let Some(channel) = self.content.midi.first() {
                let origin =
                    i64::from(channel.start_offset) + i64::from(channel.capture_alignment_frames);
                self.event = channel
                    .events
                    .partition_point(|event| i64::from(event.frame) < origin);
                self.initial_end = self.event;
                self.event_end = channel.events.partition_point(|event| {
                    i64::from(event.frame) < origin + i64::from(self.content.length)
                });
            }
        }
        self.certified = true;
        Ok(())
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
                .map(|event| event.data.as_ref())
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
                let frame = (i64::from(event.frame) - origin) as u64;
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
