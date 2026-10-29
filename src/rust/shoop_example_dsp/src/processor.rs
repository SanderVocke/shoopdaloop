use crate::{Config, Meters, Operation, Preset, Receipt, Refusal};
use rt_runtime::{
    graph::PortCounts,
    planner::Planner,
    processor::{OperationContext, ProcessBlock, Processor},
    Error,
};

pub struct Dsp {
    config: Config,
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
        presets: Box<[Preset]>,
        active: Preset,
        delay: Box<[[f32; 2]]>,
        meters: Meters,
    ) -> Self {
        Self {
            config,
            presets,
            active,
            revision: 1,
            delay,
            cursor: 0,
            meters,
        }
    }
    /// Revision of the active preset; each applied selection advances it.
    pub fn revision(&self) -> u32 {
        self.revision
    }
}

impl<L: Planner> Processor<L, Operation> for Dsp {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 4,
            audio_outputs: 2,
            midi_inputs: 2,
            midi_outputs: 1,
        }
    }
    /// Selection resets the delay history to silence at its exact sample;
    /// selecting the active preset again is still a new revision.
    fn operation(
        &mut self,
        Operation::SelectPreparedPreset {
            preset,
            expected_revision,
        }: Operation,
        context: OperationContext<'_>,
    ) -> Result<Result<Receipt, Refusal>, Error> {
        if context.frame != context.requested_frame {
            return Ok(Err(Refusal::Late {
                requested: context.requested_frame,
                decision_frame: context.frame,
            }));
        }
        let Some(active) = self
            .presets
            .get(preset.slot() as usize)
            .filter(|candidate| candidate.id == preset)
            .copied()
        else {
            return Ok(Err(Refusal::InvalidPreset { preset }));
        };
        if expected_revision != self.revision {
            return Ok(Err(Refusal::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            }));
        }
        let Some(revision) = self.revision.checked_add(1) else {
            return Ok(Err(Refusal::RevisionExhausted {
                frame: context.frame,
            }));
        };
        self.active = active;
        self.delay.fill([0.; 2]);
        self.cursor = 0;
        let previous_revision = self.revision;
        self.revision = revision;
        Ok(Ok(Receipt {
            requested_frame: context.requested_frame,
            frame: context.frame,
            previous_revision,
            revision,
            preset,
        }))
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
        let mut peaks = [0f32; 2];
        for offset in 0..block.frames {
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
