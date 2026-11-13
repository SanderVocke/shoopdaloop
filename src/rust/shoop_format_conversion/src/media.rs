use super::{invalid, FrameScale};
use crate::{Error as SessionError, ExactMidi};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MediaTiming {
    pub scale: FrameScale,
    pub origin: i64,
    pub target_origin: i64,
}

impl MediaTiming {
    pub fn new(scale: FrameScale, origin: i64) -> Result<Self, SessionError> {
        if scale.numerator == 0 {
            return Err(invalid("media scale must be positive"));
        }
        let target_origin = i64::try_from(scale.signed_ceil(i128::from(origin))?)
            .map_err(|_| invalid("media origin overflow"))?;
        Ok(Self {
            scale,
            origin,
            target_origin,
        })
    }

    pub fn frame(self, frame: i128) -> Result<i128, SessionError> {
        i128::from(self.target_origin)
            .checked_add(
                self.scale.signed_floor(
                    frame
                        .checked_sub(i128::from(self.origin))
                        .ok_or_else(|| invalid("media frame overflow"))?,
                )?,
            )
            .ok_or_else(|| invalid("media frame overflow"))
    }

    pub fn length(self, frames: u64) -> Result<u64, SessionError> {
        if frames == 0 {
            return Ok(0);
        }
        let end = self
            .scale
            .signed_ceil(i128::from(frames) - i128::from(self.origin))?
            .checked_add(i128::from(self.target_origin))
            .ok_or_else(|| invalid("media length overflow"))?;
        u64::try_from(end).map_err(|_| invalid("media length overflow"))
    }

    fn source(self, frame: usize) -> Result<(i128, f64), SessionError> {
        let numerator = (frame as i128 - i128::from(self.target_origin))
            .checked_mul(
                i128::try_from(self.scale.denominator)
                    .map_err(|_| invalid("media scale overflow"))?,
            )
            .ok_or_else(|| invalid("source position overflow"))?;
        let denominator =
            i128::try_from(self.scale.numerator).map_err(|_| invalid("media scale overflow"))?;
        let position = numerator
            .div_euclid(denominator)
            .checked_add(i128::from(self.origin))
            .ok_or_else(|| invalid("source position overflow"))?;
        Ok((
            position,
            numerator.rem_euclid(denominator) as f64 / denominator as f64,
        ))
    }
}

const TAPS: usize = 48;
const PHASES: usize = 256;

struct SincKernel(Vec<[f64; TAPS]>);
impl SincKernel {
    fn new(scale: FrameScale) -> Result<Self, SessionError> {
        let ratio = scale.ratio();
        if !ratio.is_finite() || ratio <= 0.0 {
            return Err(invalid("invalid audio conversion ratio"));
        }
        let cutoff = 0.95 * ratio.min(1.0);
        let mut phases = Vec::with_capacity(PHASES + 1);
        for phase in 0..=PHASES {
            let fraction = phase as f64 / PHASES as f64;
            let mut taps = [0.0; TAPS];
            for (index, weight) in taps.iter_mut().enumerate() {
                let distance = index as f64 - 23.0 - fraction;
                let angle = PI * distance / 24.0;
                let window = (0.35875
                    + 0.48829 * angle.cos()
                    + 0.14128 * (2.0 * angle).cos()
                    + 0.01168 * (3.0 * angle).cos())
                .powi(2);
                let x = PI * cutoff * distance;
                *weight = window
                    * if x.abs() < 1.0e-12 {
                        cutoff
                    } else {
                        cutoff * x.sin() / x
                    };
            }
            let normalization = taps.iter().sum::<f64>();
            for weight in &mut taps {
                *weight /= normalization;
            }
            phases.push(taps);
        }
        Ok(Self(phases))
    }
}

/// Bounded incremental conversion with zero filter delay in the output timeline.
pub struct AudioConversion<'a> {
    input: &'a [f32],
    timing: MediaTiming,
    periodic: bool,
    length: usize,
    output: Vec<f32>,
    kernel: SincKernel,
}

impl<'a> AudioConversion<'a> {
    pub fn new(
        input: &'a [f32],
        timing: MediaTiming,
        periodic: bool,
        max_frames: u64,
    ) -> Result<Self, SessionError> {
        let length = timing.length(input.len() as u64)?;
        if length > max_frames {
            return Err(SessionError::ResourceLimit(
                "converted audio exceeds frame budget".into(),
            ));
        }
        let length = usize::try_from(length).map_err(|_| invalid("audio frame count overflow"))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(length)
            .map_err(|_| SessionError::ResourceLimit("converted audio allocation failed".into()))?;
        Ok(Self {
            input,
            timing,
            periodic,
            length,
            output,
            kernel: SincKernel::new(timing.scale)?,
        })
    }

