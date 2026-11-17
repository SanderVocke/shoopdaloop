//! Integer-picosecond clock events. Coincident edges are evaluated together.
#[derive(Clone, Copy, Debug)]
pub struct Edges {
    pub ps: u64,
    rising: u64,
    falling: u64,
}
impl Edges {
    pub fn rises(self, clock: usize) -> bool {
        self.rising & (1 << clock) != 0
    }
    pub fn falls(self, clock: usize) -> bool {
        self.falling & (1 << clock) != 0
    }
}
struct Clock {
    next: u64,
    half: u64,
    high: bool,
}
pub struct Scheduler {
    clocks: Vec<Clock>,
    pub ps: u64,
}
impl Scheduler {
    /// Each pair is (period, first rising edge), in picoseconds.
    pub fn new(clocks: &[(u64, u64)]) -> Self {
        assert!(!clocks.is_empty() && clocks.len() <= 64);
        Self {
            clocks: clocks
                .iter()
                .map(|&(period, phase)| {
                    assert!(period >= 2 && period % 2 == 0);
                    Clock {
                        next: phase,
                        half: period / 2,
                        high: false,
                    }
                })
                .collect(),
            ps: 0,
        }
    }
    pub fn next(&mut self) -> Edges {
        self.ps = self.clocks.iter().map(|c| c.next).min().unwrap();
        let mut edges = Edges {
            ps: self.ps,
            rising: 0,
            falling: 0,
        };
        for (i, clock) in self.clocks.iter_mut().enumerate() {
            if clock.next == self.ps {
                clock.high = !clock.high;
                if clock.high {
                    edges.rising |= 1 << i;
                } else {
                    edges.falling |= 1 << i;
                }
                clock.next = clock
                    .next
                    .checked_add(clock.half)
                    .expect("simulation time overflow");
            }
        }
        edges
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn phase_offsets_half_nanoseconds_and_coincident_edges() {
        let mut clocks = Scheduler::new(&[(10_000, 0), (13_000, 3_000)]);
        let mut audio_rises = Vec::new();
        let mut coincident = false;
        for _ in 0..100 {
            let e = clocks.next();
            if e.rises(1) {
                audio_rises.push(e.ps);
            }
            if e.ps == 55_000 {
                coincident = e.falls(0) && e.rises(1);
            }
        }
        assert_eq!(&audio_rises[..3], &[3_000, 16_000, 29_000]);
        assert!(coincident);
    }
}
