//! Optional numeric diagnostics on the same production path, with Q824 arithmetic.
use crate::{
    production::{self, OracleSample},
    spec,
};
use rt_audio_math::{Sample, Q824};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub products: u64,
    pub mixes: u64,
    pub subtractions: u64,
    pub negations: u64,
    pub clipped_products: u64,
    pub clipped_mixes: u64,
    pub clipped_subtractions: u64,
    pub clipped_negations: u64,
    pub first_clip_operation: Option<u64>,
}
impl Counts {
    fn record(mut self, kind: u8, clipped: bool) -> Self {
        let ordinal = self.products + self.mixes + self.subtractions + self.negations;
        match kind {
            0 => {
                self.products += 1;
                self.clipped_products += u64::from(clipped);
            }
            1 => {
                self.mixes += 1;
                self.clipped_mixes += u64::from(clipped);
            }
            2 => {
                self.subtractions += 1;
                self.clipped_subtractions += u64::from(clipped);
            }
            _ => {
                self.negations += 1;
                self.clipped_negations += u64::from(clipped);
            }
        }
        if clipped && self.first_clip_operation.is_none() {
            self.first_clip_operation = Some(ordinal);
        }
        self
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Quantum {
    pub frame: u64,
    pub counts: Counts,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub format_version: u32,
    pub result: spec::Result,
    pub quanta: Vec<Quantum>,
}
thread_local! {
    static COUNTS:Cell<Counts> = Cell::new(Counts::default());
    static FRAME:Cell<u64> = const { Cell::new(0) };
    static ROWS:RefCell<Vec<Quantum>> = const { RefCell::new(Vec::new()) };
}
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Audited(Q824);
fn outside(value: i64) -> bool {
    value < i64::from(i32::MIN) || value > i64::from(i32::MAX)
}
fn record(kind: u8, clipped: bool) {
    COUNTS.with(|c| c.set(c.get().record(kind, clipped)));
}
impl Sample for Audited {
    const ZERO: Self = Self(Q824::ZERO);
    const ONE: Self = Self(Q824::ONE);
    fn mix(self, other: Self) -> Self {
        record(1, outside(i64::from(self.0 .0) + i64::from(other.0 .0)));
        Self(self.0.mix(other.0))
    }
    fn multiply(self, other: Self) -> Self {
        record(
            0,
            outside(i64::from(self.0 .0) * i64::from(other.0 .0) / i64::from(Q824::ONE.0)),
        );
        Self(self.0.multiply(other.0))
    }
    fn crossfade(self, target: Self, weight: Self) -> Self {
        let difference = i64::from(target.0 .0) - i64::from(self.0 .0);
        let value =
            i64::from(self.0 .0) + difference * i64::from(weight.0 .0) / i64::from(Q824::ONE.0);
        record(2, false);
        record(0, false);
        record(1, outside(value));
        Self(self.0.crossfade(target.0, weight.0))
    }
    fn from_f32(v: f32) -> Self {
        Self(Q824::from_f32(v))
    }
    fn to_f32(self) -> f32 {
        self.0.to_f32()
    }
    fn from_bits(v: u32) -> Self {
        Self(Q824::from_bits(v))
    }
    fn to_bits(self) -> u32 {
        self.0.to_bits()
    }
    fn interpolate(start: Self, target: Self, duration: u32, elapsed: u32) -> Self {
        Self(Q824::interpolate(start.0, target.0, duration, elapsed))
    }
    fn fill_ramp(start: Self, target: Self, duration: u32, elapsed: u32, output: &mut [Self]) {
        for (i, value) in output.iter_mut().enumerate() {
            *value = Self::interpolate(start, target, duration, elapsed.saturating_add(i as u32));
        }
    }
    fn ratio(n: i64, d: u64) -> Self {
        Self(Q824::ratio(n, d))
    }
    fn divide(self, d: u32) -> Self {
        Self(self.0.divide(d))
    }
    fn is_finite(self) -> bool {
        true
    }
    fn min(self, o: Self) -> Self {
        Self(core::cmp::min(self.0, o.0))
    }
    fn max(self, o: Self) -> Self {
        Self(core::cmp::max(self.0, o.0))
    }
    fn peak_key(self) -> u32 {
        self.0.peak_key()
    }
}
impl Add for Audited {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        self.mix(o)
    }
}
impl AddAssign for Audited {
    fn add_assign(&mut self, o: Self) {
        *self = self.mix(o);
    }
}
impl Mul for Audited {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        self.multiply(o)
    }
}
impl MulAssign for Audited {
    fn mul_assign(&mut self, o: Self) {
        *self = self.multiply(o);
    }
}
impl Sub for Audited {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        record(2, outside(i64::from(self.0 .0) - i64::from(o.0 .0)));
        Self(self.0 - o.0)
    }
}
impl Neg for Audited {
    type Output = Self;
    fn neg(self) -> Self {
        record(3, self.0 .0 == i32::MIN);
        Self(-self.0)
    }
}
impl OracleSample for Audited {
    const NUMERIC: spec::Numeric = spec::Numeric::Q824;
    fn from_raw(value: i32) -> Self {
        Self(Q824(value))
    }
    fn samples(channels: Vec<Vec<Self>>) -> spec::Samples {
        spec::Samples::Q824(
            channels
                .into_iter()
                .map(|c| c.into_iter().map(|s| s.0 .0).collect())
                .collect(),
        )
    }
    fn begin_quantum(frame: u64) {
        COUNTS.with(|c| c.set(Counts::default()));
        FRAME.with(|f| f.set(frame));
    }
    fn end_quantum() {
        ROWS.with_borrow_mut(|r| {
            r.push(Quantum {
                frame: FRAME.get(),
                counts: COUNTS.get(),
            })
        });
    }
}
pub fn run(scenario: &spec::Scenario) -> crate::validation::Checked<Report> {
    ROWS.with_borrow_mut(Vec::clear);
    let result = production::run::<Audited>(scenario)?;
    let quanta = ROWS.with_borrow(|r| r.clone());
    Ok(Report {
        format_version: spec::FORMAT_VERSION,
        result,
        quanta,
    })
}
