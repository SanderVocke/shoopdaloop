//! Checked musical lookback windows. Source admission and publication are separate.

use crate::{Error, Frame, Mode};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target {
    pub frames: u64,
    pub position: u64,
    pub next_transition_delay: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GestureSpec {
    pub synced: bool,
    pub cycles: u32,
    pub play_after_record: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Finish {
    pub mode: Mode,
    /// Skip this many sync boundaries before completing the growing take.
    pub delay: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Gesture {
    pub window: WindowSpec,
    pub mode: Mode,
    pub finish: Option<Finish>,
}

impl GestureSpec {
    /// Resolve target cycle and finish intent from authoritative state at the cut.
    /// Target selection and batch admission belong to the caller.
    pub fn resolve(self, sync_frames: u64, target: Option<Target>) -> Result<Gesture, Error> {
        if sync_frames == 0 {
            return Err(Error::MissingSync);
        }
        let narrow = |n| u32::try_from(n).map_err(|_| Error::InvalidFrame);
        let cycles = self.cycles.max(1);
        let (period, current) = target
            .map(|target| -> Result<(u32, u32), Error> {
                Ok((
                    narrow(target.frames.div_ceil(sync_frames).max(1))?,
                    narrow(target.position / sync_frames)?,
                ))
            })
            .transpose()?
            .unwrap_or((cycles, if self.synced { 0 } else { cycles - 1 }));
        let post_mode = if self.play_after_record {
            Mode::Playing
        } else {
            Mode::Stopped
        };
        let window = if self.synced {
            WindowSpec {
                reverse_start_cycle: Some(current.checked_add(period).ok_or(Error::InvalidFrame)?),
                cycles_length: Some(period),
                go_to_cycle: Some(current),
                continue_recording: false,
            }
        } else {
            WindowSpec {
                reverse_start_cycle: None,
                cycles_length: Some(current.checked_add(1).ok_or(Error::InvalidFrame)?),
                go_to_cycle: Some(current),
                continue_recording: true,
            }
        };
        let finish = if self.synced {
            None
        } else {
            Some(Finish {
                mode: post_mode,
                delay: match target {
                    Some(Target {
                        next_transition_delay: Some(delay),
                        ..
                    }) => delay,
                    Some(target) => {
                        narrow(target.frames.saturating_sub(target.position) / sync_frames)?
                    }
                    None => 0,
                },
            })
        };
        Ok(Gesture {
            window,
            mode: if self.synced {
                post_mode
            } else {
                Mode::Recording
            },
            finish,
        })
    }
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
    fn gesture_policy_resolves_target_cycle_and_unsynced_finish_at_the_authoritative_cut() {
        for synced in [false, true] {
            for play_after_record in [false, true] {
                let spec = GestureSpec {
                    synced,
                    cycles: 7,
                    play_after_record,
                };
                let target = Target {
                    frames: 300,
                    position: 140,
                    next_transition_delay: None,
                };
                let gesture = assert_no_heap(|| spec.resolve(100, Some(target))).unwrap();
                let window = gesture
                    .window
                    .resolve(Frame(1000)..Frame(2000), 100, 40)
                    .unwrap();
                let post = if play_after_record {
                    Mode::Playing
                } else {
                    Mode::Stopped
                };
                if synced {
                    assert_eq!(gesture.mode, post);
                    assert_eq!(gesture.finish, None);
                    assert_eq!(
                        (window.source, window.frames, window.go_to_frame),
                        (Frame(1560)..Frame(1860), 300, 140)
                    );
                } else {
                    assert_eq!(gesture.mode, Mode::Recording);
                    assert_eq!(
                        gesture.finish,
                        Some(Finish {
                            mode: post,
                            delay: 1
                        })
                    );
                    assert_eq!(
                        (window.source, window.frames, window.go_to_frame),
                        (Frame(1860)..Frame(2000), 140, 140)
                    );
                    let pending = spec
                        .resolve(
                            100,
                            Some(Target {
                                next_transition_delay: Some(5),
                                ..target
                            }),
                        )
                        .unwrap();
                    assert_eq!(
                        pending.finish,
                        Some(Finish {
                            mode: post,
                            delay: 5
                        })
                    );
                }
            }
        }
    }

    #[shoop_test]
    fn gesture_policy_clamps_apply_cycles_and_requires_sync_even_when_unsynced() {
        for synced in [false, true] {
            for cycles in [0, 1, 3] {
                let spec = GestureSpec {
                    synced,
                    cycles,
                    play_after_record: true,
                };
                assert_eq!(spec.resolve(0, None), Err(Error::MissingSync));
                let gesture = spec.resolve(100, None).unwrap();
                assert_eq!(gesture.window.cycles_length, Some(cycles.max(1)));
                let window = gesture
                    .window
                    .resolve(Frame(1000)..Frame(2000), 100, 40)
                    .unwrap();
                if synced {
                    assert_eq!(window.frames, u64::from(cycles.max(1)) * 100);
                    assert_eq!(window.go_to_frame, 40);
                } else {
                    assert_eq!(window.frames, u64::from(cycles.max(1) - 1) * 100 + 40);
                    assert_eq!(window.source.end, Frame(2000));
                    assert_eq!(
                        gesture.finish,
                        Some(Finish {
                            mode: Mode::Playing,
                            delay: 0
                        })
                    );
                }
            }
        }
    }

    #[shoop_test]
    fn gesture_policy_rounds_target_periods_and_refuses_unrepresentable_cycle_intent() {
        let spec = GestureSpec {
            synced: true,
            cycles: 1,
            play_after_record: false,
        };
        let target = Target {
            frames: 201,
            position: 200,
            next_transition_delay: None,
        };
        let gesture = spec.resolve(100, Some(target)).unwrap();
        assert_eq!(gesture.window.cycles_length, Some(3));
        assert_eq!(gesture.window.reverse_start_cycle, Some(5));
        assert_eq!(gesture.window.go_to_cycle, Some(2));
        assert_eq!(
            spec.resolve(
                1,
                Some(Target {
                    frames: u64::MAX,
                    ..target
                })
            ),
            Err(Error::InvalidFrame)
        );
        assert_eq!(
            spec.resolve(
                1,
                Some(Target {
                    frames: 1,
                    position: u64::from(u32::MAX),
                    ..target
                })
            ),
            Err(Error::InvalidFrame)
        );
    }

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
