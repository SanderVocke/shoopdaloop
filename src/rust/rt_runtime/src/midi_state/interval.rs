//! Composable pedal effects for indexed MIDI state lookup.

use crate::Frame;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HoldState(u8);
impl HoldState {
    pub(crate) fn new(sustain: bool, hold2: bool) -> Self {
        Self(u8::from(sustain) | (u8::from(hold2) << 1))
    }
    pub(crate) fn sustain(self) -> bool {
        self.0 & 1 != 0
    }
    pub(crate) fn hold2(self) -> bool {
        self.0 & 2 != 0
    }
    pub(crate) fn active(self) -> bool {
        self.0 != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HoldUpdate {
    Sustain(bool),
    Hold2(bool),
    Reset,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HoldProjection {
    pub(crate) state: HoldState,
    /// Both pedals were up after at least one update in the interval.
    pub(crate) clear_seen: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HoldSummary([HoldProjection; 4]);
impl Default for HoldSummary {
    fn default() -> Self {
        Self(core::array::from_fn(|state| HoldProjection {
            state: HoldState(state as u8),
            clear_seen: false,
        }))
    }
}
impl HoldSummary {
    pub(crate) fn update(update: HoldUpdate) -> Self {
        Self(core::array::from_fn(|state| {
            let state = match update {
                HoldUpdate::Sustain(down) => (state as u8 & !1) | u8::from(down),
                HoldUpdate::Hold2(down) => (state as u8 & !2) | (u8::from(down) << 1),
                HoldUpdate::Reset => 0,
            };
            HoldProjection {
                state: HoldState(state),
                clear_seen: state == 0,
            }
        }))
    }
    pub(crate) fn apply(self, initial: HoldState) -> HoldProjection {
        self.0[initial.0 as usize]
    }
    pub(crate) fn then(self, later: Self) -> Self {
        Self(core::array::from_fn(|initial| {
            let first = self.0[initial];
            let second = later.apply(first.state);
            HoldProjection {
                state: second.state,
                clear_seen: first.clear_seen || second.clear_seen,
            }
        }))
    }
}

/// Logical packet order, including distinct originals at the same frame.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Point {
    pub(crate) frame: Frame,
    pub(crate) order: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SostenutoUpdate {
    Pedal(bool),
    /// All Sound Off clears latches without changing the pedal value.
    Clear,
    /// Reset Controllers and System Reset also put the pedal up.
    Reset,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SostenutoProjection {
    pub(crate) down: bool,
    pub(crate) last_rising: Option<Point>,
    pub(crate) first_clear: Option<Point>,
    pub(crate) last_clear: Option<Point>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SostenutoSummary([SostenutoProjection; 2]);
impl Default for SostenutoSummary {
    fn default() -> Self {
        Self(core::array::from_fn(|down| SostenutoProjection {
            down: down != 0,
            last_rising: None,
            first_clear: None,
            last_clear: None,
        }))
    }
}
impl SostenutoSummary {
    pub(crate) fn update(update: SostenutoUpdate, point: Point) -> Self {
        Self(core::array::from_fn(|initial| {
            let initial = initial != 0;
            let (down, rising, clear) = match update {
                SostenutoUpdate::Pedal(down) => (down, down && !initial, !down && initial),
                SostenutoUpdate::Clear => (initial, false, true),
                SostenutoUpdate::Reset => (false, false, true),
            };
            SostenutoProjection {
                down,
                last_rising: rising.then_some(point),
                first_clear: clear.then_some(point),
                last_clear: clear.then_some(point),
            }
        }))
    }
    pub(crate) fn apply(self, initial_down: bool) -> SostenutoProjection {
        self.0[usize::from(initial_down)]
    }
    pub(crate) fn then(self, later: Self) -> Self {
        Self(core::array::from_fn(|initial| {
            let first = self.0[initial];
            let second = later.apply(first.down);
            SostenutoProjection {
                down: second.down,
                last_rising: second.last_rising.or(first.last_rising),
                first_clear: first.first_clear.or(second.first_clear),
                last_clear: second.last_clear.or(first.last_clear),
            }
        }))
    }
}

#[cfg(test)]
mod tests;
