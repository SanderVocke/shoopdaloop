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
    use crate::audio_refs::RenderArena;
    use crate::resource::ResourceStore;
    use crate::{Config, Error, Quantum};
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn elision_preserves_reference_validation_and_work_without_touching_samples() {
        let resources = ResourceStore::new(
            Config::default().prepare().unwrap(),
            Quantum::new(32).unwrap(),
        )
        .unwrap();
        let mut arena = RenderArena::prepare(64, 0);
        assert_no_heap(|| {
            arena.reset().unwrap();
            let source = arena.allocate(32).unwrap();
            arena.output(source, 32).fill(2.);
            let reference = arena.transient(source, 32);
            let target = arena.allocate(32).unwrap();
            arena.accumulate(reference, target, 32, &resources).unwrap();
            assert_eq!(
                arena.output(target, 32),
                if ENABLED { &[2.; 32] } else { &[0.; 32] }
            );
            let mut output = [7.; 32];
            arena
                .read_into(reference, &mut output, &resources, false)
                .unwrap();
            assert_eq!(output, if ENABLED { [2.; 32] } else { [7.; 32] });
            assert_eq!(
                arena.peak(reference, 32, &resources).unwrap(),
                if ENABLED { 2. } else { 0. }
            );
            assert_eq!(arena.work.materialized, 2);
            assert_eq!(arena.work.sample_reads, 96);
            assert_eq!(arena.work.sample_writes, 160);
            let mut invalid = reference;
            invalid.spans[0].frames = 33;
            assert_eq!(
                arena.read_into(invalid, &mut output, &resources, false),
                Err(Error::InvalidShape)
            );
        });
    }
}
