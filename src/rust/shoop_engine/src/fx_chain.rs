//! An effect slot, and the built-in effects that can fill one.
//!
//! there is nothing to test it against here, so the abstraction is built first around effects that
//! come with the engine. That makes a plugin *another kind of processor* rather than a prerequisite:
//! the graph position, the dry/wet handling, the bypass and the session format can all settle
//! before a host exists, and none of them have to change when one arrives.
//!
//! Realtime-safe by construction. Everything an effect needs is allocated when it is configured;
//! processing only reads and writes. That is why the delay's buffer is sized from a maximum rather
//! than from its current time -- changing the time must not allocate.

use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum FXChainType {
    CarlaRack = 0,
    CarlaPatchbay = 1,
    CarlaPatchbay16x = 2,
    Test2x2x1 = 3,
    OxiSynth = 5,
    BuiltInFx = 6,
}

impl FXChainType {
    pub fn to_ffi(&self) -> u32 {
        *self as u32
    }
}

impl TryFrom<u32> for FXChainType {
    type Error = num_enum::TryFromPrimitiveError<FXChainType>;
    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        FXChainType::try_from(value as i32)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FXChainState {
    pub ready: u32,
    pub active: u32,
    pub visible: u32,
}

/// What an effect does to a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// Passes everything through. Useful as an explicit "nothing here".
    None,
    /// One-pole low-pass, the cheapest useful tone control.
    LowPass,
    /// Delay with feedback.
    Delay,
}

/// The longest delay that can be set, which fixes the buffer size.
///
/// Two seconds: long enough to be musical, short enough that the buffer is small.
pub const MAX_DELAY_SECONDS: f32 = 2.0;

#[derive(Debug)]
pub struct FxChain {
    kind: EffectKind,
    bypassed: bool,
    sample_rate: f32,

    /// How much of the processed signal is heard, against the untouched input.
    wet: f32,

    /// Low-pass coefficient, derived from the cutoff.
    lp_coeff: f32,
    lp_state: f32,
    cutoff_hz: f32,

    /// Delay line, sized for `MAX_DELAY_SECONDS` so changing the time never allocates.
    delay_buffer: Vec<f32>,
    delay_write: usize,
    delay_samples: usize,
    feedback: f32,
}

impl Default for FxChain {
    fn default() -> Self {
        let mut chain = Self {
            kind: EffectKind::None,
            bypassed: false,
            sample_rate: 48000.0,
            wet: 1.0,
            lp_coeff: 0.0,
            lp_state: 0.0,
            cutoff_hz: 2000.0,
            delay_buffer: Vec::new(),
            delay_write: 0,
            delay_samples: 0,
            feedback: 0.4,
        };
        chain.configure(48000);
        chain
    }
}

impl FxChain {
    /// Sizes everything for a sample rate. Allocates, so it is a control-path call.
    pub fn configure(&mut self, sample_rate: u32) {
        if sample_rate == 0 {
            return;
        }
        self.sample_rate = sample_rate as f32;
        let max = (MAX_DELAY_SECONDS * self.sample_rate).ceil() as usize;
        self.delay_buffer = vec![0.0; max.max(1)];
        self.delay_write = 0;
        self.set_delay_seconds(0.25);
        self.set_cutoff_hz(self.cutoff_hz);
        self.lp_state = 0.0;
    }

    pub fn kind(&self) -> EffectKind {
        self.kind
    }
    /// Changing effect resets its state, so a stale tail is not heard through a new effect.
    pub fn set_kind(&mut self, kind: EffectKind) {
        if self.kind != kind {
            self.kind = kind;
            self.reset();
        }
    }

    pub fn bypassed(&self) -> bool {
        self.bypassed
    }
    pub fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    pub fn wet(&self) -> f32 {
        self.wet
    }
    pub fn set_wet(&mut self, wet: f32) {
        self.wet = wet.clamp(0.0, 1.0);
    }

