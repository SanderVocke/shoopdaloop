//! Sample-generic controls used by the production engine and hardware oracle.
//! Controls retain their original time domain across processing blocks.
use alloc::{boxed::Box, vec};
use rt_audio_math::{Ramp as DomainRamp, Sample};
use rt_runtime::audio_refs::GainEnvelope;

#[derive(Clone, Copy)]
pub struct Ramp<S: Sample = f32> {
    domain: DomainRamp<S>,
    pub(crate) value: S,
    pub(crate) remaining: u32,
}
impl<S: Sample> Ramp<S> {
    pub fn at(value: S) -> Self {
        Self {
            domain: DomainRamp::constant(value),
            value,
            remaining: 0,
        }
    }
    pub fn set(&mut self, target: S, frames: u32) {
        self.domain.set(target, frames);
        self.sync();
    }
    pub fn target(&self) -> S {
        self.domain.target
    }
    pub fn current(&self) -> S {
        self.value
    }
    pub fn steady(&self) -> bool {
        self.remaining == 0 || self.domain.start == self.domain.target
    }
    fn sync(&mut self) {
        self.value = self.domain.at(0);
        self.remaining = self.domain.duration.saturating_sub(self.domain.elapsed);
    }
    pub fn fill(&mut self, output: &mut [S]) {
        self.domain.fill(output);
        self.sync();
    }
    fn advance(&mut self, frames: usize) {
        self.domain
            .advance(u32::try_from(frames).unwrap_or(u32::MAX));
        self.sync();
    }
    pub(crate) fn envelope(&mut self, frames: usize) -> GainEnvelope<S> {
        debug_assert!(self.steady());
        let envelope = GainEnvelope::constant(self.value);
        self.advance(frames);
        envelope
    }
}

