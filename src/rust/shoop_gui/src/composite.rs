//! Composite loops: a sequence that plays other loops in turn.
//!
//! The existing GUI builds these above the engine rather than inside it, and that is right
//! -- the engine knows about loops and sync, not about arrangements. A composite is a list
//! of members with a start cycle and a length in cycles, and playing it means issuing the
//! same play and stop commands a user would, at the cycle each member is due.
//!
//! Cycles, not frames. Everything is expressed in sync-loop cycles because that is the
//! grid a user arranges on, and it makes the schedule independent of sample rate and buffer
//! size.
//!
//! Kept out of the UI so the scheduling can be tested: an arrangement that starts a member
//! one cycle late is the kind of bug that is obvious to hear and tedious to find by ear.

use crate::selection::Cell;

/// One member's place in the arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub cell: Cell,
    /// Cycle within the composite at which this member starts.
    pub start_cycle: u32,
    /// How many cycles it plays for. Zero is treated as one, since a member that plays for
    /// no cycles cannot be what was meant.
    pub n_cycles: u32,
}

impl Entry {
    pub fn new(cell: Cell, start_cycle: u32, n_cycles: u32) -> Self {
        Self {
            cell,
            start_cycle,
            n_cycles: n_cycles.max(1),
        }
    }

    /// First cycle after this member finishes.
    pub fn end_cycle(&self) -> u32 {
        self.start_cycle + self.n_cycles
    }
}

#[derive(Debug, Clone, Default)]
pub struct Composite {
    pub name: String,
    pub entries: Vec<Entry>,
}

impl Composite {
    /// Cycles the whole arrangement spans.
    pub fn total_cycles(&self) -> u32 {
        self.entries
            .iter()
            .map(|e| e.end_cycle())
            .max()
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Members that start, and members that stop, at `cycle`.
    ///
    /// Stops are reported for the cycle a member ends *on*, so a member of one cycle
    /// starting at 0 stops at 1 and does not bleed into the next.
    ///
    /// A member listed twice at the same cycle is started once: the caller issues commands
    /// from this, and issuing two starts would retrigger.
    pub fn events_at(&self, cycle: u32) -> (Vec<Cell>, Vec<Cell>) {
        let mut starts: Vec<Cell> = Vec::new();
        let mut stops: Vec<Cell> = Vec::new();
        for e in &self.entries {
            if e.start_cycle == cycle && !starts.contains(&e.cell) {
                starts.push(e.cell);
            }
            if e.end_cycle() == cycle && !stops.contains(&e.cell) {
                stops.push(e.cell);
            }
        }
        // A member that stops and starts in the same cycle is continuing, so neither
        // command is issued: stopping and restarting it would produce an audible gap.
        let continuing: Vec<Cell> = starts
            .iter()
            .filter(|c| stops.contains(c))
            .copied()
            .collect();
        starts.retain(|c| !continuing.contains(c));
        stops.retain(|c| !continuing.contains(c));
        (starts, stops)
    }

    /// Everything still playing at the end, which the caller stops when the run finishes.
    pub fn members(&self) -> Vec<Cell> {
        let mut out: Vec<Cell> = Vec::new();
        for e in &self.entries {
            if !out.contains(&e.cell) {
                out.push(e.cell);
            }
        }
        out
    }
}

/// Where a composite has got to.
#[derive(Debug, Clone, Default)]
pub struct Playback {
    cycle: u32,
    running: bool,
    looping: bool,
}

impl Playback {
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn cycle(&self) -> u32 {
        self.cycle
    }
    pub fn looping(&self) -> bool {
        self.looping
    }
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Starts from the beginning, returning what should start now.
    pub fn start(&mut self, composite: &Composite) -> (Vec<Cell>, Vec<Cell>) {
        self.running = !composite.is_empty();
        self.cycle = 0;
        if !self.running {
            return (Vec::new(), Vec::new());
        }
        composite.events_at(0)
    }

    /// Stops, returning every member so the caller can silence them.
    pub fn stop(&mut self, composite: &Composite) -> Vec<Cell> {
        self.running = false;
        self.cycle = 0;
        composite.members()
    }

    /// Advances one sync cycle.
    ///
    /// Called when the sync loop wraps. Returns what starts and what stops. At the end of
    /// the arrangement it either wraps round or stops, and stopping reports every member so
    /// nothing is left ringing.
    pub fn advance(&mut self, composite: &Composite) -> (Vec<Cell>, Vec<Cell>) {
        if !self.running {
            return (Vec::new(), Vec::new());
        }
        self.cycle += 1;

        let total = composite.total_cycles();
        if self.cycle >= total {
            if self.looping {
                self.cycle = 0;
                let (starts, _) = composite.events_at(0);
                // Everything that was playing stops, then cycle zero's members start, so a
                // wrap behaves like a fresh run.
                let stops = composite
                    .members()
                    .into_iter()
                    .filter(|c| !starts.contains(c))
                    .collect();
                return (starts, stops);
            }
            self.running = false;
            return (Vec::new(), composite.members());
        }
        composite.events_at(self.cycle)
    }
}

/// Detects sync-loop wraps from polled positions.
///
/// The engine publishes a position, not a cycle count, so a wrap is inferred from the
/// position going backwards. That is why it needs to be stateful, and why it is separate:
/// it is easy to get wrong and easy to test.
#[derive(Debug, Clone, Default)]
pub struct CycleCounter {
    last: Option<u32>,
}

impl CycleCounter {
    /// Feeds a position, reporting whether the loop wrapped since the previous one.
    ///
    /// The first position never counts as a wrap: there is nothing to compare it to.
    pub fn update(&mut self, position: u32) -> bool {
        let wrapped = matches!(self.last, Some(prev) if position < prev);
        self.last = Some(position);
        wrapped
    }

    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(track: usize, row: usize) -> Cell {
        Cell { track, row }
    }

