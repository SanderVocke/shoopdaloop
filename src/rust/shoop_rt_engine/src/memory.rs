use core::sync::atomic::{AtomicU64, Ordering};
use rt_runtime::resource::MemoryUsage;
use shoop_rt_protocol::{MemoryReport, MemoryResource, MemoryUnit, Name};

#[derive(Default)]
pub(crate) struct Memory {
    sequence: AtomicU64,
    values: [AtomicU64; 10],
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use rt_runtime::resource::PoolUsage;

    #[shoop_wasm_test_support::shoop_test]
    fn concurrent_readers_only_observe_complete_memory_snapshots() {
        let memory = Memory::default();
        assert!(memory.read().is_none());
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for frame in 1..10_000 {
                    memory.publish(
                        frame,
                        MemoryUsage {
                            accounted_bytes: frame,
                            payload_bytes: 4096,
                            page_bytes: 4096,
                            pages: 1,
                            slots: 4,
                            pools: PoolUsage {
                                free_pages: 1,
                                reserved_pages: 0,
                                free_versions: 4,
                                free_snapshots: 4,
                                retired_versions: 0,
                                retired_pages: 0,
                            },
                        },
                    );
                }
            });
            for _ in 0..10_000 {
                if let Some(report) = memory.read() {
                    assert_eq!(report.accounted_bytes, Some(report.frame));
                    assert!(report.valid());
                }
            }
        });
        assert_eq!(memory.read().unwrap().frame, 9999);
    }
}

impl Memory {
    pub fn publish(&self, frame: u64, usage: MemoryUsage) {
        let pools = usage.pools;
        // Sequential consistency makes a mixed snapshot detectable without reader retries.
        self.sequence.fetch_add(1, Ordering::SeqCst);
        for (cell, value) in self.values.iter().zip([
            frame,
            usage.accounted_bytes,
            usage.payload_bytes,
            usage.page_bytes,
            usage.pages,
            usage.slots,
            pools.free_pages as u64,
            pools.reserved_pages as u64,
            pools.free_versions as u64,
            pools.free_snapshots as u64,
        ]) {
            cell.store(value, Ordering::SeqCst);
        }
        self.sequence.fetch_add(1, Ordering::SeqCst);
    }

    pub fn read(&self) -> Option<MemoryReport> {
        let sequence = self.sequence.load(Ordering::SeqCst);
        if sequence == 0 || sequence % 2 != 0 {
            return None;
        }
        let [frame, accounted, payload, page_bytes, pages, slots, free, reserved, versions, snapshots] =
            self.values
                .each_ref()
                .map(|cell| cell.load(Ordering::SeqCst));
        if self.sequence.load(Ordering::SeqCst) != sequence {
            return None;
        }
        let resource = |name, unit, used, reserved, capacity| {
            Some(MemoryResource {
                name: Name::new(name).expect("inline memory resource name"),
                unit,
                used,
                reserved,
                capacity: Some(capacity),
            })
        };
        Some(MemoryReport {
            frame,
            accounted_bytes: Some(accounted),
            resources: [
                resource(
                    "Media payload",
                    MemoryUnit::Bytes,
                    (pages - free) * page_bytes,
                    reserved * page_bytes,
                    payload,
                ),
                resource(
                    "Resource versions",
                    MemoryUnit::Entries,
                    slots - versions,
                    0,
                    slots,
                ),
                resource(
                    "Snapshots",
                    MemoryUnit::Entries,
                    slots - snapshots,
                    0,
                    slots,
                ),
                None,
                None,
                None,
                None,
                None,
            ],
        })
    }
}
