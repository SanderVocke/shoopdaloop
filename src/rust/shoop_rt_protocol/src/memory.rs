//! Backend-owned storage accounting shared by host and application diagnostics.

pub use profiling_protocol::memory::{
    MemoryReport, MemoryResource, MemoryUnit, MAX_MEMORY_RESOURCES,
};
