#![forbid(unsafe_code)]

use rt_runtime::{
    graph::PortCounts,
    planner::Planner,
    processor::{ProcessBlock, Processor},
    transport::{
        self, Completion, ControlEndpoint, Lane, Outcome, RenderEndpoint, SubmitError, Ticket,
    },
    Epoch, Error, Frame, Sequence,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetId {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    SelectPreparedPreset {
        at: Frame,
        preset: PresetId,
        expected_revision: u32,
    },
    CancelPending {
        target: Sequence,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reply {
    Applied {
        frame: Frame,
        previous_revision: u32,
        revision: u32,
        preset: PresetId,
    },
    CancelRegistered {
        target: Sequence,
    },
    Refused(Error),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub action: Action,
    pub reply: Option<Reply>,
}

pub struct Client(ControlEndpoint<Request, (), ()>);

// Only the two render-owned halves access this copy-only publication mailbox.
struct Mailbox {
    armed: AtomicBool,
    frame: AtomicU64,
    slot: AtomicU32,
    revision: AtomicU32,
    acknowledged: AtomicBool,
    actual_frame: AtomicU64,
    previous_revision: AtomicU32,
    actual_revision: AtomicU32,
}

pub struct RenderControl {
    transport: RenderEndpoint<Request, (), ()>,
    mailbox: Arc<Mailbox>,
    revision: u32,
    pending: Option<Sequence>,
    cancelled: Option<Sequence>,
    callback: Option<(Frame, Frame)>,
    cycles: u64,
}

#[derive(Clone, Copy)]
struct Preset {
    live: f32,
    playback: f32,
    swap: bool,
}

pub struct Dsp {
    mailbox: Arc<Mailbox>,
    presets: [Preset; 2],
    preset: Preset,
    revision: u32,
}

pub fn prepare() -> Result<(Client, RenderControl, Dsp), Error> {
    let (mut client, render) = transport::channel(transport::TransportConfig {
        normal_slots: 1,
        recovery_slots: 1,
        telemetry_slots: 1,
        operations_per_cycle: 2,
        recovery_per_cycle: 1,
        publications_per_cycle: 2,
    })?;
    client.prepare_submissions()?;
    let mailbox = Arc::new(Mailbox {
        armed: AtomicBool::new(false),
        frame: AtomicU64::new(0),
        slot: AtomicU32::new(0),
        revision: AtomicU32::new(1),
        acknowledged: AtomicBool::new(false),
        actual_frame: AtomicU64::new(0),
        previous_revision: AtomicU32::new(1),
        actual_revision: AtomicU32::new(1),
    });
    let initial = Preset {
        live: 1.,
        playback: 1.,
        swap: false,
    };
    let alternative = Preset {
        live: 2.,
        playback: 0.5,
        swap: true,
    };
    Ok((
        Client(client),
        RenderControl {
            transport: render,
            mailbox: Arc::clone(&mailbox),
            revision: 1,
            pending: None,
            cancelled: None,
            callback: None,
            cycles: 0,
        },
        Dsp {
            mailbox,
            presets: [initial, alternative],
            preset: initial,
            revision: 1,
        },
    ))
}

impl Client {
    pub fn select(
        &mut self,
        at: Frame,
        preset: PresetId,
        expected_revision: u32,
    ) -> Result<Ticket, SubmitError<Request>> {
        self.0.submit_prepared(
            Lane::Normal,
            Request {
                action: Action::SelectPreparedPreset {
                    at,
                    preset,
                    expected_revision,
                },
                reply: None,
            },
        )
    }
    pub fn cancel_pending(&mut self, target: Sequence) -> Result<Ticket, SubmitError<Request>> {
        self.0.submit_prepared(
            Lane::Recovery,
            Request {
                action: Action::CancelPending { target },
                reply: None,
            },
        )
    }
    pub fn try_completion(&mut self) -> Option<Completion<Request, ()>> {
        let mut completion = self.0.try_completion()?;
        if completion.outcome == Outcome::Cancelled && completion.request.reply.is_none() {
            completion.request.reply = Some(Reply::Cancelled);
        }
        Some(completion)
    }
}

impl RenderControl {
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    fn settle(&mut self, reply: Reply, outcome: Outcome) -> Result<(), Error> {
        let operation = self.transport.next_operation().ok_or(Error::InvalidState)?;
        operation.request.reply = Some(reply);
        self.transport
            .complete(outcome, None)
            .map_err(|error| error.error)
    }

    pub fn begin_callback(&mut self, frame: Frame, frames: usize) -> Result<(), Error> {
        if self.callback.is_some() {
            return Err(Error::InvalidState);
        }
        if frames == 0 {
            return Ok(());
        }
        let end = Frame(frame.0.checked_add(frames as u64).ok_or(Error::Capacity)?);
        self.callback = Some((frame, end));
        self.cycles = self.cycles.checked_add(1).ok_or(Error::Capacity)?;
        self.transport.begin_cycle(frame, Epoch(0));
        for _ in 0..2 {
            let Some(operation) = self.transport.next_operation() else {
                break;
            };
            let (sequence, lane, action) =
                (operation.sequence, operation.lane, operation.request.action);
            if !self.transport.connected() {
                self.pending = None;
                self.settle(
                    Reply::Refused(Error::Disconnected),
                    Outcome::Rejected(Error::Disconnected),
                )?;
                continue;
            }
            match (lane, action) {
                (Lane::Recovery, Action::CancelPending { target }) => {
                    if target.0 >= sequence.0 {
                        self.settle(
                            Reply::Refused(Error::InvalidId),
                            Outcome::Rejected(Error::InvalidId),
                        )?;
                    } else {
                        self.cancelled = Some(target);
                        self.settle(Reply::CancelRegistered { target }, Outcome::Applied)?;
                    }
                }
                (
                    Lane::Normal,
                    Action::SelectPreparedPreset {
                        at,
                        preset,
                        expected_revision,
                    },
                ) => {
                    self.pending = Some(sequence);
                    let refusal = if self.cancelled == Some(sequence) {
                        self.pending = None;
                        self.cancelled = None;
                        self.settle(Reply::Cancelled, Outcome::Cancelled)?;
                        continue;
                    } else if at < frame {
                        Some(Error::InvalidEvent)
                    } else if preset.slot >= 2 || preset.generation != 1 {
                        Some(Error::InvalidId)
                    } else if expected_revision != self.revision {
                        Some(Error::InvalidState)
                    } else if self.revision == u32::MAX {
                        Some(Error::Capacity)
                    } else {
                        None
                    };
                    if let Some(error) = refusal {
                        self.pending = None;
                        self.settle(Reply::Refused(error), Outcome::Rejected(error))?;
                        continue;
                    }
                    if at < end {
                        self.mailbox.frame.store(at.0, Ordering::Relaxed);
                        self.mailbox.slot.store(preset.slot, Ordering::Relaxed);
                        self.mailbox
                            .revision
                            .store(expected_revision, Ordering::Relaxed);
                        self.mailbox.armed.store(true, Ordering::Release);
                    }
                    break;
                }
                _ => self.settle(
                    Reply::Refused(Error::InvalidEvent),
                    Outcome::Rejected(Error::InvalidEvent),
                )?,
            }
        }
        Ok(())
    }

    pub fn finish_callback(&mut self, processing: Result<(), Error>) -> Result<(), Error> {
        let Some(_) = self.callback.take() else {
            return Ok(());
        };
        if self.mailbox.acknowledged.swap(false, Ordering::Acquire) {
            let frame = Frame(self.mailbox.actual_frame.load(Ordering::Relaxed));
            let previous_revision = self.mailbox.previous_revision.load(Ordering::Relaxed);
            self.revision = self.mailbox.actual_revision.load(Ordering::Relaxed);
            let preset = PresetId {
                slot: self.mailbox.slot.load(Ordering::Relaxed),
                generation: 1,
            };
            self.pending = None;
            self.settle(
                Reply::Applied {
                    frame,
                    previous_revision,
                    revision: self.revision,
                    preset,
                },
                Outcome::Applied,
            )?;
        } else if let Some(error) = processing.err().or_else(|| {
            self.mailbox
                .armed
                .load(Ordering::Acquire)
                .then_some(Error::InvalidState)
        }) {
            self.mailbox.armed.store(false, Ordering::Release);
            if self.pending.take().is_some() {
                self.settle(Reply::Refused(error), Outcome::Rejected(error))?;
            }
        }
        Ok(())
    }
}

impl<L: Planner> Processor<L> for Dsp {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 4,
            audio_outputs: 2,
            ..PortCounts::default()
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_, L>) -> Result<(), Error> {
        let activation = self
            .mailbox
            .armed
            .load(Ordering::Acquire)
            .then(|| Frame(self.mailbox.frame.load(Ordering::Relaxed)));
        for offset in 0..block.frames {
            let frame = Frame(
                block
                    .frame
                    .0
                    .checked_add(offset as u64)
                    .ok_or(Error::Capacity)?,
            );
            if activation == Some(frame) {
                if self.revision != self.mailbox.revision.load(Ordering::Relaxed) {
                    return Err(Error::InvalidState);
                }
                let slot = self.mailbox.slot.load(Ordering::Relaxed) as usize;
                self.preset = *self.presets.get(slot).ok_or(Error::InvalidId)?;
                let previous_revision = self.revision;
                self.revision = self.revision.checked_add(1).ok_or(Error::Capacity)?;
                self.mailbox.armed.store(false, Ordering::Relaxed);
                self.mailbox.actual_frame.store(frame.0, Ordering::Relaxed);
                self.mailbox
                    .previous_revision
                    .store(previous_revision, Ordering::Relaxed);
                self.mailbox
                    .actual_revision
                    .store(self.revision, Ordering::Relaxed);
                self.mailbox.acknowledged.store(true, Ordering::Release);
            }
            for channel in 0..2 {
                let source = if self.preset.swap {
                    1 - channel
                } else {
                    channel
                };
                let live = block.audio.input(source).ok_or(Error::InvalidShape)?[offset];
                let playback = block.audio.input(source + 2).ok_or(Error::InvalidShape)?[offset];
                block.audio.output(channel).ok_or(Error::InvalidShape)?[offset] =
                    live * self.preset.live + playback * self.preset.playback;
            }
        }
        Ok(())
    }
}
