//! The request queue. Structural requests that can go together are validated
//! against the staged model and become one transaction with one activation;
//! every other request runs as soon as nothing it depends on is pending, so
//! controls and transitions of untouched strips and loops do not wait behind a
//! transaction.
//!
//! Requests keep their order per target: a request waits while an earlier
//! request for the same strip, loop or the Master waits. Structural requests
//! keep their order among each other: once one has to wait, the later ones wait
//! too. A loop transition waits while its strip's structure or connections
//! change, as they decide what it records; an immediate one does not depend on
//! the sync source, so a pending sync change does not hold it back.

use crate::endpoint::ControlEndpoint;
use crate::jobs::Job;
use crate::model::Change;
use shoop_rt_protocol::{BusId, BusPort, EnginePort, Outcome, PortKind, Refusal, Request};

/// A pending transaction changes the slot's connections, routes or sync source.
pub(crate) const SOFT: u8 = 1;
/// A pending transaction creates or removes the slot's occupant.
pub(crate) const HARD: u8 = 2;

enum Staged {
    Change(Change),
    Done,
    Refused(Refusal),
    /// Valid only after the transaction being staged.
    Wait,
}

fn structural(request: &Request) -> bool {
    matches!(
        request,
        Request::CreateStrip { .. }
            | Request::RemoveStrip { .. }
            | Request::CreateLoop { .. }
            | Request::RemoveLoop { .. }
            | Request::LoopSync { .. }
            | Request::Connect { .. }
            | Request::Route { .. }
    )
}

fn raise(level: &mut u8, to: u8) {
    *level = (*level).max(to);
}

impl ControlEndpoint {
    /// Handle queued requests in order. With `stage`, and no transaction in
    /// progress, the structural requests that can go together start one.
    pub(crate) fn process_queue(&mut self, stage: bool) {
        let mut staging = stage && self.job.is_none();
        if staging {
            self.next.copy_from(&self.model);
            self.batch_len = 0;
        }
        self.blocked_strips.fill(false);
        self.blocked_loops.fill(false);
        self.blocked_master = false;
        self.blocked_all = false;
        let mut open = staging;
        for _ in 0..self.queue.len() {
            let Some((correlation, request)) = self.queue.pop() else {
                break;
            };
            if !self.queued(correlation, request, &mut staging, &mut open) {
                if structural(&request)
                    || matches!(
                        request,
                        Request::LoopClear { .. } | Request::SetSmoothing { .. }
                    )
                {
                    open = false;
                }
                self.block(request);
                let _ = self.queue.push((correlation, request));
            }
        }
        if staging && self.batch_len > 0 {
            self.job = Some(Job::batch(self.batch_len));
            self.advance_job();
        }
    }

    /// Handle one request; `false` keeps it queued.
    fn queued(
        &mut self,
        correlation: u64,
        request: Request,
        staging: &mut bool,
        open: &mut bool,
    ) -> bool {
        if self.blocked_all || self.blocked(request) {
            return false;
        }
        if structural(&request) {
            if !*open {
                return false;
            }
            return match self.stage(request) {
                Staged::Change(change) => {
                    if self.batch_len == self.batch.len() {
                        return false;
                    }
                    self.touch(change);
                    self.next.apply(change);
                    self.batch[self.batch_len] = (Some(correlation), change);
                    self.batch_len += 1;
                    true
                }
                Staged::Done => {
                    self.answer(correlation, Outcome::Done);
                    true
                }
                Staged::Refused(refusal) => {
                    self.answer(correlation, Outcome::Refused(refusal));
                    true
                }
                Staged::Wait => false,
            };
        }
        if matches!(
            request,
            Request::LoopClear { .. } | Request::SetSmoothing { .. }
        ) {
            if !*open || self.batch_len > 0 {
                return false;
            }
            if self.start(correlation, request).is_err() {
                return false;
            }
            if self.job.is_some() {
                *staging = false;
                *open = false;
                if let Request::LoopClear { loop_id } = request {
                    self.touched_loops[loop_id.0.index as usize] = HARD;
                }
            }
            return true;
        }
        self.start(correlation, request).is_ok()
    }

