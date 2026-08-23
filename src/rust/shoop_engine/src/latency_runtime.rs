use shoop_latency::{LatencyCertainty, LatencyDomainError, LatencyRangeFrames};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

/// Bounded, allocation-free latency value used by callback-facing engine surfaces.
///
/// Source and interval identities remain in control-path policy snapshots; this value is the
/// coherently published numeric portion that realtime code needs to detect revisions and latch
/// recipes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeLatencyObservation {
    pub range: Option<LatencyRangeFrames>,
    pub certainty: LatencyCertainty,
    pub sample_rate: u32,
    pub revision: u64,
}

impl RuntimeLatencyObservation {
    pub fn new(
        range: Option<LatencyRangeFrames>,
        certainty: LatencyCertainty,
        sample_rate: u32,
        revision: u64,
    ) -> Result<Self, LatencyDomainError> {
        match (certainty, range) {
            (LatencyCertainty::Exact, Some(range)) if range.min() == range.max() => {}
            (LatencyCertainty::Range, Some(range)) if range.min() < range.max() => {}
            (LatencyCertainty::Estimated, Some(_)) => {}
            (LatencyCertainty::ManualOnly | LatencyCertainty::Unknown, None) => {}
            _ => return Err(LatencyDomainError::CertaintyRangeMismatch),
        }
        if range.is_some() && sample_rate == 0 {
            return Err(LatencyDomainError::ZeroSampleRate);
        }
        Ok(Self {
            range,
            certainty,
            sample_rate,
            revision,
        })
    }

    pub fn exact(frames: u32, sample_rate: u32, revision: u64) -> Result<Self, LatencyDomainError> {
        Self::new(
            Some(LatencyRangeFrames::new(frames, frames)?),
            LatencyCertainty::Exact,
            sample_rate,
            revision,
        )
    }

    pub const fn unknown(sample_rate: u32, revision: u64) -> Self {
        Self {
            range: None,
            certainty: LatencyCertainty::Unknown,
            sample_rate,
            revision,
        }
    }
}

impl Default for RuntimeLatencyObservation {
    fn default() -> Self {
        Self::unknown(0, 0)
    }
}

/// Seqlock publication for a complete runtime latency observation.
///
/// There is one writer per provider/control surface. Callback readers retry if they overlap the
/// short publication window and never take a lock or allocate.
#[derive(Debug)]
pub struct AtomicLatencyObservation {
    generation: AtomicU64,
    minimum: AtomicU32,
    maximum: AtomicU32,
    certainty: AtomicU8,
    sample_rate: AtomicU32,
    revision: AtomicU64,
}

impl AtomicLatencyObservation {
    pub fn new(observation: RuntimeLatencyObservation) -> Self {
        let range = observation.range;
        Self {
            generation: AtomicU64::new(0),
            minimum: AtomicU32::new(range.map(LatencyRangeFrames::min).unwrap_or(0)),
            maximum: AtomicU32::new(range.map(LatencyRangeFrames::max).unwrap_or(0)),
            certainty: AtomicU8::new(certainty_to_u8(observation.certainty)),
            sample_rate: AtomicU32::new(observation.sample_rate),
            revision: AtomicU64::new(observation.revision),
        }
    }

    pub fn publish(&self, observation: RuntimeLatencyObservation) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let range = observation.range;
        self.minimum.store(
            range.map(LatencyRangeFrames::min).unwrap_or(0),
            Ordering::Relaxed,
        );
        self.maximum.store(
            range.map(LatencyRangeFrames::max).unwrap_or(0),
            Ordering::Relaxed,
        );
        self.sample_rate
            .store(observation.sample_rate, Ordering::Relaxed);
        self.certainty
            .store(certainty_to_u8(observation.certainty), Ordering::Relaxed);
        self.revision.store(observation.revision, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn read(&self) -> RuntimeLatencyObservation {
        loop {
            let before = self.generation.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let certainty = certainty_from_u8(self.certainty.load(Ordering::Relaxed));
            let sample_rate = self.sample_rate.load(Ordering::Relaxed);
            let minimum = self.minimum.load(Ordering::Relaxed);
            let maximum = self.maximum.load(Ordering::Relaxed);
            let revision = self.revision.load(Ordering::Relaxed);
            let after = self.generation.load(Ordering::Acquire);
            if before != after {
                std::hint::spin_loop();
                continue;
            }
            let range = matches!(
                certainty,
                LatencyCertainty::Exact | LatencyCertainty::Range | LatencyCertainty::Estimated
            )
            .then(|| {
                LatencyRangeFrames::new(minimum, maximum)
                    .expect("published runtime latency is validated")
            });
            return RuntimeLatencyObservation {
                range,
                certainty,
                sample_rate,
                revision,
            };
        }
    }
}

impl Default for AtomicLatencyObservation {
    fn default() -> Self {
        Self::new(RuntimeLatencyObservation::default())
    }
}

const fn certainty_to_u8(certainty: LatencyCertainty) -> u8 {
    match certainty {
        LatencyCertainty::Exact => 0,
        LatencyCertainty::Range => 1,
        LatencyCertainty::Estimated => 2,
        LatencyCertainty::ManualOnly => 3,
        LatencyCertainty::Unknown => 4,
    }
}

const fn certainty_from_u8(certainty: u8) -> LatencyCertainty {
    match certainty {
        0 => LatencyCertainty::Exact,
        1 => LatencyCertainty::Range,
        2 => LatencyCertainty::Estimated,
        3 => LatencyCertainty::ManualOnly,
        _ => LatencyCertainty::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn publication_preserves_complete_observations_without_allocation() {
        let observation = RuntimeLatencyObservation::new(
            Some(LatencyRangeFrames::new(7, 11).unwrap()),
            LatencyCertainty::Range,
            48_000,
            9,
        )
        .unwrap();
        let atomic = AtomicLatencyObservation::default();
        assert_no_alloc::assert_no_alloc(|| {
            atomic.publish(observation);
            assert_eq!(atomic.read(), observation);
        });
    }
}
