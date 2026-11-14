//! Application capture and projection; legacy communication remains independent.
use crate::{
    cells::Cells,
    endpoint::{protocol_mode, ControlEndpoint},
    node::EngineNode,
};
use alloc::{boxed::Box, sync::Arc, vec};
use rt_runtime::{snapshot, Error};
use shoop_loops::{runtime::LoopPlanner, Mode};
use shoop_rt_protocol::{
    codec,
    snapshot::{Chunk, Header, Record, Records, SnapshotDelivery, CHUNK_BYTES, HEADER},
    EngineMessage, LoopSlotId, SlotId, StateRecord, Telemetry,
};

pub(crate) struct Capture {
    publisher: snapshot::Publisher,
    bytes: Box<[u8]>,
    cells: Arc<Cells>,
    loop_generations: Box<[u32]>,
}
pub(crate) struct State {
    reader: snapshot::Reader,
    cache_scratch: Box<[u8]>,
    cache_sequence: u32,
    cache_epochs: Option<(u64, u64)>,
    cache_frames: Box<[Option<(LoopSlotId, u64)>]>,
    scratch: Box<[u8]>,
    accepted: Box<[u8]>,
    len: usize,
    sequence: u32,
    offset: usize,
    pub enabled: bool,
    delivery: SnapshotDelivery,
    requested: Option<u32>,
    reply: bool,
    completion: Option<u32>,
    sample_rate: u32,
    sent_frame: Option<u64>,
    pub configuration: Option<(u64, u64)>,
    pub confirmed: (u64, u64),
}
pub(crate) fn prepare(
    cells: Arc<Cells>,
    strips: usize,
    loops: usize,
    buses: usize,
    sample_rate: u32,
) -> Result<(Capture, State), Error> {
    let capacity =
        shoop_rt_protocol::snapshot::capacity(strips, loops, buses).ok_or(Error::Capacity)?;
    let (publisher, reader) = snapshot::channel(10, capacity)?;
    Ok((
        Capture {
            loop_generations: vec![0; loops].into_boxed_slice(),
            publisher,
            bytes: vec![0; capacity].into_boxed_slice(),
            cells,
        },
        State {
            reader,
            cache_scratch: vec![0; capacity].into_boxed_slice(),
            cache_sequence: 0,
            cache_epochs: None,
            cache_frames: vec![None; loops].into_boxed_slice(),
            scratch: vec![0; capacity].into_boxed_slice(),
            accepted: vec![0; capacity].into_boxed_slice(),
            len: 0,
            sequence: 0,
            offset: 0,
            enabled: false,
            delivery: Default::default(),
            requested: None,
            reply: false,
            completion: None,
            sample_rate,
            sent_frame: None,
            configuration: None,
            confirmed: (0, 0),
        },
    ))
}
impl snapshot::Observer<EngineNode, LoopPlanner> for Capture {
    fn capture(&mut self, context: snapshot::Capture<'_, EngineNode, LoopPlanner>) {
        let mut pos = HEADER;
        let mut records = 0;
        let (mut loops, mut strips, mut buses) = (0, 0, 0);
        let failed = core::cell::Cell::new(false);
        let mut emit = |record| {
            if failed.get() {
                return;
            }
            match record {
                Record::LoopLevels { .. } => loops += 1,
                Record::Telemetry(Telemetry::Strip { .. }) => strips += 1,
                Record::Telemetry(Telemetry::Bus { .. }) => buses += 1,
                _ => {}
            }
            match codec::encode_snapshot(&record, &mut self.bytes[pos..]) {
                Ok(n) => {
                    pos += n;
                    records += 1;
                }
                Err(_) => failed.set(true),
            }
        };
        let resolver = context.planner.resolver();
        for node in resolver.topology().nodes() {
            if resolver.topology().program(node.id).is_some() {
                continue;
            }
            let Ok(cut) = context.planner.cut(node.id, context.frame) else {
                failed.set(true);
                break;
            };
            let state = cut.state;
            let slot = state.id.slot() as usize;
            if self.loop_generations[slot] != state.id.generation() {
                self.cells.loops[slot].peaks.reset_snapshot();
                self.loop_generations[slot] = state.id.generation();
            }
            let id = LoopSlotId(SlotId::new(state.id.slot(), state.id.generation()));
            let mut next = None;
            let mut armed = false;
            for pending in resolver
                .pending_transitions()
                .filter(|p| p.request.target == state.id)
            {
                use shoop_loops::Transition;
                let mode = match pending.request.transition {
                    Transition::Stop => Mode::Stopped,
                    Transition::Play { .. } => Mode::Playing,
                    Transition::PlayDry { .. } => Mode::PlayingDryThroughWet,
                    Transition::Record { .. } | Transition::RecordPrefix { .. } => Mode::Recording,
                    Transition::ReplaceInput { .. } => Mode::Replacing,
                    Transition::Rerecord => Mode::RecordingDryIntoWet,
                    Transition::Adopt { mode, .. } => mode,
                    _ => continue,
                };
                armed |= mode.records() || mode == Mode::Replacing;
                let delay = pending.remaining_ticks.unwrap_or_else(|| {
                    pending
                        .effective
                        .0
                        .saturating_sub(context.frame.0)
                        .div_ceil(resolver.quantum().frames() as u64)
                });
                if let Ok(delay) = u32::try_from(delay) {
                    if next.is_none_or(|(old, _)| delay < old) {
                        next = Some((delay, protocol_mode(mode)));
                    }
                }
            }
            emit(Record::LoopLevels {
                id,
                gain: cut.gain,
                balance: cut.balance,
                repeat_sync: state.repeat_sync,
            });
            emit(Record::State(StateRecord::LoopTiming {
                loop_id: id,
                timing: crate::timing::state(cut),
            }));
            emit(Record::Telemetry(Telemetry::Loop {
                loop_id: id,
                mode: protocol_mode(state.mode),
                next_mode: next.map(|(_, m)| m),
                next_delay: next.map(|(d, _)| d),
                content_revision: state.content_revision.0,
                recording_armed: armed,
                capture_incomplete: state.capture_incomplete,
                timing_revision: state.timing_revision,
                length: if state.mode == Mode::Recording {
                    state.capture_frames
                } else {
                    state.content_length
                },
                position: state.position,
                peaks: self.cells.loops[state.id.slot() as usize]
                    .peaks
                    .snapshot(context.planner.display_channels(state.id)),
            }));
        }
        for (_, node) in context.nodes {
            node.snapshot_records(&mut emit);
        }
        if failed.get() {
            self.publisher.fail();
            return;
        }
        Header {
            frame: context.frame.0,
            graph: context.graph_epoch.0,
            topology: resolver.epoch().0,
            records,
            loops,
            strips,
            buses,
        }
        .write(&mut self.bytes);
        let _ = self.publisher.publish(&self.bytes[..pos]);
    }
}