/// Gain, stereo balance and an additional scale, in their production arithmetic
/// order: gain * scale, then the balance factor. Scratch is allocated at prepare.
pub struct Gains<S: Sample = f32> {
    pub gain: Ramp<S>,
    pub balance: Ramp<S>,
    pub(crate) left: Box<[S]>,
    pub(crate) right: Box<[S]>,
    pub(crate) plain: Box<[S]>,
}
impl<S: Sample> Gains<S> {
    pub fn new(frames: usize) -> Self {
        Self {
            gain: Ramp::at(S::ONE),
            balance: Ramp::at(S::ZERO),
            left: vec![S::ZERO; frames].into_boxed_slice(),
            right: vec![S::ZERO; frames].into_boxed_slice(),
            plain: vec![S::ZERO; frames].into_boxed_slice(),
        }
    }
    pub fn reset(&mut self) {
        self.gain = Ramp::at(S::ONE);
        self.balance = Ramp::at(S::ZERO);
    }
    pub fn fill(&mut self, frames: usize, scale: &mut Ramp<S>) {
        if self.steady() && scale.steady() {
            let levels = self.levels(scale);
            self.plain[..frames].fill(levels[0]);
            self.left[..frames].fill(levels[1]);
            self.right[..frames].fill(levels[2]);
            self.gain.advance(frames);
            self.balance.advance(frames);
            scale.advance(frames);
            return;
        }
        self.gain.fill(&mut self.plain[..frames]);
        self.balance.fill(&mut self.left[..frames]);
        scale.fill(&mut self.right[..frames]);
        for i in 0..frames {
            let gain = self.plain[i] * self.right[i];
            let balance = self.left[i];
            self.plain[i] = gain;
            self.left[i] = gain * (S::ONE - balance.max(S::ZERO));
            self.right[i] = gain * (S::ONE + balance.min(S::ZERO));
        }
    }
    pub fn steady(&self) -> bool {
        self.gain.steady() && self.balance.steady()
    }
    pub(crate) fn envelopes(&mut self, frames: usize, scale: &mut Ramp<S>) -> [GainEnvelope<S>; 3] {
        debug_assert!(self.steady() && scale.steady());
        let envelopes = self.levels(scale).map(GainEnvelope::constant);
        self.gain.advance(frames);
        self.balance.advance(frames);
        scale.advance(frames);
        envelopes
    }
    fn levels(&self, scale: &Ramp<S>) -> [S; 3] {
        let gain = self.gain.current() * scale.current();
        let balance = self.balance.current();
        [
            gain,
            gain * (S::ONE - balance.max(S::ZERO)),
            gain * (S::ONE + balance.min(S::ZERO)),
        ]
    }
    pub(crate) fn unity(&self) -> bool {
        self.steady() && self.gain.value == S::ONE && self.balance.value == S::ZERO
    }
    pub fn channel(&self, channel: usize, stereo: bool) -> &[S] {
        match (stereo, channel) {
            (true, 0) => &self.left,
            (true, 1) => &self.right,
            _ => &self.plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_audio_math::Q824;
    use shoop_wasm_test_support::shoop_test;

    fn render<S: Sample>(partitions: &[usize]) -> alloc::vec::Vec<S> {
        let mut ramp = Ramp::at(S::ONE);
        ramp.set(-S::ONE, 7);
        let mut output = alloc::vec::Vec::new();
        for &frames in partitions {
            let mut block = vec![S::ZERO; frames];
            ramp.fill(&mut block);
            output.extend(block);
        }
        ramp.set(S::ONE, 3);
        let mut tail = [S::ZERO; 6];
        ramp.fill(&mut tail);
        output.extend(tail);
        output
    }

    #[shoop_test]
    fn production_controls_are_partition_invariant_and_match_float_with_bounded_error() {
        let fixed = render::<Q824>(&[9]);
        assert_eq!(fixed, render::<Q824>(&[1, 2, 3, 3]));
        let floating = render::<f32>(&[9]);
        assert_eq!(floating, render::<f32>(&[1, 2, 3, 3]));
        for (q, f) in fixed.iter().zip(floating) {
            assert!((q.to_f32() - f).abs() <= 2f32.powi(-22));
        }
        assert_eq!(fixed[0], Q824::ONE);
        assert_eq!(fixed[7], -Q824::ONE);
        assert_eq!(fixed[12], Q824::ONE);
        let mut interrupted = Ramp::at(Q824(16_777_217));
        interrupted.set(Q824(-16_777_219), 33);
        let mut first = [Q824::ZERO; 17];
        interrupted.fill(&mut first);
        let boundary = interrupted.current();
        interrupted.set(Q824(123), 1);
        let mut second = [Q824::ZERO; 3];
        interrupted.fill(&mut second);
        assert_eq!(second, [boundary, Q824(123), Q824(123)]);
        interrupted.set(Q824(-91), 0);
        assert_eq!(interrupted.current(), Q824(-91));
    }
}

/// Prepared production gain, also describable by the hardware adapter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainOperation<S: Sample> {
    pub target: S,
    pub duration: u32,
}
impl<S: Sample> rt_runtime::processor::ProcessorOperation for GainOperation<S> {
    type Result = ();
}
pub struct GainProcessor<S: Sample> {
    channels: usize,
    controls: Gains<S>,
    scale: Ramp<S>,
}
impl<S: Sample> GainProcessor<S> {
    pub fn new(channels: usize, frames: usize, initial: S) -> Self {
        let mut controls = Gains::new(frames);
        controls.gain = Ramp::at(initial);
        Self {
            channels,
            controls,
            scale: Ramp::at(S::ONE),
        }
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn frames(&self) -> usize {
        self.controls.plain.len()
    }
    pub fn steady(&self) -> bool {
        self.controls.gain.steady()
    }
    pub fn gain(&self) -> S {
        self.controls.gain.current()
    }
    pub fn apply(&mut self, operation: GainOperation<S>) {
        self.controls.gain.set(operation.target, operation.duration);
    }
    pub fn process<L: rt_runtime::planner::Planner<S>>(
        &mut self,
        block: &mut rt_runtime::processor::ProcessBlock<'_, L, S>,
    ) -> Result<(), rt_runtime::Error> {
        self.controls.fill(block.frames, &mut self.scale);
        for channel in 0..self.channels {
            let (source, target) = block
                .audio
                .pair(channel, channel)
                .ok_or(rt_runtime::Error::InvalidShape)?;
            rt_audio_math::gain_each(
                source,
                target,
                &self.controls.channel(channel, false)[..block.frames],
            );
        }
        Ok(())
    }
}
impl<S: Sample, L: rt_runtime::planner::Planner<S>>
    rt_runtime::processor::Processor<L, GainOperation<S>, S> for GainProcessor<S>
{
    fn ports(&self) -> rt_runtime::graph::PortCounts {
        rt_runtime::graph::PortCounts {
            audio_inputs: self.channels,
            audio_outputs: self.channels,
            ..Default::default()
        }
    }
    fn process(
        &mut self,
        block: &mut rt_runtime::processor::ProcessBlock<'_, L, S>,
    ) -> Result<(), rt_runtime::Error> {
        self.process(block)
    }
    fn operation(
        &mut self,
        operation: GainOperation<S>,
        _: rt_runtime::processor::OperationContext<'_, S>,
    ) -> Result<(), rt_runtime::Error> {
        self.apply(operation);
        Ok(())
    }
}
