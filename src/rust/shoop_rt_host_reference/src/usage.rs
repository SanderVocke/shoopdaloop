use core::sync::atomic::{AtomicU64, Ordering::SeqCst};

use rt_fpga_compiler::{
    loading::{Allocator, Layout},
    ProgramLimits,
};
use shoop_rt_protocol::{
    EngineMessage, Lane, MemoryReport, MemoryResource, MemoryUnit, Name, Telemetry,
};
use shoop_rt_transport::carrier::Outbox;

pub(crate) const AUDIO_STORAGE: &str = "Audio storage";

#[derive(Clone, Copy, Debug)]
pub struct StorageUsage {
    pub audio_blocks: (u64, u64),
    pub metadata_pages: Option<(u64, u64)>,
}

#[derive(Debug, Default)]
pub(crate) struct StorageCounter {
    sequence: AtomicU64,
    counts: [AtomicU64; 4],
}

impl StorageCounter {
    // The storage owner is the sole writer. A contended reader skips the sample.
    pub fn publish(&self, usage: StorageUsage) {
        let (audio_used, audio_limit) = usage.audio_blocks;
        let (pages_used, pages_limit) = usage.metadata_pages.unwrap_or_default();
        self.sequence.fetch_add(1, SeqCst);
        for (counter, value) in
            self.counts
                .iter()
                .zip([audio_used, audio_limit, pages_used, pages_limit])
        {
            counter.store(value, SeqCst);
        }
        self.sequence.fetch_add(1, SeqCst);
    }

    pub fn report(&self, frame: u64) -> Option<MemoryReport> {
        let before = self.sequence.load(SeqCst);
        if before == 0 || before % 2 != 0 {
            return None;
        }
        let [audio_used, audio_limit, pages_used, pages_limit] =
            self.counts.each_ref().map(|counter| counter.load(SeqCst));
        if self.sequence.load(SeqCst) != before {
            return None;
        }
        let mut report = MemoryReport {
            frame,
            ..Default::default()
        };
        report.resources[0] = bounded(
            AUDIO_STORAGE,
            MemoryUnit::Blocks,
            audio_used,
            0,
            audio_limit,
        );
        report.resources[1] = bounded(
            "Media metadata",
            MemoryUnit::Pages,
            pages_used,
            0,
            pages_limit,
        );
        Some(report)
    }
}

fn bounded(
    name: &str,
    unit: MemoryUnit,
    used: u64,
    reserved: u64,
    capacity: u64,
) -> Option<MemoryResource> {
    let resource = MemoryResource {
        name: Name::new(name).expect("inline resource name"),
        unit,
        used,
        reserved,
        capacity: Some(capacity),
    };
    resource.valid().then_some(resource)
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProgramUsage {
    pub instructions: u32,
    pub jobs: u32,
    pub buffers: u32,
}

pub(crate) fn add_program_usage(
    report: &mut MemoryReport,
    allocator: &Allocator,
    limits: ProgramLimits,
    current: ProgramUsage,
    pending: Option<ProgramUsage>,
) {
    let next = pending.unwrap_or(current);
    let used = allocator
        .current()
        .map(Layout::allocations)
        .unwrap_or_default();
    let reserved = allocator
        .pending()
        .map(Layout::fresh_allocations)
        .unwrap_or_default();
    let row = |name, used, reserved, capacity| {
        bounded(
            name,
            MemoryUnit::Entries,
            used,
            reserved,
            u64::from(capacity),
        )
    };
    // Either program can run at a boundary. Their per-program requirements share
    // a denominator. Fresh allocations in shared state pools coexist until the fence.
    let program_row = |name, used: u32, next: u32, capacity| {
        row(
            name,
            used.into(),
            next.saturating_sub(used).into(),
            capacity,
        )
    };
    report.resources[2..].copy_from_slice(&[
        program_row(
            "Program instructions",
            current.instructions,
            next.instructions,
            limits.instructions,
        ),
        row(
            "State slots",
            used.state as u64,
            reserved.state as u64,
            limits.state_values,
        ),
        row(
            "Loop slots",
            used.loops as u64,
            reserved.loops as u64,
            limits.loop_slots,
        ),
        program_row("Job slots", current.jobs, next.jobs, limits.jobs),
        program_row(
            "Audio buffers",
            current.buffers,
            next.buffers,
            limits.buffers,
        ),
        row(
            "History slots",
            used.history as u64,
            reserved.history as u64,
            limits.history_slots,
        ),
    ]);
}

#[derive(Debug, Default)]
pub(crate) struct UsageTelemetry {
    last: Option<MemoryReport>,
}

impl UsageTelemetry {
    pub fn publish(&mut self, report: MemoryReport, interval: u64, out: &mut Outbox) {
        if out.available(Lane::Telemetry) == 0 {
            return;
        }
        if self.last.is_some_and(|last| {
            last.resources == report.resources && report.frame.saturating_sub(last.frame) < interval
        }) {
            return;
        }
        if out
            .push(EngineMessage::Telemetry(Telemetry::Memory(report)))
            .is_ok()
        {
            self.last = Some(report);
        }
    }
}
