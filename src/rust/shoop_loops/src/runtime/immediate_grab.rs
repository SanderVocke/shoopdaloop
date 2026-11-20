use super::*;
use crate::grab::{GestureSpec, Target, WindowSpec};

#[derive(Clone, Copy, Debug)]
pub enum ImmediateGrab {
    Window {
        sync: Option<LoopId>,
        window: WindowSpec,
        mode: Mode,
    },
    Gesture {
        sync: LoopId,
        target: Option<LoopId>,
        spec: GestureSpec,
    },
}

impl<S: Sample> LoopPlanner<S> {
    pub fn grab_immediate(
        &mut self,
        target: LoopId,
        grab: ImmediateGrab,
        frame: Frame,
        io: &mut PlannerResourceIo<'_, S>,
    ) -> Result<(), Error> {
        let row = self.row(target)?;
        let state = self.resolver.state(target).map_err(runtime_error)?;
        if frame != self.resolver.frame()
            || row.dry_wet
            || row.binding.channels == 0
            || row.active.is_some()
            || row.prepared.is_some()
            || row.interval.is_some()
            || row.aborted
            || !matches!(state.mode, Mode::Stopped | Mode::Playing)
            || self
                .shared
                .as_ref()
                .is_some_and(|s| s.live_for(target).is_some())
        {
            return Err(Error::InvalidState);
        }
        let (sync, window, mode, finish) = match grab {
            ImmediateGrab::Window { sync, window, mode } => (sync, window, mode, None),
            ImmediateGrab::Gesture { sync, target, spec } => {
                let sync_state = self.resolver.immediate_state(sync).map_err(runtime_error)?;
                let target = target
                    .map(|id| {
                        let state = self.resolver.immediate_state(id).map_err(runtime_error)?;
                        Ok::<_, Error>(Target {
                            frames: if state.mode == Mode::Recording {
                                state.capture_frames
                            } else {
                                state.content_length
                            },
                            position: state.position,
                            next_transition_delay: self.pending_musical_delay(id),
                        })
                    })
                    .transpose()?;
                let gesture = spec
                    .resolve(sync_state.content_length, target)
                    .map_err(runtime_error)?;
                let finish = gesture
                    .finish
                    .map(|mut finish| -> Result<_, Error> {
                        if self.resolver.immediate_pulse(sync).map_err(runtime_error)? {
                            finish.delay =
                                finish.delay.checked_add(1).ok_or(Error::InvalidFrame)?;
                        }
                        Ok(finish)
                    })
                    .transpose()?;
                (Some(sync), gesture.window, gesture.mode, finish)
            }
        };
        if !matches!(mode, Mode::Stopped | Mode::Playing | Mode::Recording)
            || window.continue_recording != (mode == Mode::Recording)
        {
            return Err(Error::InvalidParameter);
        }
        let source = sync;
        let sync = sync
            .map(|id| self.resolver.immediate_state(id).map_err(runtime_error))
            .transpose()?;
        let shared = self.shared.as_ref().ok_or(Error::InvalidOffset)?;
        let status = shared
            .status(row.history_source, self.graph_epoch, io)
            .map_err(|_| Error::InvalidOffset)?;
        if status.failure.is_some() || status.end != frame {
            return Err(Error::InvalidOffset);
        }
        let window = window
            .resolve(
                status.start..status.end,
                sync.map_or(0, |s| s.content_length),
                sync.map_or(0, |s| s.position),
            )
            .map_err(runtime_error)?;
        if window.frames > window.source.end.0 - window.source.start.0 && !status.startup_padding {
            return Err(Error::InvalidOffset);
        }
        let revision = state
            .revision_watermark
            .checked_next()
            .map_err(|_| Error::Exhausted)?;
        let mut decision = self
            .resolver
            .immediate_capture_decision(
                target,
                Transition::Adopt {
                    length: window.frames,
                    revision,
                    position: window.go_to_frame,
                    mode,
                },
            )
            .map_err(runtime_error)?;
        decision.state.capture_incomplete = false;
        let description = media::Description {
            owner: target,
            revision,
            frames: 0,
            channels: row.binding.channels as u32,
            auxiliary_channels: 0,
            midi: false,
        };
        let prefix = self.ledger.reserve(description)?;
        let mut snapshot = None;
        let mut finished = None;
        let mut writer = None;
        let mut windows = row.windows;
        for channel in windows
            .audio
            .iter_mut()
            .chain(core::iter::once(&mut windows.midi))
        {
            channel.start_offset = 0;
            channel.capture_alignment = 0;
            channel.raw_frames = None;
        }
        let result = (|| {
            snapshot = Some(self.shared.as_mut().unwrap().grab_audio_window(
                status.id,
                &window,
                self.resolver.quantum().frames(),
                frame,
                io,
            )?);
            if mode == Mode::Recording {
                finished = Some(self.ledger.reserve(media::Description {
                    revision: decision.state.capture_revision,
                    ..description
                })?);
                let shared = self.shared.as_mut().unwrap();
                let id = shared.begin_live_snapshots(
                    shared::Fence {
                        target,
                        revision: state.content_revision,
                        length: state.content_length,
                        timing_revision: row.timing_revision,
                        graph: self.graph_epoch,
                        topology: self.resolver.epoch(),
                    },
                    status.id,
                    window.frames,
                    [snapshot, None],
                    indexed_midi::WriteKind::Growing { maximum_frames: 0 },
                    shared.available_recorders(),
                    windows,
                    frame,
                    io,
                    false,
                    false,
                )?;
                writer = Some(id);
                if !shared.live_job(id)?.ready {
                    return Err(Error::Busy);
                }
                shared.start_live(id, window.frames, io)?;
            }
            let audio = snapshot.unwrap();
            let index = io.read_index(audio.id)?;
            self.ledger
                .publish_windowed(
                    io,
                    prefix,
                    media::Input {
                        description: media::Description {
                            frames: window.frames,
                            ..description
                        },
                        audio: Some(media::Source {
                            snapshot: audio,
                            index,
                        }),
                        auxiliary: None,
                        midi: None,
                    },
                    windows,
                )
                .map_err(|e| e.error)?;
            snapshot = None;
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(id) = writer {
                self.shared.as_mut().unwrap().cancel_live(id, io)?;
            }
            if let Some(id) = finished {
                self.ledger.retire(io, id)?;
            }
            if let Some(snapshot) = snapshot {
                io.release_snapshot(snapshot.id)?;
            }
            self.ledger.retire(io, prefix)?;
            return Err(error);
        }
        self.ledger.set_playback(io, target, Some(prefix))?;
        self.ledger.set_latest(io, target, Some(prefix))?;
        self.ledger.retire(io, prefix)?;
        let row = self.rows[target.slot() as usize]
            .as_mut()
            .expect("validated grab target");
        row.windows = windows;
        row.length_override = None;
        row.failure = None;
        if let Some(media) = finished {
            row.active = Some(Take {
                audio_only: true,
                publish_incomplete: true,
                started_at: frame,
                windows,
                writer,
                revision: decision.state.capture_revision,
                media,
                bank: row.next_bank,
                token: None,
                edit: None,
                snapshot_credit: None,
                auxiliary: None,
                midi: None,
            });
        }
        self.resolver.commit_immediate_capture(decision);
        self.install_grab_plan(target, source, finish)?;
        Ok(())
    }
}
