use thiserror::Error;

pub const MAX_COMPENSATION_FRAMES: u32 = 768_000;
pub const MAX_RETAINED_MARGIN_FRAMES: u32 = 768_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecordingOffset(i32);

impl RecordingOffset {
    pub fn new(frames: i32) -> Result<Self, LatencyDomainError> {
        if frames.unsigned_abs() > MAX_COMPENSATION_FRAMES {
            return Err(LatencyDomainError::SignedValueExceedsMaximum(frames));
        }
        Ok(Self(frames))
    }

    pub const fn frames(self) -> i32 {
        self.0
    }

    pub fn checked_add(self, trim_frames: i32) -> Result<Self, LatencyDomainError> {
        let frames = self
            .0
            .checked_add(trim_frames)
            .ok_or(LatencyDomainError::FrameArithmeticOverflow)?;
        Self::new(frames)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RecordingOffsetAdjustment {
    #[default]
    Automatic,
    ManualOverride(i32),
    AutomaticPlusTrim(i32),
}

pub fn resolve_recording_offset(
    automatic: Option<RecordingOffset>,
    adjustment: RecordingOffsetAdjustment,
) -> Result<RecordingOffset, LatencyDomainError> {
    match adjustment {
        RecordingOffsetAdjustment::Automatic => {
            automatic.ok_or(LatencyDomainError::AutomaticOffsetUnavailable)
        }
        RecordingOffsetAdjustment::ManualOverride(frames) => RecordingOffset::new(frames),
        RecordingOffsetAdjustment::AutomaticPlusTrim(trim_frames) => automatic
            .ok_or(LatencyDomainError::AutomaticOffsetUnavailable)?
            .checked_add(trim_frames),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessorRenderAdvance(u32);

impl ProcessorRenderAdvance {
    pub fn new(frames: u32) -> Result<Self, LatencyDomainError> {
        if frames > MAX_COMPENSATION_FRAMES {
            return Err(LatencyDomainError::ValueExceedsMaximum(frames));
        }
        Ok(Self(frames))
    }

    pub const fn frames(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProcessorLatencyAdjustment {
    Automatic,
    #[default]
    ManualOverride,
    AutomaticPlusTrim,
}

pub fn resolve_processor_advance(
    automatic: Option<ProcessorRenderAdvance>,
    adjustment: ProcessorLatencyAdjustment,
    manual_or_trim_frames: i32,
) -> Result<ProcessorRenderAdvance, LatencyDomainError> {
    match adjustment {
        ProcessorLatencyAdjustment::Automatic => {
            automatic.ok_or(LatencyDomainError::AutomaticProcessorLatencyUnavailable)
        }
        ProcessorLatencyAdjustment::ManualOverride => {
            let frames = u32::try_from(manual_or_trim_frames)
                .map_err(|_| LatencyDomainError::NegativeProcessorLatency(manual_or_trim_frames))?;
            ProcessorRenderAdvance::new(frames)
        }
        ProcessorLatencyAdjustment::AutomaticPlusTrim => {
            let automatic =
                automatic.ok_or(LatencyDomainError::AutomaticProcessorLatencyUnavailable)?;
            let frames = i64::from(automatic.frames()) + i64::from(manual_or_trim_frames);
            let frames = u32::try_from(frames).map_err(|_| {
                LatencyDomainError::NegativeProcessorLatency(
                    i32::try_from(frames).unwrap_or(i32::MIN),
                )
            })?;
            ProcessorRenderAdvance::new(frames)
        }
    }
}

pub fn wet_recording_offset(
    recording_offset: RecordingOffset,
    processor_advance: ProcessorRenderAdvance,
) -> Result<RecordingOffset, LatencyDomainError> {
    let advance = i32::try_from(processor_advance.frames())
        .map_err(|_| LatencyDomainError::FrameArithmeticOverflow)?;
    recording_offset.checked_add(advance)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetentionWindow {
    before_frames: u32,
    after_frames: u32,
}

impl RetentionWindow {
    pub fn for_offset(offset: RecordingOffset) -> Self {
        let frames = offset.frames();
        if frames < 0 {
            Self {
                before_frames: frames.unsigned_abs(),
                after_frames: 0,
            }
        } else {
            Self {
                before_frames: 0,
                after_frames: frames as u32,
            }
        }
    }

    pub const fn before_frames(self) -> u32 {
        self.before_frames
    }

    pub const fn after_frames(self) -> u32 {
        self.after_frames
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureFrameMapping {
    capture_alignment_frames: i32,
}

impl CaptureFrameMapping {
    pub fn new(capture_alignment_frames: i32) -> Result<Self, LatencyDomainError> {
        RecordingOffset::new(capture_alignment_frames)?;
        Ok(Self {
            capture_alignment_frames,
        })
    }

    pub const fn capture_alignment_frames(self) -> i32 {
        self.capture_alignment_frames
    }

    pub fn raw_frame(self, logical_frame: i64) -> Result<i64, LatencyDomainError> {
        logical_frame
            .checked_add(i64::from(self.capture_alignment_frames))
            .ok_or(LatencyDomainError::FrameArithmeticOverflow)
    }

    pub fn logical_frame(self, raw_frame: i64) -> Result<i64, LatencyDomainError> {
        raw_frame
            .checked_sub(i64::from(self.capture_alignment_frames))
            .ok_or(LatencyDomainError::FrameArithmeticOverflow)
    }

    pub fn raw_media_frame(
        self,
        logical_frame: i64,
        media_layout_offset: i64,
    ) -> Result<i64, LatencyDomainError> {
        self.raw_frame(
            logical_frame
                .checked_add(media_layout_offset)
                .ok_or(LatencyDomainError::FrameArithmeticOverflow)?,
        )
    }

    pub fn logical_media_frame(
        self,
        raw_frame: i64,
        media_layout_offset: i64,
    ) -> Result<i64, LatencyDomainError> {
        self.logical_frame(raw_frame)?
            .checked_sub(media_layout_offset)
            .ok_or(LatencyDomainError::FrameArithmeticOverflow)
    }

    pub fn processor_dispatch_frame(
        target_wet_frame: i64,
        render_advance_frames: u32,
    ) -> Result<i64, LatencyDomainError> {
        target_wet_frame
            .checked_sub(i64::from(render_advance_frames))
            .ok_or(LatencyDomainError::FrameArithmeticOverflow)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LatencyDomainError {
    #[error("latency value {0} exceeds the supported maximum")]
    ValueExceedsMaximum(u32),
    #[error("signed latency value {0} exceeds the supported maximum")]
    SignedValueExceedsMaximum(i32),
    #[error("latency frame arithmetic overflowed")]
    FrameArithmeticOverflow,
    #[error("automatic recording offset is unavailable; enter a manual value")]
    AutomaticOffsetUnavailable,
    #[error("automatic processor latency is unavailable; enter a manual value")]
    AutomaticProcessorLatencyUnavailable,
    #[error("processor latency cannot be negative: {0}")]
    NegativeProcessorLatency(i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
    shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

    #[shoop_wasm_test_support::shoop_test]
    fn capture_mapping_uses_one_checked_signed_alignment() {
        for alignment in [-17, 0, 23] {
            let mapping = CaptureFrameMapping::new(alignment).unwrap();
            assert_eq!(mapping.capture_alignment_frames(), alignment);
            assert_eq!(mapping.raw_frame(100).unwrap(), 100 + i64::from(alignment));
            assert_eq!(
                mapping.raw_media_frame(100, -9).unwrap(),
                91 + i64::from(alignment)
            );
            assert_eq!(
                mapping
                    .logical_media_frame(91 + i64::from(alignment), -9)
                    .unwrap(),
                100
            );
            assert_eq!(
                mapping.logical_frame(100 + i64::from(alignment)).unwrap(),
                100
            );
        }
        assert_eq!(
            CaptureFrameMapping::new(MAX_COMPENSATION_FRAMES as i32 + 1),
            Err(LatencyDomainError::SignedValueExceedsMaximum(
                MAX_COMPENSATION_FRAMES as i32 + 1
            ))
        );
        assert_eq!(
            CaptureFrameMapping::new(-(MAX_COMPENSATION_FRAMES as i32) - 1),
            Err(LatencyDomainError::SignedValueExceedsMaximum(
                -(MAX_COMPENSATION_FRAMES as i32) - 1
            ))
        );
        let mapping = CaptureFrameMapping::new(1).unwrap();
        assert_eq!(
            mapping.raw_frame(i64::MAX),
            Err(LatencyDomainError::FrameArithmeticOverflow)
        );
        assert_eq!(
            CaptureFrameMapping::processor_dispatch_frame(i64::MIN, 1),
            Err(LatencyDomainError::FrameArithmeticOverflow)
        );
        assert_eq!(
            CaptureFrameMapping::processor_dispatch_frame(100, 17).unwrap(),
            83
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn processor_latency_resolution_and_wet_derivation_are_checked() {
        let automatic = ProcessorRenderAdvance::new(17).unwrap();
        assert_eq!(
            resolve_processor_advance(Some(automatic), ProcessorLatencyAdjustment::Automatic, 99),
            Ok(automatic)
        );
        assert_eq!(
            resolve_processor_advance(
                Some(automatic),
                ProcessorLatencyAdjustment::ManualOverride,
                23
            ),
            ProcessorRenderAdvance::new(23)
        );
        assert_eq!(
            resolve_processor_advance(
                Some(automatic),
                ProcessorLatencyAdjustment::AutomaticPlusTrim,
                -5
            ),
            ProcessorRenderAdvance::new(12)
        );
        assert_eq!(
            resolve_processor_advance(
                Some(automatic),
                ProcessorLatencyAdjustment::ManualOverride,
                -1
            ),
            Err(LatencyDomainError::NegativeProcessorLatency(-1))
        );
        assert_eq!(
            resolve_processor_advance(
                Some(ProcessorRenderAdvance::new(3).unwrap()),
                ProcessorLatencyAdjustment::AutomaticPlusTrim,
                -4
            ),
            Err(LatencyDomainError::NegativeProcessorLatency(-1))
        );
        assert_eq!(
            resolve_processor_advance(None, ProcessorLatencyAdjustment::Automatic, 0),
            Err(LatencyDomainError::AutomaticProcessorLatencyUnavailable)
        );
        assert_eq!(
            wet_recording_offset(
                RecordingOffset::new(-5).unwrap(),
                ProcessorRenderAdvance::new(12).unwrap()
            ),
            RecordingOffset::new(7)
        );
        assert!(wet_recording_offset(
            RecordingOffset::new(MAX_COMPENSATION_FRAMES as i32).unwrap(),
            ProcessorRenderAdvance::new(1).unwrap()
        )
        .is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn recording_offset_resolution_checks_manual_override_and_trim() {
        let automatic = RecordingOffset::new(17).unwrap();
        assert_eq!(
            resolve_recording_offset(Some(automatic), RecordingOffsetAdjustment::Automatic),
            Ok(automatic)
        );
        assert_eq!(
            resolve_recording_offset(
                Some(automatic),
                RecordingOffsetAdjustment::ManualOverride(-23)
            ),
            RecordingOffset::new(-23)
        );
        assert_eq!(
            resolve_recording_offset(
                Some(automatic),
                RecordingOffsetAdjustment::AutomaticPlusTrim(-5)
            ),
            RecordingOffset::new(12)
        );
        assert_eq!(
            resolve_recording_offset(None, RecordingOffsetAdjustment::Automatic),
            Err(LatencyDomainError::AutomaticOffsetUnavailable)
        );
        assert_eq!(
            resolve_recording_offset(None, RecordingOffsetAdjustment::AutomaticPlusTrim(1)),
            Err(LatencyDomainError::AutomaticOffsetUnavailable)
        );
        assert_eq!(
            resolve_recording_offset(
                Some(RecordingOffset::new(MAX_COMPENSATION_FRAMES as i32).unwrap()),
                RecordingOffsetAdjustment::AutomaticPlusTrim(1)
            ),
            Err(LatencyDomainError::SignedValueExceedsMaximum(
                MAX_COMPENSATION_FRAMES as i32 + 1
            ))
        );
        assert_eq!(
            resolve_recording_offset(
                Some(RecordingOffset::new(1).unwrap()),
                RecordingOffsetAdjustment::AutomaticPlusTrim(i32::MAX)
            ),
            Err(LatencyDomainError::FrameArithmeticOverflow)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn capture_alignment_and_processor_advance_have_separate_domains() {
        assert_eq!(RecordingOffset::new(-7).unwrap().frames(), -7);
        assert_eq!(ProcessorRenderAdvance::new(7).unwrap().frames(), 7);
        assert_eq!(
            ProcessorRenderAdvance::new(MAX_COMPENSATION_FRAMES + 1),
            Err(LatencyDomainError::ValueExceedsMaximum(
                MAX_COMPENSATION_FRAMES + 1
            ))
        );
        assert_eq!(
            RetentionWindow::for_offset(RecordingOffset::new(-7).unwrap()),
            RetentionWindow {
                before_frames: 7,
                after_frames: 0,
            }
        );
        assert_eq!(
            RetentionWindow::for_offset(RecordingOffset::new(11).unwrap()),
            RetentionWindow {
                before_frames: 0,
                after_frames: 11,
            }
        );
    }
}