    /// Two members in sequence: A for two cycles, then B for one.
    fn sequence() -> Composite {
        Composite {
            name: "seq".to_string(),
            entries: vec![Entry::new(cell(0, 0), 0, 2), Entry::new(cell(1, 0), 2, 1)],
        }
    }

    #[test]
    fn total_cycles_spans_the_whole_arrangement() {
        assert_eq!(sequence().total_cycles(), 3);
        assert_eq!(Composite::default().total_cycles(), 0);
    }

    #[test]
    fn a_zero_length_member_is_treated_as_one_cycle() {
        let e = Entry::new(cell(0, 0), 0, 0);
        assert_eq!(e.n_cycles, 1);
        assert_eq!(e.end_cycle(), 1);
    }

    #[test]
    fn members_start_and_stop_on_their_own_cycles() {
        let c = sequence();

        let (starts, stops) = c.events_at(0);
        assert_eq!(starts, vec![cell(0, 0)]);
        assert!(stops.is_empty());

        // Nothing happens mid-member.
        let (starts, stops) = c.events_at(1);
        assert!(starts.is_empty());
        assert!(stops.is_empty());

        // A ends as B begins.
        let (starts, stops) = c.events_at(2);
        assert_eq!(starts, vec![cell(1, 0)]);
        assert_eq!(stops, vec![cell(0, 0)]);
    }

    #[test]
    fn a_member_continuing_across_a_boundary_is_not_restarted() {
        // Back-to-back entries for the same cell: it should play through, not gap.
        let c = Composite {
            name: "held".to_string(),
            entries: vec![Entry::new(cell(0, 0), 0, 1), Entry::new(cell(0, 0), 1, 1)],
        };
        let (starts, stops) = c.events_at(1);
        assert!(
            starts.is_empty() && stops.is_empty(),
            "a continuing member was restarted"
        );
    }

    #[test]
    fn a_run_starts_and_finishes() {
        let c = sequence();
        let mut p = Playback::default();

        let (starts, _) = p.start(&c);
        assert_eq!(starts, vec![cell(0, 0)]);
        assert!(p.is_running());

        p.advance(&c); // cycle 1
        let (starts, stops) = p.advance(&c); // cycle 2
        assert_eq!(starts, vec![cell(1, 0)]);
        assert_eq!(stops, vec![cell(0, 0)]);

        // Cycle 3 is past the end, so it stops and reports everything.
        let (starts, stops) = p.advance(&c);
        assert!(starts.is_empty());
        assert_eq!(stops.len(), 2);
        assert!(!p.is_running());
    }

    #[test]
    fn a_looping_run_wraps_instead_of_stopping() {
        let c = sequence();
        let mut p = Playback::default();
        p.set_looping(true);
        p.start(&c);

        p.advance(&c);
        p.advance(&c);
        let (starts, stops) = p.advance(&c);

        assert_eq!(p.cycle(), 0);
        assert!(p.is_running(), "a looping run should not stop");
        assert_eq!(starts, vec![cell(0, 0)]);
        // The member that was playing is stopped; the one starting again is not.
        assert_eq!(stops, vec![cell(1, 0)]);
    }

    #[test]
    fn stopping_reports_every_member() {
        let c = sequence();
        let mut p = Playback::default();
        p.start(&c);
        let stopped = p.stop(&c);
        assert_eq!(stopped.len(), 2);
        assert!(!p.is_running());
        assert_eq!(p.cycle(), 0);
    }

    #[test]
    fn an_empty_composite_does_not_run() {
        let c = Composite::default();
        let mut p = Playback::default();
        let (starts, stops) = p.start(&c);
        assert!(starts.is_empty() && stops.is_empty());
        assert!(!p.is_running());
    }

    #[test]
    fn advancing_when_not_running_does_nothing() {
        let c = sequence();
        let mut p = Playback::default();
        let (starts, stops) = p.advance(&c);
        assert!(starts.is_empty() && stops.is_empty());
        assert_eq!(p.cycle(), 0);
    }

    #[test]
    fn a_member_listed_twice_at_one_cycle_starts_once() {
        let c = Composite {
            name: "dup".to_string(),
            entries: vec![Entry::new(cell(0, 0), 0, 1), Entry::new(cell(0, 0), 0, 2)],
        };
        let (starts, _) = c.events_at(0);
        assert_eq!(starts, vec![cell(0, 0)]);
    }

    #[test]
    fn a_wrap_is_detected_from_the_position_going_backwards() {
        let mut c = CycleCounter::default();
        // The first reading cannot be a wrap.
        assert!(!c.update(100));
        assert!(!c.update(200));
        assert!(c.update(5), "a position going backwards is a wrap");
        assert!(!c.update(50));
    }

    #[test]
    fn a_repeated_position_is_not_a_wrap() {
        // A paused transport polls the same position; that must not count as a cycle.
        let mut c = CycleCounter::default();
        c.update(10);
        assert!(!c.update(10));
    }

    #[test]
    fn resetting_forgets_the_previous_position() {
        let mut c = CycleCounter::default();
        c.update(500);
        c.reset();
        // Nothing to compare against, so no wrap even though the position dropped.
        assert!(!c.update(1));
    }
}
