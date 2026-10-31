//! Engine identities.
//!
//! A slot ID names a prepared engine slot and its generation. Reusing a slot raises
//! its generation, so a stale ID never addresses the new occupant. IDs are not
//! bounded by an initial capacity: limits may grow.

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SlotId {
    pub index: u32,
    pub generation: u32,
}

impl SlotId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

/// A channel strip: one application track's signal chain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StripId(pub SlotId);

/// A loop slot belonging to a strip.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LoopSlotId(pub SlotId);

/// A host (device) port slot.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HostPortId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BusId {
    Master,
}
