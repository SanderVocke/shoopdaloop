//! Sample-kernel switch for diagnostic, non-audible compact benchmarks.

pub const ENABLED: bool = !cfg!(feature = "benchmark-no-sample-processing");

#[inline]
pub fn process(operation: impl FnOnce()) {
    if ENABLED {
        operation();
    } else {
        core::hint::black_box(operation);
    }
}

#[inline]
pub fn copy<T: Copy>(output: &mut [T], input: &[T]) {
    assert_eq!(output.len(), input.len());
    process(|| output.copy_from_slice(input));
}

#[inline]
pub fn fill<T: Copy>(output: &mut [T], value: T) {
    process(|| output.fill(value));
}

pub trait SampleSlice<T> {
    fn copy_audio_samples(&mut self, input: &[T]);
    fn fill_audio_samples(&mut self, value: T);
}

impl<T: Copy> SampleSlice<T> for [T] {
    #[inline]
    fn copy_audio_samples(&mut self, input: &[T]) {
        copy(self, input);
    }

    #[inline]
    fn fill_audio_samples(&mut self, value: T) {
        fill(self, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn elision_keeps_sample_storage_unchanged() {
        let mut output = [7.; 32];
        output.copy_audio_samples(&[0.25; 32]);
        assert_eq!(output, if ENABLED { [0.25; 32] } else { [7.; 32] });
        output.fill_audio_samples(0.);
        assert_eq!(output, if ENABLED { [0.; 32] } else { [7.; 32] });
    }
}
