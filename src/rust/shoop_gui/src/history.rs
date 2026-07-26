//! A fixed-length history of a polled value, for drawing a graph.
//!
//! The existing GUI graphs DSP load over time; a single number hides exactly the thing a load
//! graph is for, which is spotting that load spikes periodically rather than sitting high.
//!
//! A ring rather than a growing vector: this is fed from the UI loop forever, so it has to have a
//! bound. Separate from the drawing because the ordering is the part that is easy to get wrong --
//! a ring read in the wrong order draws a graph that is subtly time-reversed and still looks
//! plausible.

#[derive(Debug, Clone)]
pub struct History {
    samples: Vec<f32>,
    /// Where the next sample goes.
    next: usize,
    filled: bool,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity.max(1)],
            next: 0,
            filled: false,
        }
    }

    pub fn capacity(&self) -> usize {
        self.samples.len()
    }

    /// How many samples are available, which is less than capacity until it has filled once.
    pub fn len(&self) -> usize {
        if self.filled {
            self.samples.len()
        } else {
            self.next
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, value: f32) {
        self.samples[self.next] = value;
        self.next = (self.next + 1) % self.samples.len();
        if self.next == 0 {
            self.filled = true;
        }
    }

    /// Samples oldest first, which is left-to-right on a graph.
    pub fn iter(&self) -> impl Iterator<Item = f32> + '_ {
        let len = self.len();
        let start = if self.filled { self.next } else { 0 };
        (0..len).map(move |i| self.samples[(start + i) % self.samples.len()])
    }

    /// Largest value held, for scaling. `None` while empty or all zero.
    pub fn peak(&self) -> Option<f32> {
        let p = self.iter().fold(0.0f32, f32::max);
        (p > 0.0).then_some(p)
    }

    pub fn clear(&mut self) {
        self.next = 0;
        self.filled = false;
        for s in self.samples.iter_mut() {
            *s = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_history_is_empty() {
        let h = History::new(4);
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert_eq!(h.peak(), None);
        assert_eq!(h.iter().count(), 0);
    }

    #[test]
    fn samples_come_back_oldest_first() {
        let mut h = History::new(4);
        for v in [1.0, 2.0, 3.0] {
            h.push(v);
        }
        assert_eq!(h.iter().collect::<Vec<_>>(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_full_history_keeps_the_most_recent() {
        let mut h = History::new(3);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            h.push(v);
        }
        // The oldest two fell off, and the order is still oldest first.
        assert_eq!(h.len(), 3);
        assert_eq!(h.iter().collect::<Vec<_>>(), vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn wrapping_exactly_once_does_not_reverse_the_order() {
        // The case a ring gets wrong: `next` is back at zero and the whole buffer is valid.
        let mut h = History::new(3);
        for v in [1.0, 2.0, 3.0] {
            h.push(v);
        }
        assert_eq!(h.iter().collect::<Vec<_>>(), vec![1.0, 2.0, 3.0]);
        h.push(4.0);
        assert_eq!(h.iter().collect::<Vec<_>>(), vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn peak_is_the_largest_held_not_the_largest_ever() {
        let mut h = History::new(2);
        h.push(9.0);
        h.push(1.0);
        assert_eq!(h.peak(), Some(9.0));
        // Pushed out, so the peak drops with it.
        h.push(2.0);
        assert_eq!(h.peak(), Some(2.0));
    }

    #[test]
    fn all_zeroes_has_no_peak() {
        let mut h = History::new(3);
        h.push(0.0);
        h.push(0.0);
        // None rather than zero, so a caller does not divide by it.
        assert_eq!(h.peak(), None);
    }

    #[test]
    fn clearing_empties_it() {
        let mut h = History::new(3);
        for v in [1.0, 2.0, 3.0, 4.0] {
            h.push(v);
        }
        h.clear();
        assert!(h.is_empty());
        assert_eq!(h.iter().count(), 0);
    }

    #[test]
    fn a_zero_capacity_is_refused_rather_than_dividing_by_zero() {
        let mut h = History::new(0);
        assert_eq!(h.capacity(), 1);
        h.push(1.0);
        assert_eq!(h.iter().collect::<Vec<_>>(), vec![1.0]);
    }
}
