//! Checked musical lookback windows. Source admission and publication are separate.

use crate::{Error, Frame};
use core::ops::Range;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowSpec {
    pub reverse_start_cycle: Option<u32>,
    pub cycles_length: Option<u32>,
    pub go_to_cycle: Option<u32>,
    pub continue_recording: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Window {
    /// Retained input copied to logical zero, without changing its source times.
    pub source: Range<Frame>,
    pub frames: u64,
    /// The requested cycle/phase position; the post-mode decides how to use it.
    pub go_to_frame: u64,
    /// Explicit empty logical content after the available input.
    pub trailing_empty_frames: u64,
}

impl WindowSpec {
    /// Resolve against retained input ending at the effective cut. This does not
    /// authorize padding over a source gap or acquire any media ownership.
    pub fn resolve(
        self,
        retained: Range<Frame>,
        sync_frames: u64,
        sync_position: u64,
    ) -> Result<Window, Error> {
        let available = retained
            .end
            .0
            .checked_sub(retained.start.0)
            .ok_or(Error::InvalidFrame)?;
        let cycles = u64::from(self.cycles_length.unwrap_or(1).max(1));
        let go = u64::from(self.go_to_cycle.unwrap_or(0));
        let product = |value: u64| value.checked_mul(sync_frames).ok_or(Error::InvalidFrame);
        let add_phase = |value: u64| value.checked_add(sync_position).ok_or(Error::InvalidFrame);
        let (frames, end, position) = if sync_frames == 0 {
            (available, available, 0)
        } else {
            if sync_position > sync_frames {
                return Err(Error::InvalidFrame);
            }
            let position = add_phase(product(go)?)?;
            let frames = if self.reverse_start_cycle == Some(0) {
                sync_position
            } else if self.continue_recording {
                position
            } else {
                product(cycles)?
            };
            let before = match self.reverse_start_cycle {
                Some(0) => 0,
                Some(reverse) => add_phase(product(u64::from(reverse).saturating_sub(cycles))?)?,
                None if self.continue_recording => 0,
                None => position,
            };
            (frames, available.saturating_sub(before), position)
        };
        let start = end.saturating_sub(frames);
        Ok(Window {
            source: Frame(retained.start.0 + start)..Frame(retained.start.0 + end),
            frames,
            go_to_frame: position,
            trailing_empty_frames: frames - (end - start),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn target_cycles_and_current_phase_select_the_reference_lookback() {
        let target = WindowSpec {
            reverse_start_cycle: Some(4),
            cycles_length: Some(3),
            go_to_cycle: Some(1),
            continue_recording: false,
        };
        let result = assert_no_heap(|| target.resolve(Frame(1000)..Frame(2000), 100, 40)).unwrap();
        assert_eq!(result.source, Frame(1560)..Frame(1860));
        assert_eq!(
            (
                result.frames,
                result.go_to_frame,
                result.trailing_empty_frames
            ),
            (300, 140, 0)
        );
        let default_cycles = WindowSpec {
            reverse_start_cycle: Some(2),
            cycles_length: Some(2),
            ..WindowSpec::default()
        };
        let result = default_cycles
            .resolve(Frame(1000)..Frame(2000), 100, 40)
            .unwrap();
        assert_eq!(result.source, Frame(1760)..Frame(1960));
        assert_eq!((result.frames, result.go_to_frame), (200, 40));
    }

    #[shoop_test]
    fn unsynced_grab_continues_from_the_live_input_cut() {
        let spec = WindowSpec {
            cycles_length: Some(2),
            go_to_cycle: Some(1),
            continue_recording: true,
            ..WindowSpec::default()
        };
        let result = spec.resolve(Frame(1000)..Frame(2000), 100, 40).unwrap();
        assert_eq!(result.source, Frame(1860)..Frame(2000));
        assert_eq!(
            (
                result.frames,
                result.go_to_frame,
                result.trailing_empty_frames
            ),
            (140, 140, 0)
        );
    }

    #[shoop_test]
    fn partial_cycle_and_explicit_padding_never_invent_source_frames() {
        let partial = WindowSpec {
            reverse_start_cycle: Some(0),
            ..WindowSpec::default()
        };
        let result = partial.resolve(Frame(1800)..Frame(2000), 100, 40).unwrap();
        assert_eq!(result.source, Frame(1960)..Frame(2000));
        assert_eq!(result.frames, 40);
        let empty = partial.resolve(Frame(1800)..Frame(2000), 100, 0).unwrap();
        assert_eq!(empty.source, Frame(2000)..Frame(2000));
        assert_eq!(empty.frames, 0);
        let padded = WindowSpec {
            reverse_start_cycle: Some(3),
            cycles_length: Some(3),
            ..WindowSpec::default()
        }
        .resolve(Frame(1800)..Frame(2000), 100, 0)
        .unwrap();
        assert_eq!(padded.source, Frame(1800)..Frame(2000));
        assert_eq!((padded.frames, padded.trailing_empty_frames), (300, 100));
        let outside = WindowSpec {
            reverse_start_cycle: Some(10),
            cycles_length: Some(2),
            ..WindowSpec::default()
        }
        .resolve(Frame(1800)..Frame(2000), 100, 40)
        .unwrap();
        assert_eq!(outside.source, Frame(1800)..Frame(1800));
        assert_eq!((outside.frames, outside.trailing_empty_frames), (200, 200));
    }

    #[shoop_test]
    fn raw_adoption_and_time_above_u32_keep_exact_source_coordinates() {
        let origin = u64::from(u32::MAX) + 100;
        let result = WindowSpec::default()
            .resolve(Frame(origin)..Frame(origin + 1000), 0, 0)
            .unwrap();
        assert_eq!(result.source, Frame(origin)..Frame(origin + 1000));
        assert_eq!(
            (
                result.frames,
                result.go_to_frame,
                result.trailing_empty_frames
            ),
            (1000, 0, 0)
        );
        let result = WindowSpec {
            reverse_start_cycle: Some(1),
            ..WindowSpec::default()
        }
        .resolve(Frame(origin)..Frame(origin + 1000), 128, 32)
        .unwrap();
        assert_eq!(result.source, Frame(origin + 840)..Frame(origin + 968));
    }

    #[shoop_test]
    fn invalid_ranges_phase_and_arithmetic_refuse_without_saturation() {
        let spec = WindowSpec {
            cycles_length: Some(2),
            ..WindowSpec::default()
        };
        assert_eq!(
            spec.resolve(Frame(10)..Frame(0), 128, 0),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            spec.resolve(Frame(0)..Frame(1000), 128, 129),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            spec.resolve(Frame(0)..Frame(1000), u64::MAX, 0),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            WindowSpec {
                go_to_cycle: Some(u32::MAX),
                ..spec
            }
            .resolve(Frame(0)..Frame(1000), u64::MAX / 2, 1),
            Err(Error::InvalidFrame)
        );
    }
}
