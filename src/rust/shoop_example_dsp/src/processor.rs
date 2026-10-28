use crate::{control::Mailbox, Config, Meters, Preset};
use rt_runtime::{
    graph::PortCounts,
    planner::Planner,
    processor::{ProcessBlock, Processor},
    Error, Frame,
};
use std::sync::{atomic::Ordering, Arc};

pub struct Dsp {
    config: Config,
    mailbox: Arc<Mailbox>,
    presets: Box<[Preset]>,
    active: Preset,
    revision: u32,
    delay: Box<[[f32; 2]]>,
    cursor: usize,
    meters: Meters,
}

impl Dsp {
    pub(crate) fn new(
        config: Config,
        mailbox: Arc<Mailbox>,
        presets: Box<[Preset]>,
        active: Preset,
        delay: Box<[[f32; 2]]>,
        meters: Meters,
    ) -> Self {
        Self {
            config,
            mailbox,
            presets,
            active,
            revision: 1,
            delay,
            cursor: 0,
            meters,
        }
    }
    fn activate(&mut self, frame: Frame) -> Result<(), Error> {
        if self.revision != self.mailbox.revision.load(Ordering::Relaxed) {
            return Err(Error::InvalidState);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Capacity)?;
        let slot = self.mailbox.slot.load(Ordering::Relaxed) as usize;
        let active = *self.presets.get(slot).ok_or(Error::InvalidId)?;
        self.active = active;
        self.delay.fill([0.; 2]);
        self.cursor = 0;
        let previous_revision = self.revision;
        self.revision = revision;
        self.mailbox.armed.store(false, Ordering::Relaxed);
        self.mailbox.actual_frame.store(frame.0, Ordering::Relaxed);
        self.mailbox
            .previous_revision
            .store(previous_revision, Ordering::Relaxed);
        self.mailbox
            .actual_revision
            .store(revision, Ordering::Relaxed);
        self.mailbox.acknowledged.store(true, Ordering::Release);
        Ok(())
    }
}

impl<L: Planner> Processor<L> for Dsp {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 4,
            audio_outputs: 2,
            midi_inputs: 2,
            midi_outputs: 1,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, L>) -> Result<(), Error> {
        if block.frames == 0 {
            return Ok(());
        }
        if block.frames > self.config.max_frames
            || block.midi_inputs.len() != 2
            || block.midi_outputs.len() != 1
        {
            return Err(Error::InvalidShape);
        }
        if block
            .midi_inputs
            .iter()
            .any(|input| input.len() > self.config.midi_events_per_input)
        {
            return Err(Error::Capacity);
        }
        let activation = self
            .mailbox
            .armed
            .load(Ordering::Acquire)
            .then(|| Frame(self.mailbox.frame.load(Ordering::Relaxed)));
        let mut peaks = [0f32; 2];
        for offset in 0..block.frames {
            let frame = Frame(
                block
                    .frame
                    .0
                    .checked_add(offset as u64)
                    .ok_or(Error::Capacity)?,
            );
            if activation == Some(frame) {
                self.activate(frame)?;
            }
            for channel in 0..2 {
                let source = if self.active.swap_channels {
                    1 - channel
                } else {
                    channel
                };
                let live = block.audio.input(source).ok_or(Error::InvalidShape)?[offset];
                let playback = block.audio.input(source + 2).ok_or(Error::InvalidShape)?[offset];
                self.delay[self.cursor][channel] = live * self.active.live_gain[channel]
                    + playback * self.active.playback_gain[channel];
            }
            let source =
                (self.cursor + self.delay.len() - self.active.delay_frames) % self.delay.len();
            for (channel, peak) in peaks.iter_mut().enumerate() {
                let sample = self.delay[source][channel];
                block.audio.output(channel).ok_or(Error::InvalidShape)?[offset] = sample;
                if !sample.is_nan() {
                    *peak = peak.max(sample.abs().min(f32::MAX));
                }
            }
            self.cursor += 1;
            if self.cursor == self.delay.len() {
                self.cursor = 0;
            }
        }
        self.meters.publish(peaks);
        let mut positions = [0; 2];
        while let Some((offset, port)) = (0..2)
            .filter_map(|port| {
                block.midi_inputs[port]
                    .get(positions[port])
                    .map(|(offset, _)| (offset, port))
            })
            .min()
        {
            let (_, packet) = block.midi_inputs[port]
                .get(positions[port])
                .ok_or(Error::InvalidState)?;
            if packet.len() > self.config.packet_bytes {
                return Err(Error::Capacity);
            }
            block.midi_outputs[0].push(offset, block.frames, packet)?;
            positions[port] += 1;
        }
        Ok(())
    }
}
