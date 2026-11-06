//! Exact off-thread timing plans for conversion into a prepared engine format.
use crate::Error as SessionError;
pub use shoop_app_api::ConversionTarget;
use std::collections::{BTreeMap, BTreeSet};

mod composite;
mod media;
mod midi;
pub use composite::{
    CompositeEffectiveInstanceDocument, CompositeSpanDocument, CompositeTimingDocument,
};
pub use midi::{ExactMidi, ExactMidiEvent};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("conversion is invalid: {0}")]
    Validation(String),
    #[error("conversion exceeds resource limits: {0}")]
    ResourceLimit(String),
}
pub use media::{
    convert_audio, convert_midi, convert_midi_with_cancel, AudioConversion, MediaTiming,
};

fn invalid(message: &str) -> SessionError {
    SessionError::Validation(message.to_owned())
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FrameScale {
    numerator: u128,
    denominator: u128,
}

impl FrameScale {
    pub fn numerator(self) -> u128 {
        self.numerator
    }
    pub fn denominator(self) -> u128 {
        self.denominator
    }
    pub const IDENTITY: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    pub fn new(numerator: u128, denominator: u128) -> Result<Self, SessionError> {
        if denominator == 0 {
            return Err(invalid("zero frame-scale denominator"));
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    pub fn multiply(self, other: Self) -> Result<Self, SessionError> {
        let left = gcd(self.numerator, other.denominator);
        let right = gcd(other.numerator, self.denominator);
        Self::new(
            (self.numerator / left)
                .checked_mul(other.numerator / right)
                .ok_or_else(|| invalid("frame-scale overflow"))?,
            (self.denominator / right)
                .checked_mul(other.denominator / left)
                .ok_or_else(|| invalid("frame-scale overflow"))?,
        )
    }

    pub fn is_identity(self) -> bool {
        self == Self::IDENTITY
    }

    pub fn ratio(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    pub fn ceil(self, value: u64) -> Result<u64, SessionError> {
        let value = self
            .numerator
            .checked_mul(u128::from(value))
            .ok_or_else(|| invalid("frame duration overflow"))?;
        let rounded = value / self.denominator + u128::from(value % self.denominator != 0);
        u64::try_from(rounded).map_err(|_| invalid("frame duration overflow"))
    }

    pub fn signed_floor(self, value: i128) -> Result<i128, SessionError> {
        let numerator =
            i128::try_from(self.numerator).map_err(|_| invalid("signed frame-scale overflow"))?;
        let denominator =
            i128::try_from(self.denominator).map_err(|_| invalid("signed frame-scale overflow"))?;
        value
            .checked_mul(numerator)
            .map(|value| value.div_euclid(denominator))
            .ok_or_else(|| invalid("signed frame overflow"))
    }

    pub fn signed_ceil(self, value: i128) -> Result<i128, SessionError> {
        self.signed_floor(
            value
                .checked_neg()
                .ok_or_else(|| invalid("signed frame overflow"))?,
        )?
        .checked_neg()
        .ok_or_else(|| invalid("signed frame overflow"))
    }

    pub fn nearest_quantum(self, quantum: u32) -> Result<u64, SessionError> {
        if self.numerator == 0 {
            return Ok(0);
        }
        let divisor = self
            .denominator
            .checked_mul(u128::from(quantum))
            .filter(|value| *value > 0)
            .ok_or_else(|| invalid("quantum alignment overflow"))?;
        let whole = self.numerator / divisor;
        let remainder = self.numerator % divisor;
        let upward = remainder >= divisor / 2 + divisor % 2;
        let ticks = (whole + u128::from(upward)).max(1);
        u64::try_from(
            ticks
                .checked_mul(u128::from(quantum))
                .ok_or_else(|| invalid("aligned duration overflow"))?,
        )
        .map_err(|_| invalid("aligned duration overflow"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoopTiming {
    pub id: u64,
    pub sync_source: Option<u64>,
    pub frames: u64,
    pub sample_rate: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoopConversion {
    pub id: u64,
    pub source_frames: u64,
    pub frames: u64,
    pub scale: FrameScale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversionPlan {
    pub target: ConversionTarget,
    /// Sources precede their dependents, with identity ordering among ready loops.
    pub loops: Vec<LoopConversion>,
    pub requires_confirmation: bool,
}

impl ConversionPlan {
    pub fn new(target: ConversionTarget, loops: &[LoopTiming]) -> Result<Self, SessionError> {
        if !target.is_valid() {
            return Err(invalid("invalid engine conversion target"));
        }
        let mut pending = BTreeMap::new();
        for timing in loops {
            if timing.id == 0
                || timing.sample_rate == 0
                || pending.insert(timing.id, *timing).is_some()
            {
                return Err(invalid("invalid or duplicate conversion loop"));
            }
        }
        let identities: BTreeSet<_> = pending.keys().copied().collect();
        if pending.values().any(|loop_| {
            loop_
                .sync_source
                .is_some_and(|id| !identities.contains(&id))
        }) {
            return Err(invalid("conversion sync source is missing"));
        }
        let ideals = pending
            .iter()
            .map(|(&id, timing)| {
                FrameScale::new(
                    u128::from(timing.frames) * u128::from(target.sample_rate),
                    u128::from(timing.sample_rate),
                )
                .map(|ideal| (id, ideal))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut result = Self {
            target,
            loops: Vec::with_capacity(loops.len()),
            requires_confirmation: false,
        };
        let mut complete = BTreeMap::<u64, LoopConversion>::new();
        while !pending.is_empty() {
            let ready = pending
                .iter()
                .find(|(&id, timing)| {
                    timing
                        .sync_source
                        .is_none_or(|source| source == id || complete.contains_key(&source))
                })
                .map(|(&id, _)| id)
                .ok_or_else(|| invalid("conversion sync dependency cycle"))?;
            let timing = pending.remove(&ready).expect("ready loop");
            let mut ideal = ideals[&ready];
            if let Some(source) = timing.sync_source.filter(|source| *source != ready) {
                let original = ideals[&source];
                if original.numerator != 0 {
                    let correction = FrameScale::new(u128::from(complete[&source].frames), 1)?
                        .multiply(FrameScale::new(original.denominator, original.numerator)?)?;
                    ideal = ideal.multiply(correction)?;
                }
            }
            let frames = ideal.nearest_quantum(target.quantum_frames)?;
            let scale = if timing.frames == 0 {
                FrameScale::new(
                    u128::from(target.sample_rate),
                    u128::from(timing.sample_rate),
                )?
            } else {
                FrameScale::new(u128::from(frames), u128::from(timing.frames))?
            };
            let converted = LoopConversion {
                id: ready,
                source_frames: timing.frames,
                frames,
                scale,
            };
            result.requires_confirmation |=
                timing.sample_rate != target.sample_rate || !scale.is_identity();
            complete.insert(ready, converted);
            result.loops.push(converted);
        }
        Ok(result)
    }

    pub fn get(&self, id: u64) -> Option<&LoopConversion> {
        self.loops.iter().find(|loop_| loop_.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn target(quantum_frames: u32) -> ConversionTarget {
        ConversionTarget {
            sample_rate: 48_000,
            quantum_frames,
        }
    }
    fn timing(id: u64, source: Option<u64>, frames: u64) -> LoopTiming {
        LoopTiming {
            id,
            sync_source: source,
            frames,
            sample_rate: 48_000,
        }
    }

    #[shoop_test]
    fn sync_chain_uses_original_ratios_in_dependency_order() {
        let plan = ConversionPlan::new(
            target(32),
            &[
                timing(1, Some(2), 576_192),
                timing(3, Some(3), 48_016),
                timing(2, Some(3), 192_064),
            ],
        )
        .unwrap();
        assert_eq!(
            plan.loops
                .iter()
                .map(|loop_| (loop_.id, loop_.frames))
                .collect::<Vec<_>>(),
            [(3, 48_032), (2, 192_128), (1, 576_384)]
        );
        assert!(plan.requires_confirmation);
        assert_eq!(plan.loops[0].scale, plan.loops[2].scale);
    }

    #[shoop_test]
    fn all_quanta_round_ties_up_keep_empty_and_preserve_identity() {
        for quantum in [1, 16, 32, 64, 128] {
            let plan = ConversionPlan::new(
                target(quantum),
                &[
                    timing(1, None, 0),
                    timing(2, Some(1), u64::from(quantum) * 3),
                ],
            )
            .unwrap();
            assert_eq!(plan.get(1).unwrap().frames, 0);
            assert_eq!(plan.get(2).unwrap().frames, u64::from(quantum) * 3);
            assert!(!plan.requires_confirmation);
        }
        for (frames, expected) in [(1, 32), (16, 32), (47, 32), (48, 64), (80, 96)] {
            assert_eq!(
                ConversionPlan::new(target(32), &[timing(1, None, frames)])
                    .unwrap()
                    .loops[0]
                    .frames,
                expected
            );
        }
    }

    #[shoop_test]
    fn combined_rate_conversion_preserves_cycle_multiples_without_intermediate_rounding() {
        let loops = [
            LoopTiming {
                sample_rate: 44_100,
                ..timing(1, None, 44_101)
            },
            LoopTiming {
                sample_rate: 44_100,
                ..timing(2, Some(1), 176_404)
            },
            timing(3, None, 96),
        ];
        let plan = ConversionPlan::new(target(32), &loops).unwrap();
        assert_eq!(plan.get(1).unwrap().frames, 48_000);
        assert_eq!(plan.get(2).unwrap().frames, 192_000);
        assert_eq!(plan.get(3).unwrap().frames, 96);
        assert_eq!(
            plan.get(1).unwrap().scale,
            FrameScale::new(48_000, 44_101).unwrap()
        );
        let fractional =
            ConversionPlan::new(target(32), &[timing(1, None, 48), timing(2, Some(1), 72)])
                .unwrap();
        assert_eq!(fractional.get(2).unwrap().frames, 96);
    }

    #[shoop_test]
    fn invalid_targets_dependencies_and_overflow_are_refused() {
        assert!(ConversionPlan::new(target(3), &[]).is_err());
        assert!(ConversionPlan::new(target(32), &[timing(1, Some(2), 32)]).is_err());
        assert!(ConversionPlan::new(
            target(32),
            &[timing(1, Some(2), 32), timing(2, Some(1), 32)]
        )
        .is_err());
        assert!(
            ConversionPlan::new(target(32), &[timing(1, None, 32), timing(1, None, 32)]).is_err()
        );
        assert!(ConversionPlan::new(target(32), &[timing(1, None, u64::MAX)]).is_err());
        assert!(FrameScale::new(u128::MAX, 1).unwrap().ceil(2).is_err());
        assert_eq!(FrameScale::new(3, 2).unwrap().signed_floor(-1).unwrap(), -2);
        assert_eq!(FrameScale::new(3, 2).unwrap().signed_ceil(-1).unwrap(), -1);
    }
}
