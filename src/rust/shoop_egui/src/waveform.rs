#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaveformBin {
    pub min: f32,
    pub max: f32,
}

pub fn waveform_bins(samples: &[f32], requested_bins: usize) -> Vec<WaveformBin> {
    if samples.is_empty() || requested_bins == 0 {
        return Vec::new();
    }

    let n_bins = requested_bins.min(samples.len());
    (0..n_bins)
        .map(|bin| {
            let start = bin * samples.len() / n_bins;
            let end = ((bin + 1) * samples.len() / n_bins).max(start + 1);
            samples[start..end]
                .iter()
                .copied()
                .filter(|sample| sample.is_finite())
                .fold(WaveformBin::default(), |mut result, sample| {
                    result.min = result.min.min(sample);
                    result.max = result.max.max(sample);
                    result
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_or_zero_width_has_no_bins() {
        assert!(waveform_bins(&[], 10).is_empty());
        assert!(waveform_bins(&[1.0], 0).is_empty());
    }

    #[test]
    fn short_input_is_not_upsampled() {
        assert_eq!(
            waveform_bins(&[-0.5, 0.25], 20),
            vec![
                WaveformBin {
                    min: -0.5,
                    max: 0.0,
                },
                WaveformBin {
                    min: 0.0,
                    max: 0.25,
                },
            ]
        );
    }

    #[test]
    fn long_input_preserves_extrema_per_bin() {
        assert_eq!(
            waveform_bins(&[-0.2, 0.5, -0.8, 0.1, 0.4, -0.3], 2),
            vec![
                WaveformBin {
                    min: -0.8,
                    max: 0.5,
                },
                WaveformBin {
                    min: -0.3,
                    max: 0.4,
                },
            ]
        );
    }

    #[test]
    fn non_finite_values_are_ignored() {
        assert_eq!(
            waveform_bins(&[f32::NAN, -0.4, f32::INFINITY], 1),
            vec![WaveformBin {
                min: -0.4,
                max: 0.0,
            }]
        );
    }
}
