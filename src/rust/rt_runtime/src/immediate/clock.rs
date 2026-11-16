#[cfg(target_has_atomic = "64")]
pub(super) struct FrameClock(core::sync::atomic::AtomicU64);

#[cfg(target_has_atomic = "64")]
impl FrameClock {
    pub fn new() -> Self {
        Self(core::sync::atomic::AtomicU64::new(0))
    }
    pub fn load(&self) -> u64 {
        self.0.load(core::sync::atomic::Ordering::Acquire)
    }
    pub fn store(&self, frame: u64) {
        self.0.store(frame, core::sync::atomic::Ordering::Release);
    }
}

#[cfg(not(target_has_atomic = "64"))]
pub(super) use split::FrameClock;

#[cfg(any(test, not(target_has_atomic = "64")))]
mod split {
    use core::sync::atomic::{AtomicU32, Ordering::SeqCst};

    pub(crate) struct FrameClock {
        sequence: AtomicU32,
        sequence_epoch: AtomicU32,
        low: AtomicU32,
        high: AtomicU32,
    }

    impl FrameClock {
        pub fn new() -> Self {
            Self {
                sequence: AtomicU32::new(0),
                sequence_epoch: AtomicU32::new(0),
                low: AtomicU32::new(0),
                high: AtomicU32::new(0),
            }
        }
        // Only the control reader retries; the single render writer never waits.
        pub fn load(&self) -> u64 {
            loop {
                let epoch = self.sequence_epoch.load(SeqCst);
                let sequence = self.sequence.load(SeqCst);
                if sequence & 1 != 0 {
                    core::hint::spin_loop();
                    continue;
                }
                let low = self.low.load(SeqCst);
                let high = self.high.load(SeqCst);
                if self.sequence.load(SeqCst) == sequence
                    && self.sequence_epoch.load(SeqCst) == epoch
                {
                    return (u64::from(high) << 32) | u64::from(low);
                }
            }
        }
        pub fn store(&self, frame: u64) {
            let sequence = self.sequence.load(SeqCst);
            self.sequence.store(sequence.wrapping_add(1), SeqCst);
            if sequence == u32::MAX - 1 {
                let epoch = self.sequence_epoch.load(SeqCst);
                self.sequence_epoch.store(epoch.wrapping_add(1), SeqCst);
            }
            self.low.store(frame as u32, SeqCst);
            self.high.store((frame >> 32) as u32, SeqCst);
            self.sequence.store(sequence.wrapping_add(2), SeqCst);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[shoop_wasm_test_support::shoop_test]
        fn frame_and_sequence_rollovers_preserve_all_bits() {
            let clock = FrameClock::new();
            assert_eq!(clock.load(), 0);
            clock.sequence.store(u32::MAX - 1, SeqCst);
            for frame in [u32::MAX as u64, 1 << 32, u64::MAX] {
                clock.store(frame);
                assert_eq!(clock.load(), frame);
            }
            assert_eq!(clock.sequence_epoch.load(SeqCst), 1);
        }

        #[cfg(not(target_arch = "wasm32"))]
        #[shoop_wasm_test_support::shoop_test(no_wasm = "native clock writer thread")]
        fn concurrent_reads_never_mix_frame_words() {
            let clock = alloc::sync::Arc::new(FrameClock::new());
            let writer_clock = clock.clone();
            let writer = std::thread::spawn(move || {
                for value in 1..=100_000u64 {
                    writer_clock.store(value << 32 | value);
                }
            });
            while !writer.is_finished() {
                let frame = clock.load();
                assert_eq!(frame >> 32, frame & u64::from(u32::MAX));
            }
            writer.join().unwrap();
            assert_eq!(clock.load(), 100_000 << 32 | 100_000);
        }
    }
}
