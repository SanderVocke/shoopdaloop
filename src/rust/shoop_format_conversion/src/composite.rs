use crate::{invalid, ConversionTarget, Error as SessionError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct CompositeTimingDocument {
    pub basis_sample_rate: u32,
    pub basis_unit_frames: u64,
    pub sample_rate: u32,
    pub quantum_frames: u32,
    pub unit_frames: u64,
    pub iterations: u64,
    pub instances: Vec<CompositeEffectiveInstanceDocument>,
    pub spans: Vec<CompositeSpanDocument>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct CompositeEffectiveInstanceDocument {
    pub instance_id: u64,
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct CompositeSpanDocument {
    pub target: u64,
    pub start: u64,
    pub end: u64,
    pub occurrence_start: u64,
    pub ordinal: u32,
    pub mode: String,
}

impl CompositeTimingDocument {
    pub fn converted(&self, target: ConversionTarget) -> Result<Self, SessionError> {
        if !target.is_valid() || self.sample_rate == 0 || self.unit_frames == 0 {
            return Err(invalid("invalid composite format"));
        }
        let convert = |frame: u64| {
            let numerator = u128::from(frame) * u128::from(target.sample_rate);
            let denominator = u128::from(self.sample_rate) * u128::from(target.quantum_frames);
            u64::try_from(
                (numerator + denominator / 2) / denominator * u128::from(target.quantum_frames),
            )
            .map_err(|_| invalid("converted composite endpoint overflow"))
        };
        let unit = if self.sample_rate == target.sample_rate
            && self.unit_frames % u64::from(target.quantum_frames) == 0
        {
            self.unit_frames
        } else {
            u64::from(target.quantum_frames)
        };
        let mut next = self.clone();
        next.sample_rate = target.sample_rate;
        next.quantum_frames = target.quantum_frames;
        next.unit_frames = unit;
        next.iterations = convert(
            self.unit_frames
                .checked_mul(self.iterations)
                .ok_or_else(|| invalid("composite duration overflow"))?,
        )? / unit;
        for instance in &mut next.instances {
            instance.start_frame = convert(instance.start_frame)?;
            instance.end_frame = convert(instance.end_frame)?;
            if instance.start_frame >= instance.end_frame {
                return Err(invalid("quantum conversion collapses a composite event"));
            }
        }
        for span in &mut next.spans {
            span.start = convert(
                span.start
                    .checked_mul(self.unit_frames)
                    .ok_or_else(|| invalid("composite span overflow"))?,
            )? / unit;
            span.end = convert(
                span.end
                    .checked_mul(self.unit_frames)
                    .ok_or_else(|| invalid("composite span overflow"))?,
            )? / unit;
            span.occurrence_start = span.start;
            if span.start >= span.end {
                return Err(invalid("quantum conversion collapses a composite span"));
            }
        }
        let mut normalized: Vec<CompositeSpanDocument> = Vec::new();
        for span in next.spans {
            if let Some(previous) = normalized.last_mut().filter(|previous| {
                previous.target == span.target
                    && previous.end == span.start
                    && previous.mode == span.mode
            }) {
                previous.end = span.end;
            } else {
                normalized.push(span);
            }
        }
        next.spans = normalized;
        Ok(next)
    }
}