impl State {
    pub fn reset(&mut self) {
        self.enabled = false;
        self.configuration = None;
        self.delivery = Default::default();
        self.requested = None;
        self.reply = false;
        self.completion = None;
        self.sent_frame = None;
        self.len = 0;
        self.offset = 0;
        self.sequence = 0;
    }
    pub fn observe(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.configuration = None;
        if !enabled {
            if self.reply || self.requested.is_some() {
                self.completion = Some(0);
            }
            self.offset = self.len;
            self.reply = false;
            self.requested = None;
        }
    }
    pub fn configure(&mut self, delivery: SnapshotDelivery) {
        self.delivery = delivery;
    }
    pub fn request(&mut self, after: u32) {
        if self.enabled {
            self.requested = Some(after);
        } else {
            self.completion = Some(0);
        }
    }
    fn flush_completion(&mut self, out: &mut shoop_rt_transport::carrier::Outbox) -> bool {
        if let Some(sequence) = self.completion {
            if out.push(EngineMessage::SnapshotReady { sequence }).is_err() {
                return false;
            }
            self.completion = None;
        }
        true
    }
    fn send_transfer(&mut self, out: &mut shoop_rt_transport::carrier::Outbox) {
        let lane = if self.reply {
            shoop_rt_protocol::Lane::Reliable
        } else {
            shoop_rt_protocol::Lane::Telemetry
        };
        while self.offset < self.len && out.available(lane) > 0 {
            let len = (self.len - self.offset).min(CHUNK_BYTES);
            let mut chunk = Chunk {
                sequence: self.sequence,
                total: self.len as u32,
                offset: self.offset as u32,
                len: len as u16,
                bytes: [0; CHUNK_BYTES],
            };
            chunk.bytes[..len].copy_from_slice(&self.accepted[self.offset..self.offset + len]);
            let message = if self.reply {
                EngineMessage::SnapshotReply(chunk)
            } else {
                EngineMessage::Snapshot(chunk)
            };
            if out.push(message).is_err() {
                return;
            }
            self.offset += len;
        }
        if self.offset == self.len && self.reply {
            self.reply = false;
            self.completion = Some(self.sequence);
            self.flush_completion(out);
        }
    }
}

