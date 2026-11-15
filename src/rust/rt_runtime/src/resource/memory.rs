use super::PoolUsage;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryUsage {
    pub accounted_bytes: u64,
    pub payload_bytes: u64,
    pub page_bytes: u64,
    pub pages: u64,
    pub slots: u64,
    pub pools: PoolUsage,
}

pub(crate) fn vec_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity() * core::mem::size_of::<T>()
}