    pub fn step(&mut self, frames: usize) -> Result<bool, SessionError> {
        let end = self.output.len().saturating_add(frames).min(self.length);
        if self.timing.scale.is_identity() {
            self.output
                .extend_from_slice(&self.input[self.output.len()..end]);
            return Ok(end == self.length);
        }
        for frame in self.output.len()..end {
            let (position, fraction) = self.timing.source(frame)?;
            let phase = fraction * PHASES as f64;
            let index = (phase.floor() as usize).min(PHASES - 1);
            let fraction = phase - index as f64;
            let mut sample = 0.0;
            for tap in 0..TAPS {
                let source = position + tap as i128 - 23;
                let value = if self.periodic && !self.input.is_empty() {
                    self.input[source.rem_euclid(self.input.len() as i128) as usize]
                } else {
                    usize::try_from(source)
                        .ok()
                        .and_then(|index| self.input.get(index))
                        .copied()
                        .unwrap_or(0.0)
                };
                let left = self.kernel.0[index][tap];
                let right = self.kernel.0[index + 1][tap];
                sample += f64::from(value) * (left + fraction * (right - left));
            }
            self.output.push(sample as f32);
        }
        Ok(end == self.length)
    }

    pub fn finish(self) -> Result<Vec<f32>, SessionError> {
        if self.output.len() != self.length {
            return Err(invalid("audio conversion is incomplete"));
        }
        Ok(self.output)
    }
}

pub fn convert_audio(
    input: &[f32],
    timing: MediaTiming,
    periodic: bool,
    max_frames: u64,
) -> Result<Vec<f32>, SessionError> {
    let mut conversion = AudioConversion::new(input, timing, periodic, max_frames)?;
    while !conversion.step(4096)? {}
    conversion.finish()
}

pub fn convert_midi(
    input: &ExactMidi,
    timing: MediaTiming,
    sample_rate: u32,
) -> Result<ExactMidi, SessionError> {
    convert_midi_with_cancel(input, timing, sample_rate, || false)
}

