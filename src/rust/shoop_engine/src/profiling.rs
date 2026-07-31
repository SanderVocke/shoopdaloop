//! Per-stage timing for one cycle, for a profiling display.
//!
//! UI had nothing to show. This is the smallest thing that answers the question such a window is
//! opened to answer: *which stage of the cycle is taking the time?*
//!
//! **Off by default, and off means free.** Reading a clock twice per scheduled node is cheap but not
//! nothing, and an audio callback should not pay for a window nobody has open. Disabled, the only
//! cost is one relaxed atomic load per stage.
//!
//! Times are held as nanoseconds in atomics so the control thread can read them without locking
//! while the audio thread is writing.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub struct ProfilingReportItem {
    pub key: String,
    pub n_samples: f32,
    pub average: f32,
    pub worst: f32,
    pub most_recent: f32,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingReport {
    pub items: Vec<ProfilingReportItem>,
}

/// Stages of a cycle worth separating. Anything finer would be reporting on the scheduler rather
/// than on the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    PortPrepare,
    PortProcess,
    ChannelPrepare,
    ChannelProcess,
    LoopProcess,
}

impl Stage {
    pub const ALL: [Stage; 5] = [
        Stage::PortPrepare,
        Stage::PortProcess,
        Stage::ChannelPrepare,
        Stage::ChannelProcess,
        Stage::LoopProcess,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stage::PortPrepare => "port prepare",
            Stage::PortProcess => "port process",
            Stage::ChannelPrepare => "channel prepare",
            Stage::ChannelProcess => "channel process",
            Stage::LoopProcess => "loop process",
        }
    }

    fn index(self) -> usize {
        match self {
            Stage::PortPrepare => 0,
            Stage::PortProcess => 1,
            Stage::ChannelPrepare => 2,
            Stage::ChannelProcess => 3,
            Stage::LoopProcess => 4,
        }
    }
}

/// What one stage cost, as the control side reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StageReport {
    /// Nanoseconds spent in this stage during the most recent cycle.
    pub last_ns: u64,
    /// Worst cycle since the last reset, which is what causes an xrun.
    pub worst_ns: u64,
    /// Times this stage ran in the most recent cycle.
    pub calls: u32,
}

#[derive(Debug, Default)]
struct StageCounters {
    last_ns: AtomicU64,
    worst_ns: AtomicU64,
    calls: AtomicU32,
    /// Accumulated within the cycle currently being measured.
    accumulating_ns: AtomicU64,
    accumulating_calls: AtomicU32,
}

/// Timing for a cycle, shared between the audio and control threads.
#[derive(Debug, Default)]
pub struct Profiler {
    enabled: AtomicBool,
    stages: [StageCounters; 5],
}

