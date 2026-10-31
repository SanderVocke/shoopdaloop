//! Chunked reads of a loop's current media.
//!
//! A read session exports the loop's playback content once and hands a cursor
//! over its audio or MIDI to the bulk reader node, which copies it into the bulk
//! ring during rendering. Chunks are cut from the ring in order, as many per poll
//! as reads are queued and bytes have arrived. A client that reads in order
//! continues the session: audio channels interleaved, or one channel; reading
//! another loop, another channel or medium, or seeking backwards starts a new
//! one. A session ends, releasing its export, when its medium has been taken.

use crate::endpoint::RuntimeResult;
use crate::endpoint::{completion_refusal, execution_outcome, ControlEndpoint, Purpose};
use crate::node::EngineOp;
use crate::ring::Ring;
use rt_runtime::resource::Cursor;
use shoop_loops::media::ExportId;
use shoop_loops::runtime::{Operation, Reply, Selection};
use shoop_rt_protocol::carrier::Outbox;
use shoop_rt_protocol::{
    BulkChunk, BulkPayload, BulkRead, BulkSelector, EngineMessage, Lane, LoopSlotId, Outcome,
    Refusal, BULK_CHUNK_BYTES, BULK_CHUNK_SAMPLES,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Waiting {
    None,
    Export,
    Cursor,
    Start,
    Stop,
    Release,
}

#[derive(Clone, Copy, Debug)]
struct Session {
    loop_id: LoopSlotId,
    midi: bool,
    export: ExportId,
    /// Frames for audio, bytes for MIDI.
    total: u64,
    /// Bytes of the medium.
    bytes: u64,
    /// Medium bytes taken from the ring.
    position: u64,
    /// The reader node streams the medium.
    streaming: bool,
    /// The single channel being read, if not interleaved.
    channel: Option<u8>,
}

pub(crate) struct Bulk {
    queue: Ring<BulkRead>,
    active: Option<BulkRead>,
    respond: bool,
    waiting: Waiting,
    session: Option<Session>,
    /// Samples or bytes of the active read's chunk assembled so far.
    filled: usize,
    samples: [f32; BULK_CHUNK_SAMPLES],
    bytes: [u8; BULK_CHUNK_BYTES],
    scratch: Box<[u8]>,
    ready: Ring<BulkChunk>,
    channels: usize,
}

impl Bulk {
    pub(crate) fn new(capacity: usize, channels: usize) -> Self {
        Self {
            queue: Ring::new(capacity),
            active: None,
            respond: true,
            waiting: Waiting::None,
            session: None,
            filled: 0,
            samples: [0.; BULK_CHUNK_SAMPLES],
            bytes: [0; BULK_CHUNK_BYTES],
            scratch: vec![0; BULK_CHUNK_SAMPLES * 4 * channels.max(1)].into_boxed_slice(),
            ready: Ring::new(capacity),
            channels,
        }
    }
    pub(crate) fn idle(&self) -> bool {
        self.active.is_none()
            && self.queue.len() == 0
            && self.ready.len() == 0
            && self.waiting == Waiting::None
    }
    fn in_flight(&self) -> usize {
        self.queue.len() + usize::from(self.active.is_some()) + self.ready.len()
    }
    pub(crate) fn disconnect(&mut self) {
        self.queue.clear();
        self.respond = false;
        self.ready.clear();
    }
    fn frame_bytes(&self) -> u64 {
        self.channels as u64 * 4
    }
    /// Frames (or MIDI bytes) per chunk of `read`.
    fn chunk_units(&self, read: BulkRead) -> u64 {
        match read.selector {
            BulkSelector::Audio { .. } => BULK_CHUNK_SAMPLES as u64,
            BulkSelector::AudioFrames => (BULK_CHUNK_SAMPLES / self.channels.max(1)) as u64,
            BulkSelector::Midi => BULK_CHUNK_BYTES as u64,
        }
    }
    /// The medium byte range of `read`'s chunk.
    fn range(&self, session: &Session, read: BulkRead) -> (u64, u64) {
        let offset = read.offset.min(session.total);
        let end = (offset + self.chunk_units(read)).min(session.total);
        if session.midi {
            (offset, end)
        } else {
            (offset * self.frame_bytes(), end * self.frame_bytes())
        }
    }
    fn chunk(&self, session: &Session, read: BulkRead) -> BulkChunk {
        let payload = if session.midi {
            BulkPayload::midi(&self.bytes[..self.filled])
        } else {
            BulkPayload::audio(&self.samples[..self.filled])
        };
        BulkChunk {
            id: read.id,
            offset: read.offset.min(session.total),
            total: session.total,
            payload: payload.expect("bounded chunk"),
        }
    }
    /// Whether `session` can serve `read` from where it stands.
    fn continues(&self, session: &Session, read: BulkRead) -> bool {
        let channel = match read.selector {
            BulkSelector::Audio { channel } => Some(channel),
            _ => None,
        };
        session.loop_id == read.loop_id
            && session.midi == (read.selector == BulkSelector::Midi)
            && (session.position == 0 || session.channel == channel)
            && self.range(session, read).0 >= session.position
    }
    /// Copy the bytes of `data`, starting at medium byte `position`, into the
    /// active chunk.
    fn absorb(&mut self, read: BulkRead, position: u64, data: &[u8]) {
        match read.selector {
            BulkSelector::Midi => {
                self.bytes[self.filled..self.filled + data.len()].copy_from_slice(data);
                self.filled += data.len();
            }
            selector => {
                for (index, word) in data.chunks_exact(4).enumerate() {
                    let sample = (position / 4) as usize + index;
                    let keep = match selector {
                        BulkSelector::Audio { channel } => {
                            sample % self.channels == channel as usize
                        }
                        _ => true,
                    };
                    if keep {
                        let word = [word[0], word[1], word[2], word[3]];
                        self.samples[self.filled] = f32::from_le_bytes(word);
                        self.filled += 1;
                    }
                }
            }
        }
    }
}

fn refused(read: BulkRead, refusal: Refusal) -> BulkChunk {
    BulkChunk {
        id: read.id,
        offset: read.offset,
        total: 0,
        payload: BulkPayload::Refused(refusal),
    }
}

impl ControlEndpoint {
    pub(crate) fn deliver_bulk(&mut self, read: BulkRead, out: &mut Outbox) {
        let refusal = if !self.connected {
            Some(Refusal::NotConnected)
        } else if self.bulk.in_flight() >= self.limits.bulk_credits as usize {
            Some(Refusal::Overloaded)
        } else if self.model.find_loop(read.loop_id).is_none() {
            Some(Refusal::InvalidId)
        } else if matches!(read.selector, BulkSelector::Audio { channel } if channel as usize >= self.settings.channels)
        {
            Some(Refusal::InvalidValue)
        } else {
            self.bulk
                .queue
                .push(read)
                .err()
                .map(|_| Refusal::Overloaded)
        };
        if let Some(refusal) = refusal {
            let _ = out.push(EngineMessage::Bulk(refused(read, refusal)));
            return;
        }
        self.advance_bulk();
    }

    fn finish_read(&mut self, chunk: BulkChunk) {
        self.bulk.filled = 0;
        if self.bulk.active.take().is_some() && self.bulk.respond {
            let _ = self.bulk.ready.push(chunk);
        }
    }

    fn submit_bulk_loop(&mut self, operation: Operation, waiting: Waiting) {
        let at = self.at();
        let command = self.loops.command(at, operation);
        if self.submit(command, Purpose::Bulk, true).is_ok() {
            self.bulk.waiting = waiting;
        }
    }

    fn submit_reader(&mut self, operation: EngineOp, waiting: Waiting) {
        let at = self.at();
        let reader = self.pool.reader();
        if self.submit_op(at, reader, operation, Purpose::Bulk).is_ok() {
            self.bulk.waiting = waiting;
        }
    }

    /// Stop the reader if it streams, then release the session's export.
    fn end_session(&mut self) {
        let Some(session) = self.bulk.session else {
            return;
        };
        if session.streaming && session.position < session.bytes {
            self.submit_reader(EngineOp::ReadStop, Waiting::Stop);
            return;
        }
        self.cells.bulk.reset();
        self.submit_bulk_loop(
            Operation::Release {
                export: session.export,
            },
            Waiting::Release,
        );
    }

    pub(crate) fn advance_bulk(&mut self) {
        for _ in 0..self.bulk.queue.len() + 4 {
            if self.bulk.waiting != Waiting::None || self.bulk.ready.is_full() {
                return;
            }
            if self.bulk.active.is_none() {
                self.bulk.active = self.bulk.queue.pop();
                self.bulk.respond = true;
                self.bulk.filled = 0;
            }
            let Some(read) = self.bulk.active else {
                if !self.connected {
                    self.end_session();
                }
                return;
            };
            let Some(mut session) = self.bulk.session else {
                if self.model.find_loop(read.loop_id).is_none() {
                    self.finish_read(refused(read, Refusal::InvalidId));
                    continue;
                }
                self.submit_bulk_loop(
                    Operation::Export {
                        target: crate::endpoint::loop_id(read.loop_id),
                        selection: Selection::Playback,
                    },
                    Waiting::Export,
                );
                return;
            };
            if self.bulk.filled == 0 && !self.bulk.continues(&session, read) {
                self.end_session();
                return;
            }
            if session.position == 0 && self.bulk.filled == 0 {
                session.channel = match read.selector {
                    BulkSelector::Audio { channel } => Some(channel),
                    _ => None,
                };
            }
            let (start, end) = self.bulk.range(&session, read);
            if !session.streaming && start < end {
                self.bulk.session = Some(session);
                self.submit_bulk_loop(
                    Operation::Cursor {
                        export: session.export,
                        midi: session.midi,
                    },
                    Waiting::Cursor,
                );
                return;
            }
            if self.cells.bulk.failed() {
                self.finish_read(refused(read, Refusal::Internal));
                self.end_session();
                return;
            }
            if session.position < start {
                session.position +=
                    self.cells.bulk.skip((start - session.position) as usize) as u64;
            }
            while session.position >= start && session.position < end {
                let available = self.cells.bulk.available();
                // Audio is taken in whole samples; the reader may stop mid-sample.
                let available = if session.midi {
                    available
                } else {
                    available / 4 * 4
                };
                let wanted = ((end - session.position) as usize)
                    .min(self.bulk.scratch.len())
                    .min(available);
                let mut scratch = std::mem::take(&mut self.bulk.scratch);
                let count = self.cells.bulk.take(&mut scratch[..wanted]);
                self.bulk.absorb(read, session.position, &scratch[..count]);
                self.bulk.scratch = scratch;
                session.position += count as u64;
                if count == 0 {
                    break;
                }
            }
            self.bulk.session = Some(session);
            if session.position < end {
                return;
            }
            let chunk = self.bulk.chunk(&session, read);
            self.finish_read(chunk);
            if session.position >= session.bytes {
                self.end_session();
                return;
            }
        }
    }

    pub(crate) fn bulk_result(&mut self, result: RuntimeResult) {
        let waiting = self.bulk.waiting;
        let read = self.bulk.active;
        let outcome = match result {
            RuntimeResult::Completion(outcome, command, retired) => {
                self.reclaim(command);
                self.retire(retired);
                match completion_refusal(outcome) {
                    Some(refusal) => Err(refusal),
                    None => return,
                }
            }
            RuntimeResult::Execution(execution) => match execution_outcome(&execution) {
                Outcome::Refused(refusal) => Err(refusal),
                Outcome::Done => Ok(execution.operation_result),
            },
        };
        self.bulk.waiting = Waiting::None;
        match (waiting, outcome) {
            (Waiting::Export, Ok(Some(Reply::Export { id, media, .. }))) => {
                let Some(read) = read else {
                    let _ = self.submit(
                        self.loops
                            .command(self.at(), Operation::Release { export: id }),
                        Purpose::Silent,
                        true,
                    );
                    return;
                };
                let midi = read.selector == BulkSelector::Midi;
                let source = if midi {
                    media.view.midi
                } else {
                    media.view.audio
                };
                let total = match source {
                    Some(source) if midi => source.snapshot.description.bytes as u64,
                    Some(_) => media.view.description.frames,
                    None => 0,
                };
                self.bulk.session = Some(Session {
                    loop_id: read.loop_id,
                    midi,
                    export: id,
                    total,
                    bytes: if midi {
                        total
                    } else {
                        total * self.bulk.frame_bytes()
                    },
                    position: 0,
                    streaming: false,
                    channel: None,
                });
            }
            (Waiting::Cursor, Ok(Some(Reply::Cursor(cursor)))) => self.start_reader(cursor),
            (Waiting::Start, Ok(_)) => {
                if let Some(session) = &mut self.bulk.session {
                    session.streaming = true;
                }
            }
            (Waiting::Stop, Ok(_)) => {
                if let Some(session) = &mut self.bulk.session {
                    session.streaming = false;
                    session.position = session.bytes;
                }
                self.end_session();
            }
            (Waiting::Release, _) => self.bulk.session = None,
            (_, outcome) => {
                if let Some(read) = read {
                    let refusal = match outcome {
                        Err(refusal) => refusal,
                        Ok(_) => Refusal::Internal,
                    };
                    self.finish_read(refused(read, refusal));
                }
                if waiting == Waiting::Export {
                    return;
                }
                if waiting == Waiting::Stop {
                    if let Some(session) = &mut self.bulk.session {
                        session.streaming = false;
                        session.position = session.bytes;
                    }
                }
                self.end_session();
            }
        }
    }

    fn start_reader(&mut self, cursor: Cursor) {
        let Some(session) = self.bulk.session else {
            return;
        };
        self.cells.bulk.reset();
        self.submit_reader(
            EngineOp::ReadStart {
                cursor,
                bytes: session.bytes,
            },
            Waiting::Start,
        );
        if self.bulk.waiting != Waiting::Start {
            if let Some(read) = self.bulk.active {
                self.finish_read(refused(read, Refusal::Overloaded));
            }
            self.end_session();
        }
    }

    pub(crate) fn flush_bulk(&mut self, out: &mut Outbox) {
        for _ in 0..self.bulk.ready.capacity() + 1 {
            while let Some(chunk) = self.bulk.ready.front() {
                if out.available(Lane::Bulk) == 0 {
                    return;
                }
                let _ = out.push(EngineMessage::Bulk(chunk));
                self.bulk.ready.pop();
            }
            let before = self.bulk.in_flight();
            self.advance_bulk();
            if self.bulk.ready.len() == 0 || self.bulk.in_flight() == before {
                return;
            }
        }
    }
}