pub fn convert_midi_with_cancel(
    input: &ExactMidi,
    timing: MediaTiming,
    sample_rate: u32,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ExactMidi, SessionError> {
    if sample_rate == 0 {
        return Err(invalid("target sample rate must be positive"));
    }
    let length_frames = timing.length(input.length_frames)?;
    let mut converted = input.clone();
    converted.sample_rate = sample_rate;
    converted.length_frames = length_frames;
    for (index, event) in converted.events.iter_mut().enumerate() {
        if index % 4096 == 0 && cancelled() {
            return Err(invalid("MIDI conversion cancelled"));
        }
        event.frame = u64::try_from(timing.frame(i128::from(event.frame))?)
            .map_err(|_| invalid("converted MIDI timestamp overflow"))?;
        if event.frame >= length_frames {
            return Err(invalid("converted MIDI event outside media"));
        }
    }
    let mut start = 0;
    while start < converted.events.len() {
        let frame = converted.events[start].frame;
        let end = start + converted.events[start..].partition_point(|event| event.frame == frame);
        let group = &mut converted.events[start..end];
        if !timing.scale.is_identity() && group.windows(2).any(|pair| pair[0].order > pair[1].order)
        {
            for (index, event) in group.iter_mut().enumerate() {
                event.order =
                    u32::try_from(index).map_err(|_| invalid("MIDI ordering overflow"))?;
            }
        }
        start = end;
    }
    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExactMidiEvent;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn identity_keeps_sample_bits_and_exact_midi_order() {
        let samples = [f32::from_bits(0x7fc01234), -0.0, 0.125];
        let timing = MediaTiming::new(FrameScale::IDENTITY, -17).unwrap();
        assert_eq!(
            convert_audio(&samples, timing, false, 3)
                .unwrap()
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>(),
            samples
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>()
        );
        let midi = ExactMidi {
            sample_rate: 48_000,
            length_frames: 3,
            start_state: vec![vec![0xc0, 5]],
            events: vec![
                ExactMidiEvent {
                    frame: 1,
                    order: 9,
                    data: vec![0xf0, 1, 2, 0xf7],
                },
                ExactMidiEvent {
                    frame: 1,
                    order: 2,
                    data: vec![0x90, 60, 90],
                },
            ],
        };
        assert_eq!(convert_midi(&midi, timing, 48_000).unwrap(), midi);
    }

    #[shoop_test]
    fn signed_origins_anchor_preroll_boundaries_and_raw_tail() {
        for origin in [-17, -1, 0, 1, 17] {
            for (numerator, denominator) in [(3, 2), (2, 3), (48_032, 48_016)] {
                let timing =
                    MediaTiming::new(FrameScale::new(numerator, denominator).unwrap(), origin)
                        .unwrap();
                assert_eq!(
                    timing.frame(i128::from(origin)).unwrap(),
                    i128::from(timing.target_origin)
                );
                assert!(
                    timing.frame(i128::from(origin) - 1).unwrap()
                        < i128::from(timing.target_origin)
                );
                let end = timing.length(100).unwrap();
                for frame in 0..100 {
                    let mapped = timing.frame(frame).unwrap();
                    assert!(mapped >= 0 && mapped < i128::from(end));
                }
            }
        }
        let timing = MediaTiming::new(FrameScale::new(2, 3).unwrap(), 2).unwrap();
        let midi = ExactMidi {
            sample_rate: 48_000,
            length_frames: 5,
            start_state: vec![vec![0xb0, 64, 127]],
            events: (0..5)
                .map(|frame| ExactMidiEvent {
                    frame,
                    order: (5 - frame) as u32,
                    data: vec![0x90, frame as u8, 99],
                })
                .collect(),
        };
        let output = convert_midi(&midi, timing, 32_000).unwrap();
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.frame)
                .collect::<Vec<_>>(),
            [0, 1, 2, 2, 3]
        );
        assert_eq!(
            output
                .events
                .iter()
                .map(|event| event.order)
                .collect::<Vec<_>>(),
            [5, 4, 0, 1, 1]
        );
        assert_eq!(output.start_state, midi.start_state);
    }

    #[shoop_test]
    fn periodic_constant_stereo_phase_and_impulse_position_have_no_filter_delay() {
        let timing = MediaTiming::new(FrameScale::new(1008, 1000).unwrap(), 0).unwrap();
        let constant = convert_audio(&vec![0.75; 1000], timing, true, 1008).unwrap();
        assert!(constant
            .iter()
            .all(|sample| (*sample - 0.75).abs() < 1.0e-6));
        let mut impulse = vec![0.0; 1000];
        impulse[250] = 1.0;
        let output = convert_audio(&impulse, timing, false, 1008).unwrap();
        let peak = output
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .unwrap()
            .0;
        assert_eq!(peak, 252);
        let inverted = impulse.iter().map(|sample| -*sample).collect::<Vec<_>>();
        let right = convert_audio(&inverted, timing, false, 1008).unwrap();
        assert!(output
            .iter()
            .zip(right)
            .all(|(left, right)| (*left + right).abs() < 1.0e-7));
    }

    #[shoop_test]
    fn fractional_origin_and_incremental_steps_preserve_phase_and_limits() {
        let timing = MediaTiming::new(FrameScale::new(3, 2).unwrap(), 1).unwrap();
        assert_eq!(timing.length(0).unwrap(), 0);
        let input = (0..96)
            .map(|index| (index as f32 * 0.15).sin())
            .collect::<Vec<_>>();
        let expected = convert_audio(&input, timing, false, 200).unwrap();
        let mut conversion = AudioConversion::new(&input, timing, false, 200).unwrap();
        assert!(!conversion.step(0).unwrap());
        while !conversion.step(7).unwrap() {}
        assert_eq!(conversion.finish().unwrap(), expected);
        assert!(AudioConversion::new(&input, timing, false, 10).is_err());
        assert!(AudioConversion::new(&input, timing, false, 200)
            .unwrap()
            .finish()
            .is_err());
        assert_eq!(timing.source(2).unwrap(), (1, 0.0));
        let interior = &expected[35..100];
        let error = interior
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                let source = 1.0 + ((index + 35) as f64 - 2.0) / 1.5;
                (f64::from(*sample) - (source * 0.15).sin()).abs()
            })
            .fold(0.0_f64, f64::max);
        assert!(error < 0.0001, "sine reconstruction error {error}");
    }
}
