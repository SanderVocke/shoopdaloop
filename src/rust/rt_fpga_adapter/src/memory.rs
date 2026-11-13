//! Storage counters sampled through the device's read-only configuration port.
use rt_runtime::Error;
use shoop_rt_protocol::Name;
pub use shoop_rt_protocol::{MemoryReport, MemoryResource, MemoryUnit};

pub fn decode(frame: u64, usage: [u32; 5], geometry: [u32; 5]) -> Result<MemoryReport, Error> {
    let [blocks, limit, pages, page_limit, views] = usage;
    let [arena_lo, arena_hi, chunk_frames, block_chunks, view_limit] = geometry;
    if !(1..=65536).contains(&chunk_frames)
        || !chunk_frames.is_power_of_two()
        || !(1..=65536).contains(&block_chunks)
        || !block_chunks.is_power_of_two()
        || !(1..=65536).contains(&view_limit)
    {
        return Err(Error::InvalidParameter);
    }
    let block_bytes = u64::from(chunk_frames) * u64::from(block_chunks) * 4;
    let capacity = block_bytes
        .checked_mul(u64::from(limit))
        .ok_or(Error::Capacity)?;
    let arena_bytes = u64::from(arena_lo) | u64::from(arena_hi) << 32;
    if capacity
        .checked_add(u64::from(page_limit) * 64)
        .is_none_or(|bytes| bytes > arena_bytes)
    {
        return Err(Error::InvalidParameter);
    }
    let entry = |name, unit, used, capacity| {
        Some(MemoryResource {
            name: Name::new(name).expect("inline FPGA resource name"),
            unit,
            used,
            reserved: 0,
            capacity: Some(capacity),
        })
    };
    let report = MemoryReport {
        frame,
        accounted_bytes: None,
        resources: [
            entry(
                "Sample storage",
                MemoryUnit::Bytes,
                u64::from(blocks)
                    .checked_mul(block_bytes)
                    .ok_or(Error::Capacity)?,
                capacity,
            ),
            entry(
                "Metadata pages",
                MemoryUnit::Pages,
                u64::from(pages),
                u64::from(page_limit),
            ),
            entry(
                "View slots",
                MemoryUnit::Entries,
                u64::from(views),
                u64::from(view_limit),
            ),
            None,
            None,
            None,
            None,
            None,
        ],
    };
    if !report.valid() {
        return Err(Error::InvalidParameter);
    }
    Ok(report)
}