impl ControlEndpoint {
    pub(crate) fn refresh_snapshot_timing(&mut self) {
        let state = &mut self.snapshots;
        let Some((sequence, len)) = state.reader.read(&mut state.cache_scratch) else {
            return;
        };
        if sequence <= state.cache_sequence {
            return;
        }
        let Some(header) = Header::read(&state.cache_scratch[..len]) else {
            return;
        };
        if (header.graph, header.topology) != state.confirmed {
            return;
        }
        let records = &state.cache_scratch[HEADER..len];
        if Records(records)
            .try_fold(0u32, |count, record| record.map(|_| count + 1))
            .ok()
            != Some(header.records)
        {
            return;
        }
        if state.cache_epochs != Some(state.confirmed) {
            state.cache_frames.fill(None);
        }
        for record in Records(records).flatten() {
            if let Record::State(StateRecord::LoopTiming { loop_id, timing }) = record {
                for model in [&mut self.model, &mut self.next] {
                    if let Some(entry) = model
                        .loops
                        .get_mut(loop_id.0.index as usize)
                        .and_then(Option::as_mut)
                        .filter(|entry| {
                            entry.id == loop_id
                                && entry.program.is_none()
                                && entry.timing.revision <= timing.revision
                                && entry.timing.content_revision <= timing.content_revision
                        })
                    {
                        entry.timing = timing;
                    }
                }
                if self
                    .model
                    .find_loop(loop_id)
                    .is_some_and(|entry| entry.program.is_none() && entry.timing == timing)
                {
                    state.cache_frames[loop_id.0.index as usize] = Some((loop_id, header.frame));
                }
            }
        }
        state.cache_sequence = sequence;
        state.cache_epochs = Some(state.confirmed);
    }
    pub(crate) fn snapshot_configuration_settled(&self) -> bool {
        self.snapshots.cache_epochs == Some((self.graph_epoch.0, self.topology_epoch.0))
    }
    pub(crate) fn timing_cached_after(&self, id: LoopSlotId, frame: u64) -> bool {
        self.snapshots.cache_epochs == Some(self.snapshots.confirmed)
            && self
                .snapshots
                .cache_frames
                .get(id.0.index as usize)
                .copied()
                .flatten()
                .is_some_and(|(owner, captured)| owner == id && captured > frame)
    }
    /// Confirmed timing refreshed independently of UI snapshot delivery.
    pub fn cached_loop_timing(&self, id: LoopSlotId) -> Option<shoop_rt_protocol::TimingState> {
        self.current_timing(*self.model.find_loop(id)?)
    }
    pub(crate) fn project_snapshots(&mut self, out: &mut shoop_rt_transport::carrier::Outbox) {
        if !self.connected {
            return;
        }
        let state = &mut self.snapshots;
        if !state.flush_completion(out) {
            return;
        }
        if !state.enabled || self.resync.is_some() || self.results.len() != 0 {
            return;
        }
        if state.offset < state.len {
            state.send_transfer(out);
            return;
        }
        if state.requested.is_none() && state.delivery == SnapshotDelivery::Pull {
            return;
        }
        let epochs = state.confirmed;
        if state.configuration != Some(epochs) {
            if out
                .push(EngineMessage::SnapshotConfiguration {
                    graph: epochs.0,
                    topology: epochs.1,
                })
                .is_err()
            {
                return;
            }
            state.configuration = Some(epochs);
        }
        let requested = state.requested.take();
        let candidate = state
            .reader
            .read(&mut state.scratch)
            .and_then(|(sequence, len)| {
                let header = Header::read(&state.scratch[..len])?;
                ((header.graph, header.topology) == epochs).then_some((sequence, len, header))
            });
        let Some((sequence, len, header)) = candidate else {
            if requested.is_some() {
                state.completion = Some(0);
                state.flush_completion(out);
            }
            return;
        };
        if let Some(after) = requested {
            if sequence <= after {
                state.completion = Some(sequence);
                state.flush_completion(out);
                return;
            }
        } else {
            let SnapshotDelivery::Push { max_hz } = state.delivery else {
                return;
            };
            let interval = u64::from(state.sample_rate)
                .div_ceil(u64::from(max_hz.get()))
                .max(1);
            if sequence <= state.sequence
                || state
                    .sent_frame
                    .is_some_and(|last| header.frame < last.saturating_add(interval))
            {
                return;
            }
        }
        state.accepted[..len].copy_from_slice(&state.scratch[..len]);
        if requested.is_none() {
            out.discard_push_snapshots();
        }
        state.len = len;
        state.offset = 0;
        state.sequence = sequence;
        state.reply = requested.is_some();
        state.sent_frame = Some(header.frame);
        state.send_transfer(out);
    }
    pub fn read_snapshot(&self, scratch: &mut [u8]) -> Option<(u32, usize)> {
        let (sequence, len) = self.snapshots.reader.read(scratch)?;
        let header = Header::read(&scratch[..len])?;
        ((header.graph, header.topology) == self.snapshots.confirmed).then_some((sequence, len))
    }
    pub fn snapshot_error(&self) -> Option<Error> {
        self.snapshots.reader.error()
    }
    pub fn snapshot_frame(&self) -> Option<u64> {
        self.snapshots.sent_frame
    }
    pub fn snapshot_capacity(&self) -> usize {
        self.snapshots.reader.capacity()
    }
}