    /// Whether `request` depends on a pending transaction or on a request that
    /// waits before it.
    fn blocked(&self, request: Request) -> bool {
        // Structural requests depend only on waiting requests: the transaction
        // being staged validates them against its own result.
        const WAITING: u8 = u8::MAX;
        let strip = |index: u32, level: u8| {
            let index = index as usize;
            self.blocked_strips.get(index).copied().unwrap_or(false)
                || self.touched_strips.get(index).is_some_and(|t| *t >= level)
        };
        let loop_ = |index: u32, level: u8| {
            let index = index as usize;
            self.blocked_loops.get(index).copied().unwrap_or(false)
                || self.touched_loops.get(index).is_some_and(|t| *t >= level)
        };
        match request {
            Request::StripControl { strip: id, .. } | Request::InjectMidi { strip: id, .. } => {
                strip(id.0.index, HARD)
            }
            Request::LoopTransition { loop_id, delay, .. } => {
                let owner = self
                    .model
                    .loops
                    .get(loop_id.0.index as usize)
                    .copied()
                    .flatten()
                    .map(|entry| entry.strip.0.index);
                loop_(loop_id.0.index, if delay.is_some() { SOFT } else { HARD })
                    || owner.is_some_and(|index| strip(index, SOFT))
            }
            Request::LoopGain { loop_id, .. } | Request::LoopBalance { loop_id, .. } => {
                loop_(loop_id.0.index, HARD)
            }
            Request::LoopClear { loop_id } => loop_(loop_id.0.index, SOFT),
            Request::BusControl { .. } => self.blocked_master,
            Request::SetSmoothing { .. } => false,
            Request::CreateStrip { strip: id, .. } => strip(id.0.index, WAITING),
            Request::RemoveStrip { strip: id } => {
                strip(id.0.index, WAITING)
                    || self.model.loops.iter().flatten().any(|entry| {
                        entry.strip == id && self.blocked_loops[entry.id.0.index as usize]
                    })
            }
            Request::CreateLoop { loop_id, strip: id } => {
                loop_(loop_id.0.index, WAITING) || strip(id.0.index, WAITING)
            }
            Request::RemoveLoop { loop_id } | Request::LoopSync { loop_id, .. } => {
                loop_(loop_id.0.index, WAITING)
            }
            Request::Connect {
                port: EnginePort::Strip(port),
                ..
            } => strip(port.strip.0.index, WAITING),
            Request::Connect { .. } => self.blocked_master,
            Request::Route { port, .. } => {
                self.blocked_master || strip(port.strip.0.index, WAITING)
            }
        }
    }

    /// Later requests for the targets of a waiting `request` wait too.
    fn block(&mut self, request: Request) {
        let mut strip = |index: u32| {
            if let Some(blocked) = self.blocked_strips.get_mut(index as usize) {
                *blocked = true;
            }
        };
        match request {
            Request::StripControl { strip: id, .. }
            | Request::InjectMidi { strip: id, .. }
            | Request::CreateStrip { strip: id, .. }
            | Request::Connect {
                port: EnginePort::Strip(shoop_rt_protocol::StripPort { strip: id, .. }),
                ..
            } => strip(id.0.index),
            Request::RemoveStrip { strip: id } => {
                strip(id.0.index);
                for slot in 0..self.model.loops.len() {
                    if self.model.loops[slot].is_some_and(|entry| entry.strip == id) {
                        self.blocked_loops[slot] = true;
                    }
                }
            }
            Request::Route { port, .. } => {
                strip(port.strip.0.index);
                self.blocked_master = true;
            }
            Request::Connect { .. } | Request::BusControl { .. } => self.blocked_master = true,
            Request::CreateLoop { loop_id, .. }
            | Request::RemoveLoop { loop_id }
            | Request::LoopSync { loop_id, .. }
            | Request::LoopTransition { loop_id, .. }
            | Request::LoopGain { loop_id, .. }
            | Request::LoopBalance { loop_id, .. }
            | Request::LoopClear { loop_id } => {
                if let Some(blocked) = self.blocked_loops.get_mut(loop_id.0.index as usize) {
                    *blocked = true;
                }
            }
            Request::SetSmoothing { .. } => self.blocked_all = true,
        }
    }

