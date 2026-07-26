//! Reducing a loop's samples to something drawable.
//!
//! A loop can hold minutes of audio and a widget is a few hundred pixels wide, so the samples
//! have to be reduced. Min and max per column rather than an average: an average of a symmetric
//! waveform tends to zero and draws a flat line, which is exactly wrong for the thing a user
//! looks at a waveform to see.
//!
//! Separate from the drawing so the reduction can be tested. Getting it wrong produces a picture
//! that looks plausible and is not, which is the worst kind of bug to have in a display.

/// One column of a waveform: the extremes of the samples it covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub min: f32,
    pub max: f32,
}

impl Column {
    /// Peak magnitude, for a caller that wants one number.
    pub fn peak(&self) -> f32 {
        self.min.abs().max(self.max.abs())
    }
}

/// Reduces `samples` to at most `width` columns.
///
/// Fewer samples than columns gives one column per sample rather than stretching, so a very short
/// loop is drawn honestly as a few spikes instead of a smooth shape it does not have.
pub fn reduce(samples: &[f32], width: usize) -> Vec<Column> {
    if samples.is_empty() || width == 0 {
        return Vec::new();
    }
    let columns = width.min(samples.len());
    let mut out = Vec::with_capacity(columns);

    for c in 0..columns {
        // Computed from the column index rather than by stepping, so rounding cannot drift and
        // leave the last column short of the end.
        let start = c * samples.len() / columns;
        let end = ((c + 1) * samples.len() / columns).max(start + 1);
        let slice = &samples[start..end.min(samples.len())];

        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for &s in slice {
            if s < min {
                min = s;
            }
            if s > max {
                max = s;
            }
        }
        out.push(Column { min, max });
    }
    out
}

/// The largest magnitude across all columns, for scaling a drawing.
///
/// Returns `None` for silence, so a caller can avoid dividing by zero and draw a flat line
/// deliberately rather than by accident.
pub fn peak(columns: &[Column]) -> Option<f32> {
    let p = columns.iter().fold(0.0f32, |a, c| a.max(c.peak()));
    (p > 0.0).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_reduces_to_nothing() {
        assert!(reduce(&[], 100).is_empty());
        assert!(reduce(&[1.0, 2.0], 0).is_empty());
    }

    #[test]
    fn a_column_keeps_both_extremes() {
        // A single column over a signal that swings both ways.
        let cols = reduce(&[-0.5, 0.8, -0.2, 0.1], 1);
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].min, -0.5);
        assert_eq!(cols[0].max, 0.8);
    }

    #[test]
    fn a_symmetric_wave_does_not_reduce_to_a_flat_line() {
        // The whole point of min/max: averaging this would give zero everywhere.
        let samples: Vec<f32> = (0..1000)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let cols = reduce(&samples, 50);
        assert!(cols.iter().all(|c| c.min < -0.9 && c.max > 0.9));
        assert_eq!(peak(&cols), Some(1.0));
    }

    #[test]
    fn every_sample_is_covered_by_some_column() {
        // A single spike must survive reduction wherever it is, including at the very end.
        for spike in [0usize, 1, 499, 998, 999] {
            let mut samples = vec![0.0f32; 1000];
            samples[spike] = 1.0;
            let cols = reduce(&samples, 64);
            assert_eq!(
                peak(&cols),
                Some(1.0),
                "a spike at {spike} was lost in reduction"
            );
        }
    }

    #[test]
    fn fewer_samples_than_columns_gives_one_column_each() {
        let cols = reduce(&[0.1, 0.2, 0.3], 100);
        // Not stretched into a shape the data does not have.
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0].min, 0.1);
        assert_eq!(cols[2].max, 0.3);
    }

    #[test]
    fn the_requested_width_is_not_exceeded() {
        let samples = vec![0.5f32; 10_000];
        for width in [1, 7, 64, 333, 1000] {
            assert_eq!(reduce(&samples, width).len(), width);
        }
    }

    #[test]
    fn silence_has_no_peak() {
        let cols = reduce(&vec![0.0f32; 500], 20);
        // None rather than zero, so a caller does not divide by it.
        assert_eq!(peak(&cols), None);
        assert!(cols.iter().all(|c| c.min == 0.0 && c.max == 0.0));
    }

    #[test]
    fn peak_ignores_sign() {
        let cols = reduce(&[-0.9, 0.2], 1);
        assert_eq!(cols[0].peak(), 0.9);
        assert_eq!(peak(&cols), Some(0.9));
    }

    #[test]
    fn a_ramp_rises_monotonically_across_columns() {
        // A picture of a ramp should look like a ramp.
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let cols = reduce(&samples, 25);
        for pair in cols.windows(2) {
            assert!(
                pair[1].max >= pair[0].max,
                "the reduction is not monotonic over a ramp"
            );
        }
    }
}
