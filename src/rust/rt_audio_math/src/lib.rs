//! Static sample arithmetic. Integer samples retain their raw representation.
#![no_std]
#![forbid(unsafe_code)]

use core::fmt::Debug;
use core::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub};

pub trait Sample:
    Copy
    + Default
    + Debug
    + PartialEq
    + PartialOrd
    + Send
    + Sync
    + 'static
    + Add<Output = Self>
    + AddAssign
    + Sub<Output = Self>
    + Mul<Output = Self>
    + MulAssign
    + Neg<Output = Self>
{
    const ZERO: Self;
    const ONE: Self;
    fn mix(self, other: Self) -> Self;
    fn multiply(self, gain: Self) -> Self;
    /// Blend using a weight in [ZERO, ONE], without clipping the difference.
    fn crossfade(self, target: Self, weight: Self) -> Self;
    fn from_f32(value: f32) -> Self;
    fn to_f32(self) -> f32;
    fn from_bits(bits: u32) -> Self;
    fn to_bits(self) -> u32;
    fn interpolate(start: Self, target: Self, duration: u32, elapsed: u32) -> Self;
    fn fill_ramp(start: Self, target: Self, duration: u32, elapsed: u32, output: &mut [Self]);
    fn ratio(numerator: i64, denominator: u64) -> Self;
    fn divide(self, denominator: u32) -> Self;
    fn is_finite(self) -> bool;
    fn min(self, other: Self) -> Self;
    fn max(self, other: Self) -> Self;
    fn peak_key(self) -> u32;

    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self::from_bits(u32::from_le_bytes(bytes))
    }
    fn to_le_bytes(self) -> [u8; 4] {
        self.to_bits().to_le_bytes()
    }
}

