use crate::{Statistics, TraceSummary, CALLBACK, FAULT, MAX_CAPTURE_TRACES, PART, TRUNCATED};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub before_ns: Option<u64>,
    pub after_ns: Option<u64>,
    pub between_ns: Option<u64>,
    pub total_ns: u64,
    pub graphs: u32,
    pub incomplete: bool,
}
impl Envelope {
    pub fn from_traces<'a>(
        callback: TraceSummary,
        traces: impl Iterator<Item = &'a TraceSummary>,
    ) -> Option<Self> {
        if callback.flags & CALLBACK == 0 || callback.flags & PART != 0 {
            return None;
        }
        let mut result = Self {
            total_ns: callback.end_ns.checked_sub(callback.start_ns)?,
            graphs: callback.graphs,
            ..Self::default()
        };
        let mut first = None;
        let mut last = None;
        let mut between = 0u64;
        let mut count = 0u32;
        let mut valid = callback.flags & (FAULT | TRUNCATED) == 0;
        for tick in traces.filter(|t| t.flags & CALLBACK == 0 && callback.contains(t)) {
            count = count.saturating_add(1);
            if let (Some(start), Some(end)) = (tick.graph_start_ns, tick.graph_end_ns) {
                if let Some(previous) = last {
                    if let Some(gap) = start.checked_sub(previous) {
                        between = between.saturating_add(gap);
                    } else {
                        valid = false;
                    }
                }
                first.get_or_insert(start);
                last = Some(end);
            } else {
                valid = false;
            }
        }
        valid &= count == callback.graphs;
        result.incomplete = !valid;
        if valid && count != 0 {
            result.before_ns = first.and_then(|s| s.checked_sub(callback.start_ns));
            result.after_ns = last.and_then(|s| callback.end_ns.checked_sub(s));
            result.between_ns = Some(between);
            result.incomplete |= result.before_ns.is_none() || result.after_ns.is_none();
        }
        Some(result)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WrappingStats {
    pub before: Statistics,
    pub after: Statistics,
    pub between: Statistics,
    pub before_budget: Statistics,
    pub after_budget: Statistics,
    pub between_budget: Statistics,
    pub total: Statistics,
    pub without_graph: u64,
    pub incomplete: u64,
}
impl WrappingStats {
    /// Budget-relative durations, scaled to nanoseconds of a one-second allowance.
    pub const BUDGET_SCALE: u64 = 1_000_000_000;

    pub fn valid(&self) -> bool {
        [
            &self.before,
            &self.after,
            &self.between,
            &self.total,
            &self.before_budget,
            &self.after_budget,
            &self.between_budget,
        ]
        .into_iter()
        .all(|s| s.mean.is_finite() && s.m2.is_finite())
            && self.before.count <= self.total.count
            && self.after.count <= self.total.count
            && self.between.count <= self.total.count
            && self.before_budget.count <= self.before.count
            && self.after_budget.count <= self.after.count
            && self.between_budget.count <= self.between.count
    }
}

#[derive(Default)]
pub(crate) struct Wrapping {
    pub stats: WrappingStats,
    pending: Vec<TraceSummary>,
    lost: bool,
}
impl Wrapping {
    pub fn accept(&mut self, trace: TraceSummary) {
        if trace.flags & CALLBACK == 0 {
            if self.pending.len() < MAX_CAPTURE_TRACES {
                self.pending.push(trace);
            } else {
                self.lost = true;
            }
            return;
        }
        if trace.flags & PART != 0 {
            return;
        }
        if let Some(envelope) = Envelope::from_traces(trace, self.pending.iter()) {
            self.stats.total.add(envelope.total_ns);
            if envelope.incomplete || self.lost {
                self.stats.incomplete += 1;
            } else if envelope.graphs == 0 {
                self.stats.without_graph += 1;
            } else if let (Some(before), Some(after), Some(between)) =
                (envelope.before_ns, envelope.after_ns, envelope.between_ns)
            {
                self.stats.before.add(before);
                self.stats.after.add(after);
                self.stats.between.add(between);
                if trace.budget_ns != 0 {
                    let normalized = |ns: u64| {
                        (u128::from(ns) * u128::from(WrappingStats::BUDGET_SCALE)
                            / u128::from(trace.budget_ns))
                        .min(u128::from(u64::MAX)) as u64
                    };
                    self.stats.before_budget.add(normalized(before));
                    self.stats.after_budget.add(normalized(after));
                    self.stats.between_budget.add(normalized(between));
                }
            }
        }
        self.pending.retain(|t| t.end_ns > trace.end_ns);
        self.lost = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    fn tick(start: u64, end: u64, graph_start: u64, graph_end: u64) -> TraceSummary {
        TraceSummary {
            epoch: 1,
            start_ns: start,
            end_ns: end,
            graph_start_ns: Some(graph_start),
            graph_end_ns: Some(graph_end),
            ..TraceSummary::default()
        }
    }
    fn callback() -> TraceSummary {
        TraceSummary {
            epoch: 1,
            start_ns: 100,
            end_ns: 500,
            graphs: 2,
            flags: CALLBACK | crate::HOST,
            ..TraceSummary::default()
        }
    }
    #[shoop_test]
    fn wrapper_intervals_cover_the_callback_without_counting_other_ticks_as_prefix_or_suffix() {
        let ticks = [tick(120, 250, 150, 220), tick(280, 430, 310, 400)];
        let result = Envelope::from_traces(callback(), ticks.iter()).unwrap();
        assert_eq!(
            (result.before_ns, result.between_ns, result.after_ns),
            (Some(50), Some(90), Some(100))
        );
        assert_eq!(result.total_ns, 400);
        assert!(!result.incomplete);
        let mut wrapping = Wrapping::default();
        for t in ticks {
            wrapping.accept(t);
        }
        wrapping.accept(TraceSummary {
            flags: CALLBACK | PART,
            ..callback()
        });
        assert_eq!(wrapping.stats.total.count, 0);
        wrapping.accept(callback());
        assert_eq!(wrapping.stats.before.mean, 50.);
        assert_eq!(wrapping.stats.between.worst, 90);
        assert_eq!(wrapping.stats.after.mean, 100.);
        assert_eq!(wrapping.stats.total.count, 1);
    }
    #[shoop_test]
    fn missing_or_unfinished_graph_records_do_not_turn_into_wrapper_time() {
        let ticks = [tick(120, 250, 150, 220)];
        let result = Envelope::from_traces(callback(), ticks.iter()).unwrap();
        assert!(result.incomplete);
        assert_eq!(
            (result.before_ns, result.after_ns, result.between_ns),
            (None, None, None)
        );
        let ticks = [
            ticks[0],
            TraceSummary {
                graph_end_ns: None,
                ..tick(280, 430, 310, 400)
            },
        ];
        let result = Envelope::from_traces(callback(), ticks.iter()).unwrap();
        assert!(result.incomplete);
        assert!(result.before_ns.is_none());
        let empty = TraceSummary {
            graphs: 0,
            ..callback()
        };
        let result = Envelope::from_traces(empty, core::iter::empty()).unwrap();
        assert!(!result.incomplete);
        assert_eq!(result.total_ns, 400);
        assert_eq!((result.before_ns, result.after_ns), (None, None));
    }
    #[shoop_test]
    fn wrapper_population_statistics_use_callback_samples() {
        let mut wrapping = Wrapping::default();
        for (offset, before) in [(0, 20), (1000, 80)] {
            wrapping.accept(tick(
                offset + before,
                offset + 100,
                offset + before,
                offset + 90,
            ));
            wrapping.accept(TraceSummary {
                epoch: 1,
                start_ns: offset,
                end_ns: offset + 110,
                graphs: 1,
                flags: CALLBACK,
                ..TraceSummary::default()
            });
        }
        assert_eq!(wrapping.stats.before.count, 2);
        assert_eq!(wrapping.stats.before.mean, 50.);
        assert_eq!(wrapping.stats.before.worst, 80);
        assert_eq!(wrapping.stats.before.stddev(), 30.);
        assert_eq!(wrapping.stats.after.mean, 20.);
        assert_eq!(wrapping.stats.between.worst, 0);
    }

    #[shoop_test]
    fn wrapper_budget_statistics_normalize_each_callback_before_aggregation() {
        let mut wrapping = Wrapping::default();
        for (offset, frames, budget_ns, before, between, after) in [
            (0, 32, 1000, 100, 50, 200),
            (10000, 128, 4000, 200, 200, 400),
        ] {
            let first = offset + before;
            let second = first + 20 + between;
            wrapping.accept(tick(first, first + 20, first, first + 20));
            wrapping.accept(tick(second, second + 20, second, second + 20));
            wrapping.accept(TraceSummary {
                epoch: 1,
                start_ns: offset,
                end_ns: second + 20 + after,
                frames,
                budget_ns,
                graphs: 2,
                flags: CALLBACK,
                ..TraceSummary::default()
            });
        }
        let s = &wrapping.stats;
        assert_eq!(s.before.mean, 150.);
        assert_eq!(s.before_budget.count, 2);
        assert_eq!(s.before_budget.mean, 75_000_000.);
        assert_eq!(s.before_budget.worst, 100_000_000);
        assert_eq!(s.before_budget.stddev(), 25_000_000.);
        assert_eq!(s.after_budget.mean, 150_000_000.);
        assert_eq!(s.after_budget.worst, 200_000_000);
        assert_eq!(s.after_budget.stddev(), 50_000_000.);
        assert_eq!(s.between_budget.mean, 50_000_000.);
        assert_eq!(s.between_budget.stddev(), 0.);
        assert!(s.valid());
    }

    #[shoop_test]
    fn missing_callback_budget_keeps_durations_without_inventing_percentages() {
        let mut wrapping = Wrapping::default();
        for t in [tick(120, 250, 150, 220), tick(280, 430, 310, 400)] {
            wrapping.accept(t);
        }
        wrapping.accept(callback());
        assert_eq!(wrapping.stats.before.mean, 50.);
        assert_eq!(wrapping.stats.before_budget.count, 0);
        assert_eq!(wrapping.stats.after_budget.count, 0);
        assert_eq!(wrapping.stats.between_budget.count, 0);
        assert!(wrapping.stats.valid());
        wrapping.stats.before_budget.mean = f64::NAN;
        assert!(!wrapping.stats.valid());
    }
}
