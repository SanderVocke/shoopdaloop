//! Canonical raw content and bounded uploads, with independent playback windows.
mod validation;
use alloc::sync::Arc;
pub(crate) use validation::{Prepared, Validation};

use anyhow::{anyhow, bail, Result};
use shoop_backend::{BackendChannelMode, BackendLoopContent};
use shoop_rt_protocol::{ContentChunk, ContentDescription, MidiMessage};

pub(crate) const MAX_MEDIA_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_MIDI_EVENTS: usize = 1_000_000;

pub(crate) struct Upload {
    pub description: ContentDescription,
    content: Arc<BackendLoopContent>,
    channels: usize,
    audio_start: usize,
    audio_len: usize,
    offset: u64,
    event: usize,
    event_end: usize,
    event_byte: usize,
    initial: usize,
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
        let (event, event_end) = (0, 0);
        let frames = if midi {
            content
                .midi
                .first()
                .map_or(0, |channel| u64::from(channel.length))
        } else {
            content
                .audio
                .iter()
                .map(|channel| channel.samples.len() as u64)
                .max()
                .unwrap_or(0)
        };
        let bytes = if midi {
            0
        } else {
            frames
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
                frames,
                bytes,
            },
            channels,
            audio_start: 0,
            audio_len: content.audio.len(),
            content,
            offset: 0,
            event,
            event_end,
            event_byte: 0,
            initial: 0,
            certified: false,
        })
    }
    /// Normalize one logical audio role into its independently prepared bank.
    pub fn audio_role(
        id: u64,
        content: Arc<BackendLoopContent>,
        channels: usize,
        role: BackendChannelMode,
    ) -> Result<Self> {
        let start = content
            .audio
            .iter()
            .position(|c| c.mode == role)
            .unwrap_or(0);
        let len = content.audio.iter().filter(|c| c.mode == role).count();
        let mut upload = Self::new(id, false, content, channels)?;
        upload.audio_start = start;
        upload.audio_len = len;
        Ok(upload)
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
            self.description.bytes = prepared.events_bytes();
            self.event = 0;
            self.event_end = self
                .content
                .midi
                .first()
                .map_or(0, |channel| channel.events.len());
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
        let packet = channel
            .start_state
            .get(self.initial)
            .map(|packet| packet.as_slice());
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
            for byte in &mut bytes[..count] {
                let event = &channel.events[self.event];
                let frame = u64::from(event.frame);
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
                let lane = sample % self.channels;
                let sample = (lane < self.audio_len)
                    .then(|| &self.content.audio[self.audio_start + lane])
                    .map_or(0., |channel| {
                        channel.samples.get(frame).copied().unwrap_or(0.)
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

pub(crate) fn windows(content: &BackendLoopContent) -> shoop_rt_protocol::PlaybackWindows {
    let mut result = shoop_rt_protocol::PlaybackWindows::default();
    for (index, window) in result.audio.iter_mut().enumerate() {
        *window = content.audio.get(index).map_or(
            shoop_rt_protocol::ChannelWindow {
                raw_frames: Some(0),
                ..Default::default()
            },
            |channel| shoop_rt_protocol::ChannelWindow {
                gain_bits: channel.gain.to_bits(),
                start_offset: channel.start_offset,
                capture_alignment: channel.capture_alignment_frames,
                preplay: channel.preplay,
                raw_frames: Some(channel.samples.len() as u64),
            },
        );
    }
    result.midi = content.midi.first().map_or(
        shoop_rt_protocol::ChannelWindow {
            raw_frames: Some(0),
            ..Default::default()
        },
        |channel| shoop_rt_protocol::ChannelWindow {
            start_offset: channel.start_offset,
            capture_alignment: channel.capture_alignment_frames,
            preplay: channel.preplay,
            raw_frames: Some(u64::from(channel.length)),
            ..Default::default()
        },
    );
    result
}

pub(crate) fn timing(
    content: &BackendLoopContent,
    content_revision: u64,
    revision: u64,
) -> shoop_rt_protocol::TimingState {
    shoop_rt_protocol::TimingState {
        revision,
        content_revision,
        length: u64::from(content.length),
        raw_audio_frames: content
            .audio
            .iter()
            .map(|channel| channel.samples.len() as u64)
            .max()
            .unwrap_or(0),
        raw_midi_frames: content
            .midi
            .first()
            .map_or(0, |channel| u64::from(channel.length)),
        windows: windows(content),
    }
}