impl Sample for f32 {
    const ZERO: Self = 0.;
    const ONE: Self = 1.;
    fn ratio(numerator: i64, denominator: u64) -> Self {
        numerator as f32 / denominator as f32
    }
    fn divide(self, denominator: u32) -> Self {
        self / denominator as f32
    }
    fn is_finite(self) -> bool {
        self.is_finite()
    }
    fn min(self, other: Self) -> Self {
        self.min(other)
    }
    fn max(self, other: Self) -> Self {
        self.max(other)
    }
    fn peak_key(self) -> u32 {
        self.to_bits() & 0x7fff_ffff
    }
    #[inline]
    fn mix(self, other: Self) -> Self {
        self + other
    }
    #[inline]
    fn multiply(self, gain: Self) -> Self {
        self * gain
    }
    fn crossfade(self, target: Self, weight: Self) -> Self {
        (f64::from(self) + (f64::from(target) - f64::from(self)) * f64::from(weight)) as f32
    }
    fn from_f32(value: f32) -> Self {
        value
    }
    fn to_f32(self) -> f32 {
        self
    }
    fn from_bits(bits: u32) -> Self {
        f32::from_bits(bits)
    }
    fn to_bits(self) -> u32 {
        self.to_bits()
    }
    fn interpolate(start: Self, target: Self, duration: u32, elapsed: u32) -> Self {
        if duration == 0 || elapsed >= duration {
            target
        } else {
            start + ((target - start) / duration as f32) * elapsed as f32
        }
    }
    fn fill_ramp(start: Self, target: Self, duration: u32, elapsed: u32, output: &mut [Self]) {
        let active = output.len().min(duration.saturating_sub(elapsed) as usize);
        if active != 0 {
            let step = (target - start) / duration as f32;
            for (index, sample) in output[..active].iter_mut().enumerate() {
                *sample = start + step * (u64::from(elapsed) + index as u64) as f32;
            }
        }
        output[active..].fill(target);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct Q824(pub i32);

impl Q824 {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1 << 24);

    fn from_wide(value: i64) -> Self {
        Self(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }
}

impl Sample for Q824 {
    const ZERO: Self = Self::ZERO;
    const ONE: Self = Self::ONE;
    fn ratio(numerator: i64, denominator: u64) -> Self {
        let value = i128::from(numerator) * i128::from(Self::ONE.0) / i128::from(denominator);
        Self(value.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32)
    }
    fn divide(self, denominator: u32) -> Self {
        Self((i64::from(self.0) / i64::from(denominator)) as i32)
    }
    fn is_finite(self) -> bool {
        true
    }
    fn min(self, other: Self) -> Self {
        core::cmp::min(self, other)
    }
    fn max(self, other: Self) -> Self {
        core::cmp::max(self, other)
    }
    fn peak_key(self) -> u32 {
        self.0.unsigned_abs().min(i32::MAX as u32)
    }
    #[inline]
    fn mix(self, other: Self) -> Self {
        Self::from_wide(i64::from(self.0) + i64::from(other.0))
    }
    #[inline]
    fn multiply(self, gain: Self) -> Self {
        Self::from_wide(i64::from(self.0) * i64::from(gain.0) / i64::from(Self::ONE.0))
    }
    fn crossfade(self, target: Self, weight: Self) -> Self {
        let difference = i64::from(target.0) - i64::from(self.0);
        Self::from_wide(
            i64::from(self.0) + difference * i64::from(weight.0) / i64::from(Self::ONE.0),
        )
    }
    fn from_f32(value: f32) -> Self {
        Self((f64::from(value) * f64::from(Self::ONE.0)) as i32)
    }
    fn to_f32(self) -> f32 {
        self.0 as f32 / Self::ONE.0 as f32
    }
    fn from_bits(bits: u32) -> Self {
        Self(bits as i32)
    }
    fn to_bits(self) -> u32 {
        self.0 as u32
    }
    fn interpolate(start: Self, target: Self, duration: u32, elapsed: u32) -> Self {
        if duration == 0 || elapsed >= duration {
            target
        } else {
            let delta = i128::from(target.0) - i128::from(start.0);
            Self((i128::from(start.0) + delta * i128::from(elapsed) / i128::from(duration)) as i32)
        }
    }
    fn fill_ramp(start: Self, target: Self, duration: u32, elapsed: u32, output: &mut [Self]) {
        let active = output.len().min(duration.saturating_sub(elapsed) as usize);
        if active != 0 {
            let delta = i64::from(target.0) - i64::from(start.0);
            let magnitude = delta.unsigned_abs();
            let denominator = u64::from(duration);
            let initial = magnitude * u64::from(elapsed);
            let mut change = initial / denominator;
            let mut fraction = initial % denominator;
            let step = magnitude / denominator;
            let remainder = magnitude % denominator;
            for sample in &mut output[..active] {
                *sample = Self(
                    (i64::from(start.0)
                        + if delta < 0 {
                            -(change as i64)
                        } else {
                            change as i64
                        }) as i32,
                );
                change += step;
                fraction += remainder;
                if fraction >= denominator {
                    fraction -= denominator;
                    change += 1;
                }
            }
        }
        output[active..].fill(target);
    }
}

impl Add for Q824 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        self.mix(other)
    }
}
impl AddAssign for Q824 {
    fn add_assign(&mut self, other: Self) {
        *self = self.mix(other);
    }
}
impl Sub for Q824 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::from_wide(i64::from(self.0) - i64::from(other.0))
    }
}
impl Mul for Q824 {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        self.multiply(other)
    }
}
impl MulAssign for Q824 {
    fn mul_assign(&mut self, other: Self) {
        *self = self.multiply(other);
    }
}
impl Neg for Q824 {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp<S: Sample = f32> {
    pub start: S,
    pub target: S,
    pub duration: u32,
    pub elapsed: u32,
}

impl<S: Sample> Ramp<S> {
    pub fn new(start: S, target: S, duration: u32) -> Self {
        Self {
            start,
            target,
            duration,
            elapsed: 0,
        }
    }
    pub fn constant(value: S) -> Self {
        Self::new(value, value, 0)
    }
    pub fn at(&self, offset: u32) -> S {
        S::interpolate(
            self.start,
            self.target,
            self.duration,
            self.elapsed.saturating_add(offset),
        )
    }
    pub fn advance(&mut self, frames: u32) {
        self.elapsed = self.elapsed.saturating_add(frames).min(self.duration);
    }
    pub fn set(&mut self, target: S, duration: u32) {
        *self = Self::new(self.at(0), target, duration);
    }
    pub fn fill(&mut self, output: &mut [S]) {
        S::fill_ramp(self.start, self.target, self.duration, self.elapsed, output);
        self.advance(u32::try_from(output.len()).unwrap_or(u32::MAX));
    }
}

#[inline]
pub fn mix<S: Sample>(source: &[S], target: &mut [S]) {
    for (&source, target) in source.iter().zip(target) {
        *target = target.mix(source);
    }
}

#[inline]
pub fn gain<S: Sample>(source: &[S], target: &mut [S], value: S) {
    for (&source, target) in source.iter().zip(target) {
        *target = source.multiply(value);
    }
}

#[inline]
pub fn gain_each<S: Sample>(source: &[S], target: &mut [S], gains: &[S]) {
    for ((&source, target), &gain) in source.iter().zip(target).zip(gains) {
        *target = source.multiply(gain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn crossfades_keep_full_scale_differences_and_round_once() {
        let low = Q824(i32::MIN);
        let high = Q824(i32::MAX);
        let half = Q824(Q824::ONE.0 / 2);
        assert_eq!(low.crossfade(high, Q824::ZERO), low);
        assert_eq!(low.crossfade(high, Q824::ONE), high);
        assert_eq!(low.crossfade(high, half), Q824(-1));
        assert_eq!(high.crossfade(low, half), Q824(0));
        assert_eq!(Q824(1).crossfade(Q824(-1), half), Q824(0));
        assert_eq!(Q824(-1).crossfade(Q824(1), half), Q824(0));
        assert_eq!((-f32::MAX).crossfade(f32::MAX, 0.5), 0.);
        assert_eq!(1f32.crossfade(-1., 0.5), 0.);
    }

    #[shoop_test]
    fn fixed_arithmetic_keeps_sign_and_each_saturation_point() {
        assert_eq!(Q824(-1).multiply(Q824(1)), Q824(0));
        assert_eq!(Q824(-3).multiply(Q824(1 << 23)), Q824(-1));
        assert_eq!(Q824(i32::MIN).multiply(Q824(-1 << 24)), Q824(i32::MAX));
        let a = Q824(100 << 24);
        assert_ne!(a.mix(a).mix(Q824(-a.0)), a.mix(a.mix(Q824(-a.0))));
        for raw in [i32::MIN, -16_777_217, -1, 0, 1, 16_777_217, i32::MAX] {
            assert_eq!(Q824::from_le_bytes(Q824(raw).to_le_bytes()).0, raw);
        }
    }

    #[shoop_test]
    fn stepped_ramps_match_wide_specification_at_every_sample() {
        for start in [i32::MIN, -17, 0, 33, i32::MAX] {
            for target in [i32::MIN, -9, 0, 18, i32::MAX] {
                for duration in [0, 1, 2, 7, 31, 32, 33, 129, u32::MAX] {
                    for elapsed in [0, 1, 6, 31, 32, duration.saturating_sub(1), u32::MAX] {
                        let mut ramp = Ramp {
                            start: Q824(start),
                            target: Q824(target),
                            duration,
                            elapsed,
                        };
                        let original = ramp;
                        let mut output = [Q824::ZERO; 67];
                        ramp.fill(&mut output);
                        for (index, actual) in output.into_iter().enumerate() {
                            let at = elapsed.saturating_add(index as u32).min(duration);
                            let expected = if duration == 0 {
                                i128::from(target)
                            } else {
                                i128::from(start)
                                    + (i128::from(target) - i128::from(start)) * i128::from(at)
                                        / i128::from(duration)
                            };
                            assert_eq!(actual.0, expected as i32, "{original:?} at {index}");
                        }
                    }
                }
            }
        }
    }

    #[shoop_test]
    fn ramps_are_partition_invariant_and_interrupt_at_the_boundary_value() {
        let mut whole = Ramp::new(Q824(-17), Q824(49), 41);
        let mut split = whole;
        let mut a = [Q824::ZERO; 65];
        let mut b = a;
        whole.fill(&mut a);
        split.fill(&mut b[..13]);
        let boundary = split.at(0);
        split.fill(&mut b[13..32]);
        split.fill(&mut b[32..]);
        assert_eq!(a, b);
        assert_eq!(a[0], Q824(-17));
        assert_eq!(a[41], Q824(49));
        let mut interrupted = Ramp::new(Q824(-17), Q824(49), 41);
        interrupted.advance(13);
        interrupted.set(Q824(-100), 8);
        assert_eq!(interrupted.at(0), boundary);
        assert_eq!(interrupted.at(8), Q824(-100));
        let mut floats = Ramp::new(0f32, 1., 4);
        let mut values = [0.; 8];
        floats.fill(&mut values);
        assert_eq!(values, [0., 0.25, 0.5, 0.75, 1., 1., 1., 1.]);
    }
}
