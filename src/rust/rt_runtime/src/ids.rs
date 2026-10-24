//! Plain-data identifiers and monotonic counters; no target-specific wide atomics.

use crate::Error;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Frame(pub u64);

impl Frame {
    pub fn checked_add(self, frames: usize) -> Result<Self, Error> {
        let frames = u64::try_from(frames).map_err(|_| Error::Exhausted)?;
        self.0.checked_add(frames).map(Self).ok_or(Error::Exhausted)
    }
}

macro_rules! counter {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name(pub u64);

        impl $name {
            pub fn checked_next(self) -> Result<Self, Error> {
                self.0.checked_add(1).map(Self).ok_or(Error::Exhausted)
            }
        }
    };
}

counter!(Epoch);
counter!(Revision);
counter!(Sequence);

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name {
            slot: u32,
            generation: std::num::NonZeroU32,
        }

        impl $name {
            pub fn from_parts(slot: u32, generation: u32) -> Result<Self, Error> {
                let generation = std::num::NonZeroU32::new(generation).ok_or(Error::InvalidId)?;
                Ok(Self { slot, generation })
            }

            pub fn slot(self) -> u32 {
                self.slot
            }

            pub fn generation(self) -> u32 {
                self.generation.get()
            }

            /// Exhausted slots must be retired permanently rather than reused.
            pub fn checked_next_generation(self) -> Result<Self, Error> {
                let generation = self
                    .generation
                    .get()
                    .checked_add(1)
                    .ok_or(Error::Exhausted)?;
                Self::from_parts(self.slot, generation)
            }
        }
    };
}

identity!(NodeId);
identity!(ResourceId);