    /// Record what a staged change affects, before it is applied.
    fn touch(&mut self, change: Change) {
        match change {
            Change::CreateStrip { strip, .. } => self.touched_strips[strip.0.index as usize] = HARD,
            Change::RemoveStrip { strip } => {
                self.touched_strips[strip.0.index as usize] = HARD;
                for slot in 0..self.next.loops.len() {
                    if let Some(entry) = self.next.loops[slot].filter(|entry| entry.strip == strip)
                    {
                        self.touched_loops[slot] = HARD;
                        self.touch_followers(entry.id);
                    }
                }
            }
            Change::CreateLoop { loop_id, .. } => {
                self.touched_loops[loop_id.0.index as usize] = HARD
            }
            Change::RemoveLoop { loop_id } => {
                self.touched_loops[loop_id.0.index as usize] = HARD;
                self.touch_followers(loop_id);
            }
            Change::Sync { loop_id, .. } => {
                raise(&mut self.touched_loops[loop_id.0.index as usize], SOFT)
            }
            Change::Connect { port, .. } => {
                raise(&mut self.touched_strips[port.strip.0.index as usize], SOFT)
            }
            Change::Route { strip, .. } => {
                raise(&mut self.touched_strips[strip.0.index as usize], SOFT);
                self.touched_master = SOFT;
            }
            Change::ConnectMaster { .. } => self.touched_master = SOFT,
        }
    }

    fn touch_followers(&mut self, source: shoop_rt_protocol::LoopSlotId) {
        for slot in 0..self.next.loops.len() {
            if self.next.loops[slot].is_some_and(|entry| entry.sync == Some(source)) {
                raise(&mut self.touched_loops[slot], SOFT);
            }
        }
    }

    pub(crate) fn clear_touched(&mut self) {
        self.touched_strips.fill(0);
        self.touched_loops.fill(0);
        self.touched_master = 0;
    }

