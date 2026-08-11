use std::sync::Arc;

const PYRAMID_BASE_BLOCK_SIZE: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaveformBin {
    pub min: f32,
    pub max: f32,
}

impl WaveformBin {
    fn include_sample(&mut self, sample: f32) {
        if sample.is_finite() {
            self.min = self.min.min(sample);
            self.max = self.max.max(sample);
        }
    }

    fn include_bin(&mut self, other: Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
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
            summarize_samples(&samples[start..end])
        })
        .collect()
}

fn summarize_samples(samples: &[f32]) -> WaveformBin {
    samples
        .iter()
        .copied()
        .fold(WaveformBin::default(), |mut result, sample| {
            result.include_sample(sample);
            result
        })
}

#[derive(Debug)]
pub(crate) struct WaveformPyramid {
    samples: Arc<[f32]>,
    levels: Vec<Vec<WaveformBin>>,
}

impl WaveformPyramid {
    pub(crate) fn new(samples: Arc<[f32]>) -> Self {
        let mut levels = Vec::new();
        let mut level: Vec<_> = samples
            .chunks(PYRAMID_BASE_BLOCK_SIZE)
            .map(summarize_samples)
            .collect();
        if !level.is_empty() {
            levels.push(level.clone());
        }
        while level.len() > 1 {
            level = level
                .chunks(2)
                .map(|children| {
                    children
                        .iter()
                        .copied()
                        .fold(WaveformBin::default(), |mut result, child| {
                            result.include_bin(child);
                            result
                        })
                })
                .collect();
            levels.push(level.clone());
        }
        Self { samples, levels }
    }

    pub(crate) fn matches(&self, samples: &Arc<[f32]>) -> bool {
        Arc::ptr_eq(&self.samples, samples)
    }

    pub(crate) fn bins(
        &self,
        offset: usize,
        sample_count: usize,
        requested_bins: usize,
    ) -> Vec<WaveformBin> {
        let end = offset.saturating_add(sample_count).min(self.samples.len());
        if offset >= end || requested_bins == 0 {
            return Vec::new();
        }
        let sample_count = end - offset;
        let n_bins = requested_bins.min(sample_count);
        (0..n_bins)
            .map(|bin| {
                let start = offset + bin * sample_count / n_bins;
                let end = offset + ((bin + 1) * sample_count / n_bins).max(bin + 1);
                self.summarize_range(start, end)
            })
            .collect()
    }

    fn summarize_range(&self, start: usize, end: usize) -> WaveformBin {
        let first_block = start.div_ceil(PYRAMID_BASE_BLOCK_SIZE);
        let last_block = end / PYRAMID_BASE_BLOCK_SIZE;
        if first_block >= last_block || self.levels.is_empty() {
            return summarize_samples(&self.samples[start..end]);
        }

        let mut result =
            summarize_samples(&self.samples[start..first_block * PYRAMID_BASE_BLOCK_SIZE]);
        self.include_block_range(&mut result, first_block, last_block);
        let suffix_start = last_block * PYRAMID_BASE_BLOCK_SIZE;
        for sample in self.samples[suffix_start..end].iter().copied() {
            result.include_sample(sample);
        }
        result
    }

    fn include_block_range(
        &self,
        result: &mut WaveformBin,
        mut first_block: usize,
        last_block: usize,
    ) {
        while first_block < last_block {
            let remaining_level = usize::BITS - 1 - (last_block - first_block).leading_zeros();
            let alignment_level = if first_block == 0 {
                remaining_level
            } else {
                first_block.trailing_zeros()
            };
            let level = remaining_level
                .min(alignment_level)
                .min((self.levels.len() - 1) as u32) as usize;
            result.include_bin(self.levels[level][first_block >> level]);
            first_block += 1 << level;
        }
    }
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
    fn output_size_is_bounded_for_large_recordings() {
        let samples = vec![0.5; 1_000_000];
        let bins = waveform_bins(&samples, 800);

        assert_eq!(bins.len(), 800);
        assert!(bins
            .iter()
            .all(|bin| *bin == WaveformBin { min: 0.0, max: 0.5 }));
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

    #[test]
    fn pyramid_queries_match_direct_binning() {
        let mut samples: Vec<_> = (0..1_037)
            .map(|index| ((index as f32 * 0.17).sin() * 1.2).clamp(-1.0, 1.0))
            .collect();
        samples[70] = f32::NAN;
        samples[511] = f32::INFINITY;
        let samples: Arc<[f32]> = Arc::from(samples);
        let pyramid = WaveformPyramid::new(Arc::clone(&samples));

        for (offset, sample_count) in [(0, 1_037), (1, 1_000), (63, 701), (64, 512), (129, 73)] {
            for requested_bins in [1, 7, 64, 200, 2_000] {
                assert_eq!(
                    pyramid.bins(offset, sample_count, requested_bins),
                    waveform_bins(&samples[offset..offset + sample_count], requested_bins),
                    "offset {offset}, count {sample_count}, bins {requested_bins}"
                );
            }
        }
    }

    #[test]
    fn pyramid_matches_only_the_source_allocation() {
        let samples: Arc<[f32]> = Arc::from([0.1, -0.2, 0.3]);
        let same_values: Arc<[f32]> = Arc::from([0.1, -0.2, 0.3]);
        let pyramid = WaveformPyramid::new(Arc::clone(&samples));

        assert!(pyramid.matches(&samples));
        assert!(!pyramid.matches(&same_values));
    }
}
