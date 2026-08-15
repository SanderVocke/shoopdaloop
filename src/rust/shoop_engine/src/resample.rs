//! Offline resampling, for loading audio recorded at a different sample rate.
//!
//! is a control-path operation -- it runs when a file is loaded, never in a cycle --
//! so allocating is fine and no buffer is reused.
//!
//!
//! - The output is **exactly** `target_n_frames` long. A resampler's output length
//!   depends on its filter delay and rounding, so the tail is padded by repeating the
//!   under-produces.
//! - The ratio is clamped to `[1/16, 64]`, so an absurd request is bounded instead of
//!   trying to build a filter for it.

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use thiserror::Error;

const MIN_RATIO: f64 = 1.0 / 16.0;
const MAX_RATIO: f64 = 64.0;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResampleError {
    #[error(
        "interleaved input of {len} samples is not a whole number of {n_channels}-channel frames"
    )]
    Ragged { len: usize, n_channels: usize },
    #[error("resampler construction failed")]
    Construction,
}

/// Resamples interleaved audio to exactly `target_n_frames` frames.
///
/// zeroes its buffer there, though only the first `target_n_frames` floats of it
/// rather than all channels, which looks like an oversight and is not copied.
pub fn resample_interleaved(
    input: &[f32],
    n_channels: usize,
    target_n_frames: usize,
) -> Result<Vec<f32>, ResampleError> {
    if target_n_frames == 0 || n_channels == 0 {
        return Ok(Vec::new());
    }
    if !input.len().is_multiple_of(n_channels) {
        return Err(ResampleError::Ragged {
            len: input.len(),
            n_channels,
        });
    }

    let n_frames = input.len() / n_channels;
    if n_frames == 0 {
        return Ok(vec![0.0; target_n_frames * n_channels]);
    }

    // Nothing to do, and worth short-circuiting: a ratio of exactly 1 still costs a
    // filter pass and would reintroduce rounding at the tail.
    if n_frames == target_n_frames {
        return Ok(input.to_vec());
    }

    let ratio = (target_n_frames as f64 / n_frames as f64).clamp(MIN_RATIO, MAX_RATIO);

    let sinc_len = 48;
    let params = SincInterpolationParameters {
        sinc_len,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(ratio, 1.0, params, n_frames, n_channels)
        .map_err(|_| ResampleError::Construction)?;

    let planes = deinterleave(input, n_channels, n_frames);

    let mut out: Vec<Vec<f32>> = (0..n_channels)
        .map(|_| Vec::with_capacity(target_n_frames + sinc_len))
        .collect();

    // One pass over the input, then flushed, so the filter's tail is not lost.
    let produced = resampler
        .process(&planes, None)
        .map_err(|_| ResampleError::Construction)?;
    for (dst, src) in out.iter_mut().zip(&produced) {
        dst.extend_from_slice(src);
    }
    if let Ok(tail) = resampler.process_partial::<Vec<f32>>(None, None) {
        for (dst, src) in out.iter_mut().zip(&tail) {
            dst.extend_from_slice(src);
        }
    }

    Ok(interleave_to_length(&out, n_channels, target_n_frames))
}

fn deinterleave(input: &[f32], n_channels: usize, n_frames: usize) -> Vec<Vec<f32>> {
    let mut planes: Vec<Vec<f32>> = (0..n_channels)
        .map(|_| Vec::with_capacity(n_frames))
        .collect();
    for frame in input.chunks_exact(n_channels) {
        for (plane, s) in planes.iter_mut().zip(frame) {
            plane.push(*s);
        }
    }
    planes
}

/// Interleaves to exactly `target_n_frames`, padding by repeating the last frame.
///
/// Repeating rather than zero-filling: a resampler that comes up a frame or two short
/// leaves a click at the end if the gap is silence.
fn interleave_to_length(
    planes: &[Vec<f32>],
    n_channels: usize,
    target_n_frames: usize,
) -> Vec<f32> {
    let mut out = vec![0.0f32; target_n_frames * n_channels];
    for (c, plane) in planes.iter().enumerate().take(n_channels) {
        let available = plane.len().min(target_n_frames);
        for f in 0..available {
            out[f * n_channels + c] = plane[f];
        }
        let pad = plane
            .get(available.saturating_sub(1))
            .copied()
            .unwrap_or(0.0);
        for f in available..target_n_frames {
            out[f * n_channels + c] = pad;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::{check, let_assert};

    /// A ramp, interleaved across `n_channels` with each channel offset so a swapped
    /// channel shows up as a wrong value rather than passing unnoticed.
    fn ramp(n_frames: usize, n_channels: usize) -> Vec<f32> {
        (0..n_frames)
            .flat_map(|f| (0..n_channels).map(move |c| f as f32 + c as f32 * 1000.0))
            .collect()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn nothing_to_produce_gives_nothing() {
        let_assert!(Ok(out) = resample_interleaved(&ramp(10, 1), 1, 0));
        check!(out.is_empty());
        let_assert!(Ok(out) = resample_interleaved(&ramp(10, 1), 0, 10));
        check!(out.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn empty_input_gives_silence_of_the_requested_length() {
        let_assert!(Ok(out) = resample_interleaved(&[], 2, 8));
        check!(out.len() == 16);
        check!(out.iter().all(|&v| v == 0.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_ragged_input_is_refused() {
        // Five samples cannot be a whole number of stereo frames.
        check!(
            resample_interleaved(&[0.0; 5], 2, 8)
                == Err(ResampleError::Ragged {
                    len: 5,
                    n_channels: 2
                })
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn the_same_length_passes_straight_through() {
        let input = ramp(16, 2);
        let_assert!(Ok(out) = resample_interleaved(&input, 2, 16));
        check!(out == input);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn the_output_is_exactly_the_requested_length() {
        for (from, to) in [(100, 50), (100, 200), (100, 137), (7, 999), (999, 7)] {
            let_assert!(Ok(out) = resample_interleaved(&ramp(from, 2), 2, to));
            check!(out.len() == to * 2, "{from} -> {to}");
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn channels_stay_separate() {
        // Channel 1 is offset by 1000, so any bleed between them is obvious.
        let_assert!(Ok(out) = resample_interleaved(&ramp(200, 2), 2, 100));

        let ch0: Vec<f32> = out.iter().step_by(2).copied().collect();
        let ch1: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();

        check!(ch0.iter().all(|&v| v < 500.0));
        check!(ch1.iter().all(|&v| v > 500.0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_constant_signal_stays_constant() {
        // A ramp would be distorted by the filter's transient; a constant should come
        // back as itself apart from the edges.
        let input = vec![0.5f32; 400];
        let_assert!(Ok(out) = resample_interleaved(&input, 1, 200));

        check!(out.len() == 200);
        // Skipping the filter's settling region at each end.
        let middle = &out[40..160];
        check!(
            middle.iter().all(|&v| (v - 0.5).abs() < 0.01),
            "middle was {middle:?}"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn an_absurd_ratio_is_clamped_rather_than_refused() {
        // Far beyond the 64x bound, so the ratio is clamped and the result is padded
        // out to the requested length rather than failing.
        let_assert!(Ok(out) = resample_interleaved(&ramp(4, 1), 1, 4096));
        check!(out.len() == 4096);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn the_tail_repeats_rather_than_falling_silent() {
        // A constant input means any zero in the output is padding that should have
        // been a repeat.
        let input = vec![0.25f32; 300];
        let_assert!(Ok(out) = resample_interleaved(&input, 1, 150));
        check!(out.iter().rev().take(4).all(|&v| v != 0.0));
    }
}
