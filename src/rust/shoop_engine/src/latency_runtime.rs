use shoop_latency::{CaptureFrameMapping, ProcessorRenderAdvance, RecordingOffset};
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

const PUBLICATION_READ_ATTEMPTS: usize = 16;
const NO_LATCHED_FRAME: u64 = u64::MAX;

pub(crate) fn cyclic_render_dispatch_position(
    media_position: i32,
    media_layout_offset: i32,
    capture_alignment_frames: i32,
    render_advance_frames: u32,
    logical_length: u32,
) -> Option<i32> {
    let mapping = CaptureFrameMapping::new(capture_alignment_frames).ok()?;
    let raw_position = i32::try_from(mapping.raw_frame(i64::from(media_position)).ok()?).ok()?;
    if render_advance_frames == 0 || logical_length == 0 {
        return Some(raw_position);
    }

    let selected_start = mapping.raw_frame(i64::from(media_layout_offset)).ok()?;
    let logical_position = i64::from(media_position) - i64::from(media_layout_offset);
    let dispatch_logical =
        (logical_position + i64::from(render_advance_frames)).rem_euclid(i64::from(logical_length));
    i32::try_from(selected_start + dispatch_logical).ok()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreparedLatency {
    recording_offset: RecordingOffset,
    wet_recording_offset: RecordingOffset,
    processor_advance: ProcessorRenderAdvance,
}

impl PreparedLatency {
    pub fn new(
        recording_offset: RecordingOffset,
        processor_advance: ProcessorRenderAdvance,
    ) -> Result<Self, shoop_latency::LatencyDomainError> {
        Ok(Self {
            recording_offset,
            wet_recording_offset: shoop_latency::wet_recording_offset(
                recording_offset,
                processor_advance,
            )?,
            processor_advance,
        })
    }

    pub const fn recording_offset(self) -> RecordingOffset {
        self.recording_offset
    }

    pub const fn wet_recording_offset(self) -> RecordingOffset {
        self.wet_recording_offset
    }

    pub const fn processor_advance(self) -> ProcessorRenderAdvance {
        self.processor_advance
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatchedLatency {
    pub values: PreparedLatency,
    pub operation_frame: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublishedEffectiveLatency {
    pub values: PreparedLatency,
    pub operation_frame: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EffectiveLatencyLatch {
    pending: Option<PreparedLatency>,
    latched: Option<LatchedLatency>,
}

impl EffectiveLatencyLatch {
    pub fn prepare(&mut self, values: PreparedLatency) {
        self.pending = Some(values);
    }

    pub const fn pending(&self) -> Option<PreparedLatency> {
        self.pending
    }

    pub const fn latched(&self) -> Option<LatchedLatency> {
        self.latched
    }

    pub fn latch(&mut self, operation_frame: u64) -> bool {
        let Some(values) = self.pending else {
            return false;
        };
        self.latched = Some(LatchedLatency {
            values,
            operation_frame,
        });
        true
    }
}

#[derive(Debug)]
pub struct AtomicEffectiveLatencyPublication {
    generation: AtomicU64,
    recording_offset: AtomicI32,
    processor_advance: AtomicU32,
    operation_frame: AtomicU64,
}

impl Default for AtomicEffectiveLatencyPublication {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            recording_offset: AtomicI32::new(0),
            processor_advance: AtomicU32::new(0),
            operation_frame: AtomicU64::new(NO_LATCHED_FRAME),
        }
    }
}

impl AtomicEffectiveLatencyPublication {
    pub fn publish_pending(&self, values: PreparedLatency) {
        self.publish(values, None);
    }

    pub fn publish_latched(&self, values: LatchedLatency) {
        self.publish(values.values, Some(values.operation_frame));
    }

    fn publish(&self, values: PreparedLatency, operation_frame: Option<u64>) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.recording_offset
            .store(values.recording_offset().frames(), Ordering::Relaxed);
        self.processor_advance
            .store(values.processor_advance().frames(), Ordering::Relaxed);
        self.operation_frame.store(
            operation_frame.unwrap_or(NO_LATCHED_FRAME),
            Ordering::Relaxed,
        );
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn read(&self) -> Option<PublishedEffectiveLatency> {
        for _ in 0..PUBLICATION_READ_ATTEMPTS {
            let before = self.generation.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let recording_offset = self.recording_offset.load(Ordering::Relaxed);
            let processor_advance = self.processor_advance.load(Ordering::Relaxed);
            let operation_frame = self.operation_frame.load(Ordering::Relaxed);
            let after = self.generation.load(Ordering::Acquire);
            if before == after {
                let values = PreparedLatency::new(
                    RecordingOffset::new(recording_offset)
                        .expect("published recording offset is validated"),
                    ProcessorRenderAdvance::new(processor_advance)
                        .expect("published processor advance is validated"),
                )
                .expect("published wet recording offset is validated");
                return Some(PublishedEffectiveLatency {
                    values,
                    operation_frame: (operation_frame != NO_LATCHED_FRAME)
                        .then_some(operation_frame),
                });
            }
            std::hint::spin_loop();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn effective_values_latch_once_and_publish_without_allocation() {
        let first = PreparedLatency::new(
            RecordingOffset::new(-7).unwrap(),
            ProcessorRenderAdvance::new(11).unwrap(),
        )
        .unwrap();
        let second = PreparedLatency::new(
            RecordingOffset::new(19).unwrap(),
            ProcessorRenderAdvance::new(23).unwrap(),
        )
        .unwrap();
        let mut latch = EffectiveLatencyLatch::default();
        latch.prepare(first);
        assert!(latch.latch(101));
        latch.prepare(second);
        assert_eq!(
            latch.latched(),
            Some(LatchedLatency {
                values: first,
                operation_frame: 101,
            })
        );

        let publication = AtomicEffectiveLatencyPublication::default();
        assert_no_alloc::assert_no_alloc(|| {
            publication.publish_pending(second);
            assert_eq!(
                publication.read(),
                Some(PublishedEffectiveLatency {
                    values: second,
                    operation_frame: None,
                })
            );
            publication.publish_latched(latch.latched().unwrap());
            assert_eq!(
                publication.read(),
                latch.latched().map(|latched| PublishedEffectiveLatency {
                    values: latched.values,
                    operation_frame: Some(latched.operation_frame),
                })
            );
        });
    }

    #[shoop_wasm_test_support::shoop_test]
    fn effective_publication_contention_fails_in_bounded_time() {
        let publication = AtomicEffectiveLatencyPublication::default();
        publication.generation.store(1, Ordering::Relaxed);
        assert_no_alloc::assert_no_alloc(|| assert_eq!(publication.read(), None));
    }
}
