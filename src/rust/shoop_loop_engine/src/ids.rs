//! Distinct generation-checked musical and request identities.

use crate::Error;
use rt_runtime::NodeId;

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NodeId);

        impl $name {
            pub fn from_parts(slot: u32, generation: u32) -> Result<Self, Error> {
                NodeId::from_parts(slot, generation)
                    .map(Self)
                    .map_err(|_| Error::InvalidId)
            }
            pub fn slot(self) -> u32 {
                self.0.slot()
            }
            pub fn generation(self) -> u32 {
                self.0.generation()
            }
            pub fn checked_next_generation(self) -> Result<Self, Error> {
                self.0
                    .checked_next_generation()
                    .map(Self)
                    .map_err(|_| Error::Exhausted)
            }
        }
    };
}

identity!(LoopId);
identity!(RequestId);