    pub fn cutoff_hz(&self) -> f32 {
        self.cutoff_hz
    }
    /// Cutoff, bounded so the coefficient stays sane at any sample rate.
    pub fn set_cutoff_hz(&mut self, hz: f32) {
        let nyquist = self.sample_rate / 2.0;
        self.cutoff_hz = hz.clamp(20.0, (nyquist - 100.0).max(100.0));
        // One-pole coefficient. Exact form matters less than that it is stable and monotonic.
        let x = (-std::f32::consts::TAU * self.cutoff_hz / self.sample_rate).exp();
        self.lp_coeff = x.clamp(0.0, 0.9999);
    }

    pub fn delay_seconds(&self) -> f32 {
        self.delay_samples as f32 / self.sample_rate
    }
    /// Delay time, clamped to the buffer so it can never read outside it.
    pub fn set_delay_seconds(&mut self, seconds: f32) {
        let max = self.delay_buffer.len().saturating_sub(1);
        let wanted = (seconds.max(0.0) * self.sample_rate) as usize;
        self.delay_samples = wanted.clamp(1, max.max(1));
    }

    pub fn feedback(&self) -> f32 {
        self.feedback
    }
    /// Feedback, bounded below one so the delay cannot run away.
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 0.95);
    }

    /// Clears whatever the effect was holding.
    pub fn reset(&mut self) {
        self.lp_state = 0.0;
        for s in self.delay_buffer.iter_mut() {
            *s = 0.0;
        }
        self.delay_write = 0;
    }

    /// Processes `input` into `output`, which may be the same length or shorter.
    ///
    /// Writes rather than adds, unlike a port: a chain replaces the signal passing through it.
    /// Nothing here allocates.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let n = input.len().min(output.len());
        if self.bypassed || self.kind == EffectKind::None {
            output[..n].copy_from_slice(&input[..n]);
            return;
        }

        for i in 0..n {
            let dry = input[i];
            let wet = match self.kind {
                EffectKind::None => dry,
                EffectKind::LowPass => {
                    self.lp_state = dry * (1.0 - self.lp_coeff) + self.lp_state * self.lp_coeff;
                    self.lp_state
                }
                EffectKind::Delay => {
                    let len = self.delay_buffer.len();
                    // Read before write, so a delay of one sample is still a delay.
                    let read = (self.delay_write + len - self.delay_samples) % len;
                    let delayed = self.delay_buffer[read];
                    self.delay_buffer[self.delay_write] = dry + delayed * self.feedback;
                    self.delay_write = (self.delay_write + 1) % len;
                    delayed
                }
            };
            output[i] = dry * (1.0 - self.wet) + wet * self.wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    fn chain(kind: EffectKind) -> FxChain {
        let mut c = FxChain::default();
        c.configure(48000);
        c.set_kind(kind);
        c
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |a, b| a.max(b.abs()))
    }

    /// A sine at `hz`, for measuring what a filter does to it.
    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48000.0).sin())
            .collect()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn no_effect_passes_the_signal_through() {
        let mut c = chain(EffectKind::None);
        let input = sine(440.0, 256);
        let mut out = vec![0.0; 256];
        c.process(&input, &mut out);
        check!(out == input);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bypass_passes_through_whatever_the_effect_is() {
        let mut c = chain(EffectKind::Delay);
        c.set_bypassed(true);
        let input = sine(440.0, 256);
        let mut out = vec![0.0; 256];
        c.process(&input, &mut out);
        check!(out == input);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_low_pass_attenuates_high_frequencies_more_than_low_ones() {
        let mut c = chain(EffectKind::LowPass);
        c.set_cutoff_hz(500.0);

        let mut low_out = vec![0.0; 4800];
        c.process(&sine(100.0, 4800), &mut low_out);
        let low = peak(&low_out[2400..]);

        c.reset();
        let mut high_out = vec![0.0; 4800];
        c.process(&sine(8000.0, 4800), &mut high_out);
        let high = peak(&high_out[2400..]);

        // The point of the filter: well above cutoff should be much quieter than well below.
        check!(low > high * 4.0, "low {low} was not far above high {high}");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_higher_cutoff_passes_more() {
        let input = sine(4000.0, 4800);

        let mut low_cut = chain(EffectKind::LowPass);
        low_cut.set_cutoff_hz(500.0);
        let mut a = vec![0.0; 4800];
        low_cut.process(&input, &mut a);

        let mut high_cut = chain(EffectKind::LowPass);
        high_cut.set_cutoff_hz(10000.0);
        let mut b = vec![0.0; 4800];
        high_cut.process(&input, &mut b);

        check!(peak(&b[2400..]) > peak(&a[2400..]));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_delay_repeats_the_signal_after_its_time() {
        let mut c = chain(EffectKind::Delay);
        c.set_delay_seconds(0.01); // 480 samples
        c.set_feedback(0.0);
        c.set_wet(1.0);

        // A single impulse, so the repeat is unmistakable.
        let mut input = vec![0.0f32; 2000];
        input[0] = 1.0;
        let mut out = vec![0.0; 2000];
        c.process(&input, &mut out);

        check!(out[0] == 0.0, "the delay produced output immediately");
        check!(
            out[480] > 0.9,
            "the repeat did not arrive at the delay time"
        );
        check!(
            peak(&out[1..480]) == 0.0,
            "there was output before the repeat"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn feedback_produces_further_repeats_that_decay() {
        let mut c = chain(EffectKind::Delay);
        c.set_delay_seconds(0.01);
        c.set_feedback(0.5);

        let mut input = vec![0.0f32; 2000];
        input[0] = 1.0;
        let mut out = vec![0.0; 2000];
        c.process(&input, &mut out);

        let first = out[480];
        let second = out[960];
        check!(second > 0.0, "feedback produced no second repeat");
        check!(second < first, "the repeats did not decay");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn feedback_cannot_be_set_to_run_away() {
        let mut c = chain(EffectKind::Delay);
        c.set_feedback(5.0);
        // Bounded below one, or the delay would grow without limit.
        check!(c.feedback() <= 0.95);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn the_wet_mix_blends_against_the_untouched_input() {
        let mut c = chain(EffectKind::Delay);
        c.set_delay_seconds(0.01);
        c.set_feedback(0.0);
        c.set_wet(0.0);

        let input = sine(440.0, 512);
        let mut out = vec![0.0; 512];
        c.process(&input, &mut out);
        // Fully dry, so the delay is inaudible even though it is running.
        check!(out == input);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_delay_longer_than_the_buffer_is_clamped() {
        let mut c = chain(EffectKind::Delay);
        c.set_delay_seconds(MAX_DELAY_SECONDS * 10.0);
        // Clamped rather than reading outside the buffer.
        check!(c.delay_seconds() <= MAX_DELAY_SECONDS);
        let mut out = vec![0.0; 128];
        c.process(&vec![0.5; 128], &mut out);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_zero_delay_is_still_at_least_one_sample() {
        let mut c = chain(EffectKind::Delay);
        c.set_delay_seconds(0.0);
        // Zero would read the sample being written, which is not a delay.
        check!(c.delay_seconds() > 0.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn changing_effect_clears_the_previous_tail() {
        let mut c = chain(EffectKind::Delay);
        c.set_delay_seconds(0.01);
        let mut input = vec![0.0f32; 200];
        input[0] = 1.0;
        let mut out = vec![0.0; 200];
        c.process(&input, &mut out);

        // Switched before the repeat arrived; it must not be heard through the new effect.
        c.set_kind(EffectKind::LowPass);
        c.set_kind(EffectKind::Delay);
        let mut after = vec![0.0; 1000];
        c.process(&vec![0.0f32; 1000], &mut after);
        check!(peak(&after) == 0.0, "a stale tail survived the change");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cutoff_is_bounded_by_the_sample_rate() {
        let mut c = chain(EffectKind::LowPass);
        c.set_cutoff_hz(1_000_000.0);
        // Below Nyquist, or the coefficient stops meaning anything.
        check!(c.cutoff_hz() < 24000.0);
        c.set_cutoff_hz(0.0);
        check!(c.cutoff_hz() >= 20.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_shorter_output_than_input_is_not_overrun() {
        let mut c = chain(EffectKind::LowPass);
        let input = vec![0.5f32; 256];
        let mut out = vec![0.0; 64];
        c.process(&input, &mut out);
        check!(out.iter().all(|&v| v != 0.0));
    }
}
