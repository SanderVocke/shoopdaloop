//! Backend-owned storage accounting, independent of elapsed-time profiling.

use crate::Name;
use serde::{Deserialize, Serialize};

pub const MAX_MEMORY_RESOURCES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemoryUnit {
    Bytes,
    Entries,
    Pages,
    Blocks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryResource {
    pub name: Name,
    pub unit: MemoryUnit,
    pub used: u64,
    /// Unused capacity committed to particular owners, included in pressure.
    pub reserved: u64,
    /// None means there is no known fixed ceiling, not unlimited memory.
    pub capacity: Option<u64>,
}

impl MemoryResource {
    pub fn valid(&self) -> bool {
        self.name.valid()
            && !self.name.as_str().is_empty()
            && self
                .used
                .checked_add(self.reserved)
                .is_some_and(|committed| {
                    self.capacity
                        .is_none_or(|capacity| capacity > 0 && committed <= capacity)
                })
    }

    pub fn pressure(&self) -> Option<f64> {
        self.capacity
            .filter(|&capacity| capacity > 0)
            .map(|capacity| (self.used as f64 + self.reserved as f64) / capacity as f64)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryReport {
    pub frame: u64,
    /// Accounted storage including prepared spare capacity; excludes allocator overhead.
    pub accounted_bytes: Option<u64>,
    pub resources: [Option<MemoryResource>; MAX_MEMORY_RESOURCES],
}

impl MemoryReport {
    pub fn valid(&self) -> bool {
        self.resources.iter().enumerate().all(|(i, resource)| {
            resource.as_ref().is_none_or(|resource| {
                resource.valid()
                    && !self.resources[..i]
                        .iter()
                        .flatten()
                        .any(|earlier| earlier.name == resource.name)
            })
        })
    }

    pub fn limiting_resource(&self) -> Option<&MemoryResource> {
        self.resources
            .iter()
            .flatten()
            .filter(|resource| resource.capacity.is_some())
            .max_by(|a, b| {
                a.pressure()
                    .unwrap_or(0.)
                    .total_cmp(&b.pressure().unwrap_or(0.))
            })
    }
}
