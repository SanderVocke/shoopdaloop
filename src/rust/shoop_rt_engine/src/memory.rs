use core::sync::atomic::{AtomicU64, Ordering};
use rt_runtime::resource::MemoryUsage;
use shoop_rt_protocol::codec::{decode_engine, encode_engine};
use shoop_rt_protocol::{
    EngineMessage, MemoryReport, MemoryResource, MemoryUnit, Name, Telemetry, MAX_RECORD_BYTES,
};

const REPORT_WORDS: usize = MAX_RECORD_BYTES.div_ceil(8);

/// Latest storage accounting. The native runtime publishes its pool counters;
/// an external consumer publishes complete reports, which then take precedence.
/// Each source has a single writer.
pub(crate) struct Memory {
    sequence: AtomicU64,
    values: [AtomicU64; 10],
    report_sequence: AtomicU64,
    /// Encoded telemetry record length, then its bytes in little-endian words.
    report: [AtomicU64; 1 + REPORT_WORDS],
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            values: Default::default(),
            report_sequence: AtomicU64::new(0),
            report: core::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
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

    fn consumer_report(frame: u64) -> MemoryReport {
        let mut report = MemoryReport {
            frame,
            ..Default::default()
        };
        for (index, slot) in report.resources.iter_mut().enumerate() {
            let name = alloc::format!(
                "{index}{}",
                "x".repeat(shoop_rt_protocol::MAX_NAME_BYTES - 1)
            );
            *slot = Some(MemoryResource {
                name: Name::new(&name).unwrap(),
                unit: MemoryUnit::Blocks,
                used: frame,
                reserved: 0,
                capacity: Some(u64::MAX),
            });
        }
        report
    }

    #[shoop_wasm_test_support::shoop_test]
    fn consumer_reports_of_maximum_size_round_trip_and_replace_native_accounting() {
        let memory = Memory::default();
        memory.publish(
            1,
            MemoryUsage {
                accounted_bytes: 4096,
                payload_bytes: 4096,
                page_bytes: 4096,
                pages: 1,
                slots: 1,
                pools: PoolUsage {
                    free_pages: 1,
                    reserved_pages: 0,
                    free_versions: 1,
                    free_snapshots: 1,
                    retired_versions: 0,
                    retired_pages: 0,
                },
            },
        );
        assert_eq!(memory.read().unwrap().accounted_bytes, Some(4096));
        let report = consumer_report(64);
        assert!(report.valid());
        memory.publish_report(&report);
        assert_eq!(memory.read(), Some(report));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn concurrent_readers_only_observe_complete_consumer_reports() {
        let memory = Memory::default();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for frame in 1..5_000 {
                    memory.publish_report(&consumer_report(frame));
                }
            });
            for _ in 0..5_000 {
                if let Some(report) = memory.read() {
                    assert_eq!(report, consumer_report(report.frame));
                }
            }
        });
        assert_eq!(memory.read().unwrap().frame, 4999);
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

    /// Stores a complete report without blocking or allocating. A report that
    /// cannot be encoded as one telemetry record is not published.
    pub fn publish_report(&self, report: &MemoryReport) {
        let mut bytes = [0; REPORT_WORDS * 8];
        let message = EngineMessage::Telemetry(Telemetry::Memory(*report));
        let Ok(len) = encode_engine(&message, &mut bytes) else {
            return;
        };
        self.report_sequence.fetch_add(1, Ordering::SeqCst);
        self.report[0].store(len as u64, Ordering::SeqCst);
        let words = bytes[..len.div_ceil(8) * 8].chunks_exact(8);
        for (cell, word) in self.report[1..].iter().zip(words) {
            cell.store(
                u64::from_le_bytes(word.try_into().expect("eight-byte chunk")),
                Ordering::SeqCst,
            );
        }
        self.report_sequence.fetch_add(1, Ordering::SeqCst);
    }

    fn read_report(&self, sequence: u64) -> Option<MemoryReport> {
        if sequence % 2 != 0 {
            return None;
        }
        let len = (self.report[0].load(Ordering::SeqCst) as usize).min(REPORT_WORDS * 8);
        let mut bytes = [0; REPORT_WORDS * 8];
        let words = bytes[..len.div_ceil(8) * 8].chunks_exact_mut(8);
        for (word, cell) in words.zip(&self.report[1..]) {
            word.copy_from_slice(&cell.load(Ordering::SeqCst).to_le_bytes());
        }
        if self.report_sequence.load(Ordering::SeqCst) != sequence {
            return None;
        }
        match decode_engine(&bytes[..len]) {
            Ok(EngineMessage::Telemetry(Telemetry::Memory(report))) => Some(report),
            _ => None,
        }
    }

    pub fn read(&self) -> Option<MemoryReport> {
        let reported = self.report_sequence.load(Ordering::SeqCst);
        if reported != 0 {
            return self.read_report(reported);
        }
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
