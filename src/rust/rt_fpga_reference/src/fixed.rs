//! Provisional Q8.24 arithmetic: signed 32-bit values, saturation, truncation toward zero.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Fixed(pub i32);

impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1 << 24);

    /// Boundary conversion only; processing uses integer arithmetic.
    pub fn from_f32(value: f32) -> Self {
        Self((f64::from(value) * f64::from(Self::ONE.0)) as i32)
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::ONE.0 as f32
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self::from_wide(i64::from(self.0) + i64::from(other.0))
    }

    pub fn multiply(self, other: Self) -> Self {
        Self::from_wide(i64::from(self.0) * i64::from(other.0) / i64::from(Self::ONE.0))
    }

    fn from_wide(value: i64) -> Self {
        Self(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }
}

/// The first affected sample has `start`; sample `duration` and later have `target`.
/// Preserve the original domain across quantum boundaries and interrupted ramps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Ramp {
    pub start: Fixed,
    pub target: Fixed,
    pub duration: u32,
    pub elapsed: u32,
}

impl Ramp {
    pub fn new(start: Fixed, target: Fixed, duration: u32) -> Self {
        Self {
            start,
            target,
            duration,
            elapsed: 0,
        }
    }

    pub fn constant(value: Fixed) -> Self {
        Self::new(value, value, 0)
    }

    pub fn at(self, offset: u32) -> Fixed {
        if self.duration == 0 {
            return self.target;
        }
        let elapsed = self.elapsed.saturating_add(offset).min(self.duration);
        let delta = i64::from(self.target.0) - i64::from(self.start.0);
        // i128 also covers the extreme i32 endpoints over a u32 duration.
        let change = i128::from(delta) * i128::from(elapsed) / i128::from(self.duration);
        Fixed((i128::from(self.start.0) + change) as i32)
    }

    pub fn advance(&mut self, frames: u32) {
        self.elapsed = self.elapsed.saturating_add(frames).min(self.duration);
    }
}