    /// Validate a structural request against the staged model.
    fn stage(&self, request: Request) -> Staged {
        let next = &self.next;
        match request {
            Request::CreateStrip { strip, shape } => {
                let slot = strip.0.index as usize;
                if slot >= next.strips.len() {
                    Staged::Refused(Refusal::Capacity)
                } else if strip.0.generation == 0
                    || strip.0.generation <= next.strip_generations[slot]
                    || next.strips[slot].is_some()
                {
                    Staged::Refused(Refusal::InvalidId)
                } else if shape.audio_inputs as usize > self.settings.channels
                    || shape.audio_outputs as usize > self.settings.channels
                {
                    Staged::Refused(Refusal::Capacity)
                } else if self.model.strips[slot].is_some() {
                    Staged::Wait
                } else {
                    Staged::Change(Change::CreateStrip { strip, shape })
                }
            }
            Request::RemoveStrip { strip } => match next.strip(strip) {
                None => Staged::Refused(Refusal::InvalidId),
                Some(_) if self.model.strip(strip).is_none() => Staged::Wait,
                Some(_) => Staged::Change(Change::RemoveStrip { strip }),
            },
            Request::CreateLoop { loop_id, strip } => {
                let slot = loop_id.0.index as usize;
                if slot >= next.loops.len() {
                    Staged::Refused(Refusal::Capacity)
                } else if loop_id.0.generation == 0
                    || loop_id.0.generation <= next.loop_generations[slot]
                    || next.loops[slot].is_some()
                    || next.strip(strip).is_none()
                {
                    Staged::Refused(Refusal::InvalidId)
                } else if self.model.loops[slot].is_some() {
                    Staged::Wait
                } else {
                    Staged::Change(Change::CreateLoop { loop_id, strip })
                }
            }
            Request::RemoveLoop { loop_id } => match next.find_loop(loop_id) {
                None => Staged::Refused(Refusal::InvalidId),
                Some(_) if self.model.find_loop(loop_id).is_none() => Staged::Wait,
                Some(_) => Staged::Change(Change::RemoveLoop { loop_id }),
            },
            Request::LoopSync { loop_id, source } => match next.find_loop(loop_id) {
                None => Staged::Refused(Refusal::InvalidId),
                Some(entry) if entry.sync == source => Staged::Done,
                Some(_) => match source {
                    Some(source) if next.find_loop(source).is_none() => {
                        Staged::Refused(Refusal::InvalidId)
                    }
                    Some(source) if !next.sync_allowed(loop_id, source) => {
                        Staged::Refused(Refusal::InvalidValue)
                    }
                    _ => Staged::Change(Change::Sync { loop_id, source }),
                },
            },
            Request::Connect {
                port:
                    EnginePort::Bus(BusPort {
                        bus: BusId::Master,
                        index,
                    }),
                host,
                connected,
            } => {
                let channel = index as usize;
                if channel >= self.settings.master_channels {
                    return Staged::Refused(Refusal::InvalidValue);
                }
                let Some(host) = self.host_index(host, PortKind::AudioOut) else {
                    return Staged::Refused(Refusal::InvalidId);
                };
                let mask = next.master_outputs.current()[channel];
                if (mask & (1 << host) != 0) == connected {
                    Staged::Done
                } else {
                    Staged::Change(Change::ConnectMaster {
                        channel,
                        host,
                        connected,
                    })
                }
            }
            Request::Connect {
                port: EnginePort::Strip(port),
                host,
                connected,
            } => {
                let Some(strip) = next.strip(port.strip) else {
                    return Staged::Refused(Refusal::InvalidId);
                };
                let index = port.index as usize;
                let valid = match port.kind {
                    PortKind::AudioIn => index < strip.shape.audio_inputs as usize,
                    PortKind::AudioOut => index < strip.shape.audio_outputs as usize,
                    PortKind::MidiIn => index == 0 && strip.shape.midi_input,
                    PortKind::MidiOut => index == 0 && strip.shape.midi_output,
                };
                if !valid {
                    return Staged::Refused(Refusal::InvalidValue);
                }
                let Some(host) = self.host_index(host, port.kind) else {
                    return Staged::Refused(Refusal::InvalidId);
                };
                let mask = match port.kind {
                    PortKind::AudioIn => strip.inputs.current()[index],
                    PortKind::AudioOut => strip.outputs.current()[index],
                    PortKind::MidiIn => strip.midi_inputs,
                    PortKind::MidiOut => strip.midi_outputs,
                };
                if (mask & (1 << host) != 0) == connected {
                    Staged::Done
                } else {
                    Staged::Change(Change::Connect {
                        port,
                        host,
                        connected,
                    })
                }
            }
            Request::Route {
                port,
                bus:
                    BusPort {
                        bus: BusId::Master,
                        index,
                    },
                connected,
            } => match next.strip(port.strip) {
                None => Staged::Refused(Refusal::InvalidId),
                Some(entry)
                    if port.kind != PortKind::AudioOut
                        || port.index >= entry.shape.audio_outputs
                        || index as usize >= self.settings.master_channels =>
                {
                    Staged::Refused(Refusal::InvalidValue)
                }
                Some(entry) => {
                    let (channel, target) = (port.index as usize, index as usize);
                    let mask = entry.outputs.master[entry.outputs.active][channel];
                    if (mask & (1 << target) != 0) == connected {
                        Staged::Done
                    } else {
                        Staged::Change(Change::Route {
                            strip: port.strip,
                            channel,
                            target,
                            connected,
                        })
                    }
                }
            },
            _ => Staged::Refused(Refusal::Internal),
        }
    }
}
