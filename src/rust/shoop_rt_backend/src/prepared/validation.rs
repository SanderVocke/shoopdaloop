use super::{initial_message, MAX_MEDIA_BYTES, MAX_MIDI_EVENTS};
use anyhow::{anyhow, bail, Result};
use shoop_backend::{BackendChannelMode, BackendLoopContent};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Prepared {
    image: Arc<BackendLoopContent>,
    bytes: u64,
    events_bytes: u64,
    rendered_midi_bytes: u64,
    rendered_bytes: u64,
}
impl Prepared {
    pub fn matches(&self, image: &Arc<BackendLoopContent>) -> bool {
        Arc::ptr_eq(&self.image, image)
    }
    pub fn image(&self) -> &Arc<BackendLoopContent> {
        &self.image
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn events_bytes(&self) -> u64 {
        self.events_bytes
    }
    pub fn rendered_midi_bytes(&self) -> u64 {
        self.rendered_midi_bytes
    }
    pub fn rendered_bytes(&self) -> u64 {
        self.rendered_bytes
    }
}
pub(crate) struct Validation {
    image: Arc<BackendLoopContent>,
    audio: usize,
    sample: usize,
    initial: usize,
    event: usize,
    previous: u32,
    prefix: usize,
    history: bool,
    bytes: u64,
    events_bytes: u64,
    rendered_midi_bytes: u64,
    rendered_audio_bytes: u64,
    done: bool,
}
impl Validation {
    pub fn new(image: Arc<BackendLoopContent>, channels: usize) -> Result<Self> {
        if image.audio.len() > channels
            || image.midi.len() > 1
            || !image.gain.is_finite()
            || !image.balance.is_finite()
        {
            bail!("unsupported content shape or controls");
        }
        let mut bytes = 0u64;
        for audio in &image.audio {
            if audio.mode != BackendChannelMode::Direct
                || !audio.gain.is_finite()
                || audio.preplay != 0
            {
                bail!("audio requires unsupported channel mode, gain or preplay");
            }
            bytes = bytes
                .checked_add(audio.samples.len() as u64 * 4)
                .ok_or_else(|| anyhow!("media size overflow"))?;
        }
        if image.midi.iter().any(|m| {
            m.mode != BackendChannelMode::Direct
                || m.preplay != 0
                || m.start_state.len() > 16_384
                || m.events.len() > MAX_MIDI_EVENTS
        }) {
            bail!("MIDI requires unsupported channel mode, preplay or event count");
        }
        let rendered_audio_bytes = u64::from(image.length)
            .checked_mul(channels as u64 * 4)
            .ok_or_else(|| anyhow!("prepared audio size overflow"))?;
        if bytes > MAX_MEDIA_BYTES {
            bail!("content exceeds the 256 MiB media budget");
        }
        Ok(Self {
            image,
            audio: 0,
            sample: 0,
            initial: 0,
            event: 0,
            previous: 0,
            prefix: 0,
            history: false,
            bytes,
            events_bytes: 0,
            rendered_midi_bytes: 0,
            rendered_audio_bytes,
            done: false,
        })
    }
    pub fn image(&self) -> &Arc<BackendLoopContent> {
        &self.image
    }
    pub fn step(&mut self, budget: usize) -> Result<bool> {
        if self.done {
            return Ok(true);
        }
        let mut remaining = budget.max(4);
        while self.audio < self.image.audio.len() {
            let audio = &self.image.audio[self.audio];
            let end = self
                .sample
                .saturating_add((remaining / 4).max(1))
                .min(audio.samples.len());
            if audio.samples[self.sample..end]
                .iter()
                .any(|s| !s.is_finite())
            {
                bail!("audio contains a non-finite sample");
            }
            remaining = remaining.saturating_sub((end - self.sample) * 4);
            self.sample = end;
            if end == audio.samples.len() {
                self.audio += 1;
                self.sample = 0;
            }
            if remaining == 0 {
                return Ok(false);
            }
        }
        if let Some(midi) = self.image.midi.first() {
            while self.initial < midi.start_state.len() && remaining > 0 {
                let packet = &midi.start_state[self.initial];
                initial_message(packet)?;
                self.history = history(self.history, packet);
                self.bytes += packet.len() as u64 + 4;
                remaining = remaining.saturating_sub(packet.len() + 4);
                self.initial += 1;
            }
            if self.initial < midi.start_state.len() {
                return Ok(false);
            }
            let origin = i64::from(midi.start_offset) + i64::from(midi.capture_alignment_frames);
            while self.event < midi.events.len() && remaining > 0 {
                let event = &midi.events[self.event];
                if event.frame < self.previous
                    || event.frame >= midi.length
                    || event.data.is_empty()
                    || event.data.len() > 1024 * 1024
                {
                    bail!("invalid MIDI event order, position or packet");
                }
                if i64::from(event.frame) < origin {
                    self.prefix += 1;
                    if self.prefix + midi.start_state.len() > 16_384 {
                        bail!("MIDI initial/preroll state exceeds its bounded packet count");
                    }
                    initial_message(&event.data)?;
                    self.history = history(self.history, &event.data);
                } else if i64::from(event.frame) < origin + i64::from(self.image.length) {
                    self.rendered_midi_bytes += event.data.len() as u64 + 12;
                }
                let bytes = event.data.len() + 12;
                self.bytes += bytes as u64;
                self.events_bytes += bytes as u64;
                self.previous = event.frame;
                self.event += 1;
                remaining = remaining.saturating_sub(bytes);
            }
            if self.event < midi.events.len() {
                return Ok(false);
            }
            if self.history {
                bail!("MIDI initial/preroll parameter history is not supported yet (P11)");
            }
        }
        if self.bytes > MAX_MEDIA_BYTES {
            bail!("content exceeds the 256 MiB canonical or prepared media budget");
        }
        self.done = true;
        Ok(true)
    }
    pub fn finish(self) -> Result<Prepared> {
        if !self.done {
            bail!("content validation is incomplete");
        }
        Ok(Prepared {
            image: self.image,
            bytes: self.bytes,
            events_bytes: self.events_bytes,
            rendered_midi_bytes: self.rendered_midi_bytes,
            rendered_bytes: self.rendered_audio_bytes + self.rendered_midi_bytes,
        })
    }
}
fn history(previous: bool, packet: &[u8]) -> bool {
    if packet == [0xff] {
        false
    } else {
        previous || matches!(packet, [0xb0..=0xbf, 6 | 38 | 96 | 97, _])
    }
}