impl Profiler {
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Turning it on clears what was there, so a reading is never a mix of two sessions.
    pub fn set_enabled(&self, enabled: bool) {
        if enabled != self.enabled() {
            self.reset();
        }
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Times `f`, if enabled. Returns whatever `f` returns either way.
    ///
    /// Taking a closure rather than start/stop calls means a stage cannot be left unclosed by an
    /// early return.
    pub fn time<T>(&self, stage: Stage, f: impl FnOnce() -> T) -> T {
        if !self.enabled() {
            return f();
        }
        let began = std::time::Instant::now();
        let out = f();
        let ns = began.elapsed().as_nanos() as u64;
        let c = &self.stages[stage.index()];
        c.accumulating_ns.fetch_add(ns, Ordering::Relaxed);
        c.accumulating_calls.fetch_add(1, Ordering::Relaxed);
        out
    }

    /// Ends the cycle: what accumulated becomes the latest reading.
    ///
    /// Separate from `time` because a stage runs many times per cycle, and the useful figure is what
    /// the whole cycle spent in it rather than what one call did.
    pub fn end_cycle(&self) {
        if !self.enabled() {
            return;
        }
        for c in &self.stages {
            let ns = c.accumulating_ns.swap(0, Ordering::Relaxed);
            let calls = c.accumulating_calls.swap(0, Ordering::Relaxed);
            c.last_ns.store(ns, Ordering::Relaxed);
            c.calls.store(calls, Ordering::Relaxed);
            if ns > c.worst_ns.load(Ordering::Relaxed) {
                c.worst_ns.store(ns, Ordering::Relaxed);
            }
        }
    }

    pub fn report(&self, stage: Stage) -> StageReport {
        let c = &self.stages[stage.index()];
        StageReport {
            last_ns: c.last_ns.load(Ordering::Relaxed),
            worst_ns: c.worst_ns.load(Ordering::Relaxed),
            calls: c.calls.load(Ordering::Relaxed),
        }
    }

    /// Total across every stage for the last cycle, for comparing against the buffer's budget.
    pub fn last_total_ns(&self) -> u64 {
        Stage::ALL.iter().map(|s| self.report(*s).last_ns).sum()
    }

    pub fn reset(&self) {
        for c in &self.stages {
            c.last_ns.store(0, Ordering::Relaxed);
            c.worst_ns.store(0, Ordering::Relaxed);
            c.calls.store(0, Ordering::Relaxed);
            c.accumulating_ns.store(0, Ordering::Relaxed);
            c.accumulating_calls.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[test]
    fn disabled_costs_nothing_and_reports_nothing() {
        let p = Profiler::default();
        check!(!p.enabled());

        let mut ran = false;
        p.time(Stage::LoopProcess, || ran = true);
        p.end_cycle();

        // The work still happened; only the measurement was skipped.
        check!(ran);
        check!(p.report(Stage::LoopProcess) == StageReport::default());
    }

    #[test]
    fn a_timed_stage_is_reported_after_the_cycle_ends() {
        let p = Profiler::default();
        p.set_enabled(true);

        p.time(Stage::PortProcess, || {
            std::thread::sleep(std::time::Duration::from_millis(2));
        });
        // Nothing yet: a stage runs many times per cycle, so the figure is only meaningful once the
        // cycle closes.
        check!(p.report(Stage::PortProcess).last_ns == 0);

        p.end_cycle();
        let r = p.report(Stage::PortProcess);
        check!(r.calls == 1);
        check!(r.last_ns > 1_000_000, "measured {}ns", r.last_ns);
    }

    #[test]
    fn calls_within_a_cycle_accumulate() {
        let p = Profiler::default();
        p.set_enabled(true);

        for _ in 0..3 {
            p.time(Stage::ChannelProcess, || {});
        }
        p.end_cycle();
        check!(p.report(Stage::ChannelProcess).calls == 3);
    }

    #[test]
    fn a_new_cycle_replaces_the_last_reading() {
        let p = Profiler::default();
        p.set_enabled(true);

        p.time(Stage::LoopProcess, || {
            std::thread::sleep(std::time::Duration::from_millis(2));
        });
        p.end_cycle();
        let busy = p.report(Stage::LoopProcess).last_ns;

        // A quiet cycle: the latest reading drops, so the display follows what is happening now.
        p.end_cycle();
        check!(p.report(Stage::LoopProcess).last_ns < busy);
    }

    #[test]
    fn the_worst_cycle_is_remembered_when_the_latest_is_not() {
        let p = Profiler::default();
        p.set_enabled(true);

        p.time(Stage::LoopProcess, || {
            std::thread::sleep(std::time::Duration::from_millis(3));
        });
        p.end_cycle();
        let worst = p.report(Stage::LoopProcess).worst_ns;
        check!(worst > 0);

        p.end_cycle();
        // The spike is what causes an xrun, so it survives quiet cycles.
        check!(p.report(Stage::LoopProcess).worst_ns == worst);
        check!(p.report(Stage::LoopProcess).last_ns < worst);
    }

    #[test]
    fn stages_are_measured_separately() {
        let p = Profiler::default();
        p.set_enabled(true);

        p.time(Stage::LoopProcess, || {
            std::thread::sleep(std::time::Duration::from_millis(2));
        });
        p.time(Stage::PortPrepare, || {});
        p.end_cycle();

        check!(p.report(Stage::LoopProcess).last_ns > p.report(Stage::PortPrepare).last_ns);
        check!(p.report(Stage::ChannelPrepare).calls == 0);
    }

    #[test]
    fn the_total_spans_every_stage() {
        let p = Profiler::default();
        p.set_enabled(true);
        p.time(Stage::PortPrepare, || {
            std::thread::sleep(std::time::Duration::from_millis(1));
        });
        p.time(Stage::LoopProcess, || {
            std::thread::sleep(std::time::Duration::from_millis(1));
        });
        p.end_cycle();

        let total = p.last_total_ns();
        check!(
            total >= p.report(Stage::PortPrepare).last_ns + p.report(Stage::LoopProcess).last_ns
        );
    }

    #[test]
    fn enabling_clears_a_previous_run() {
        let p = Profiler::default();
        p.set_enabled(true);
        p.time(Stage::LoopProcess, || {
            std::thread::sleep(std::time::Duration::from_millis(2));
        });
        p.end_cycle();
        check!(p.report(Stage::LoopProcess).worst_ns > 0);

        p.set_enabled(false);
        p.set_enabled(true);
        // A reading must never mix two runs.
        check!(p.report(Stage::LoopProcess).worst_ns == 0);
    }

    #[test]
    fn every_stage_has_a_name() {
        for s in Stage::ALL {
            check!(!s.name().is_empty());
        }
    }
}
