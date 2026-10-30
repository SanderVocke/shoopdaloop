//! Chunked reads of a loop's current media.
//!
//! A read session exports the loop's playback content once and reads it
//! sequentially through a resource cursor into a prepared page buffer. A client
//! that reads chunks in order continues the session; reading another loop or
//! seeking backwards starts a new one. A session ends, releasing its export,
//! when its last chunk is answered.

use crate::endpoint::{
    completion_refusal, execution_outcome, ControlEndpoint, Purpose, RuntimeResult,
};
use crate::ring::Ring;
use rt_runtime::executor::Command;
use rt_runtime::resource::{Cursor, Reply as ResourceReply, Request as ResourceRequest};
use rt_runtime::transport::Outcome as RuntimeOutcome;
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
    Release,
    Export,
    Cursor,
    Read,
}

#[derive(Clone, Copy, Debug)]
struct Session {
    loop_id: LoopSlotId,
    midi: bool,
    export: ExportId,
    /// Frames for audio, bytes for MIDI.
    total: u64,
    cursor: Option<Cursor>,
    /// Media bytes consumed through the cursor.
    position: usize,
}

pub(crate) struct Bulk {
    queue: Ring<BulkRead>,
    active: Option<BulkRead>,
    /// The active read is being served by the current session.
    serving: bool,
    respond: bool,
    waiting: Waiting,
    session: Option<Session>,
    pub buffer: Option<Box<[u8]>>,
    samples: [f32; BULK_CHUNK_SAMPLES],
    bytes: [u8; BULK_CHUNK_BYTES],
    ready: Option<BulkChunk>,
    channels: usize,
}

impl Bulk {
    pub(crate) fn new(capacity: usize, page_bytes: usize, channels: usize) -> Self {
        Self {
            queue: Ring::new(capacity),
            active: None,
            serving: false,
            respond: true,
            waiting: Waiting::None,
            session: None,
            buffer: Some(vec![0; page_bytes].into_boxed_slice()),
            samples: [0.; BULK_CHUNK_SAMPLES],
            bytes: [0; BULK_CHUNK_BYTES],
            ready: None,
            channels,
        }
    }
    pub(crate) fn idle(&self) -> bool {
        self.active.is_none()
            && self.queue.len() == 0
            && self.ready.is_none()
            && self.waiting == Waiting::None
    }
    fn in_flight(&self) -> usize {
        self.queue.len() + usize::from(self.active.is_some()) + usize::from(self.ready.is_some())
    }
    pub(crate) fn disconnect(&mut self) {
        self.queue.clear();
        self.respond = false;
        self.ready = None;
    }
    fn media_bytes(&self, session: &Session) -> usize {
        if session.midi {
            session.total as usize
        } else {
            session.total as usize * self.channels * 4
        }
    }
    /// The media byte range of `read`'s chunk.
    fn range(&self, session: &Session, read: BulkRead) -> (usize, usize) {
        let offset = read.offset.min(session.total);
        if session.midi {
            let end = (offset + BULK_CHUNK_BYTES as u64).min(session.total);
            (offset as usize, end as usize)
        } else {
            let end = (offset + BULK_CHUNK_SAMPLES as u64).min(session.total);
            let frame = self.channels * 4;
            (offset as usize * frame, end as usize * frame)
        }
    }
    fn chunk(&self, session: &Session, read: BulkRead) -> BulkChunk {
        let (start, end) = self.range(session, read);
        let payload = if session.midi {
            BulkPayload::midi(&self.bytes[..end - start])
        } else {
            BulkPayload::audio(&self.samples[..(end - start) / (self.channels * 4)])
        };
        BulkChunk {
            id: read.id,
            offset: read.offset.min(session.total),
            total: session.total,
            payload: payload.expect("bounded chunk"),
        }
    }
    /// Copy the part of `bytes`, read at media byte `position`, that falls in the
    /// chunk of `read`.
    fn absorb(&mut self, session: &Session, read: BulkRead, position: usize, bytes: &[u8]) {
        let (start, end) = self.range(session, read);
        let channel = match read.selector {
            BulkSelector::Audio { channel } => channel as usize,
            BulkSelector::Midi => 0,
        };
        for index in 0..bytes.len() {
            let at = position + index;
            if at < start || at >= end {
                continue;
            }
            if session.midi {
                self.bytes[at - start] = bytes[index];
            } else if (at - start) % 4 == 0 && index + 4 <= bytes.len() {
                let sample = (at - start) / 4;
                if sample % self.channels == channel {
                    let word = [
                        bytes[index],
                        bytes[index + 1],
                        bytes[index + 2],
                        bytes[index + 3],
                    ];
                    self.samples[sample / self.channels] = f32::from_le_bytes(word);
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
        self.bulk.serving = false;
        if self.bulk.active.take().is_some() && self.bulk.respond {
            self.bulk.ready = Some(chunk);
        }
    }

    fn submit_bulk(&mut self, operation: Option<Operation>, waiting: Waiting) {
        let command = match operation {
            Some(operation) => {
                let at = self.at();
                self.loops.command(at, operation)
            }
            None => {
                let (Some(session), Some(bytes)) = (self.bulk.session, self.bulk.buffer.take())
                else {
                    return;
                };
                Command::resource(ResourceRequest::Read {
                    cursor: session.cursor.expect("session cursor"),
                    bytes,
                })
            }
        };
        let expects = operation.is_some();
        if self.submit(command, Purpose::Bulk, expects).is_ok() {
            self.bulk.waiting = waiting;
        }
    }

    fn release_session(&mut self) {
        if let Some(session) = self.bulk.session.take() {
            self.submit_bulk(
                Some(Operation::Release {
                    export: session.export,
                }),
                Waiting::Release,
            );
            if self.bulk.waiting != Waiting::Release {
                self.bulk.session = Some(session);
            }
        }
    }

    pub(crate) fn advance_bulk(&mut self) {
        for _ in 0..4 {
            if self.bulk.waiting != Waiting::None || self.bulk.ready.is_some() {
                return;
            }
            if self.bulk.active.is_none() {
                self.bulk.active = self.bulk.queue.pop();
                self.bulk.respond = true;
            }
            let Some(read) = self.bulk.active else {
                if !self.connected {
                    self.release_session();
                }
                return;
            };
            let midi = read.selector == BulkSelector::Midi;
            let Some(session) = self.bulk.session else {
                if self.model.find_loop(read.loop_id).is_none() {
                    self.finish_read(refused(read, Refusal::InvalidId));
                    continue;
                }
                self.submit_bulk(
                    Some(Operation::Export {
                        target: crate::endpoint::loop_id(read.loop_id),
                        selection: Selection::Playback,
                    }),
                    Waiting::Export,
                );
                return;
            };
            let (start, end) = self.bulk.range(&session, read);
            if !self.bulk.serving {
                if session.loop_id != read.loop_id
                    || session.midi != midi
                    || start < session.position
                {
                    self.release_session();
                    return;
                }
                self.bulk.serving = true;
            }
            if session.cursor.is_none() && start < end {
                self.submit_bulk(
                    Some(Operation::Cursor {
                        export: session.export,
                        midi,
                    }),
                    Waiting::Cursor,
                );
                return;
            }
            if session.position >= end || start >= end {
                let chunk = self.bulk.chunk(&session, read);
                self.finish_read(chunk);
                if end >= self.bulk.media_bytes(&session) {
                    self.release_session();
                }
                return;
            }
            self.submit_bulk(None, Waiting::Read);
            return;
        }
    }

    pub(crate) fn bulk_result(&mut self, result: RuntimeResult) {
        let waiting = self.bulk.waiting;
        let read = self.bulk.active;
        match result {
            RuntimeResult::Completion(outcome, Command::Resource(command), _) => {
                let reply = command.reply();
                let ResourceRequest::Read { cursor, bytes } = command.into_request() else {
                    return;
                };
                self.bulk.waiting = Waiting::None;
                match (outcome, reply, read, self.bulk.session) {
                    (
                        RuntimeOutcome::Applied,
                        Some(ResourceReply::Read { written, .. }),
                        Some(read),
                        Some(mut session),
                    ) if written > 0 => {
                        self.bulk
                            .absorb(&session, read, session.position, &bytes[..written]);
                        session.position += written;
                        session.cursor = Some(cursor);
                        self.bulk.session = Some(session);
                        self.bulk.buffer = Some(bytes);
                    }
                    (_, _, read, _) => {
                        self.bulk.buffer = Some(bytes);
                        if let Some(read) = read {
                            self.finish_read(refused(read, Refusal::Internal));
                        }
                        self.release_session();
                    }
                }
            }
            RuntimeResult::Completion(outcome, command, _) => {
                self.reclaim(command);
                if let Some(refusal) = completion_refusal(outcome) {
                    self.bulk.waiting = Waiting::None;
                    if let Some(read) = read {
                        self.finish_read(refused(read, refusal));
                    }
                }
            }
            RuntimeResult::Execution(execution) => {
                self.bulk.waiting = Waiting::None;
                match (waiting, execution.operation_result, read) {
                    (Waiting::Release, ..) => {}
                    (Waiting::Export, Some(Reply::Export { id, media, .. }), read) => {
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
                            cursor: None,
                            position: 0,
                        });
                    }
                    (Waiting::Cursor, Some(Reply::Cursor(cursor)), _) => {
                        if let Some(session) = &mut self.bulk.session {
                            session.cursor = Some(cursor);
                        }
                    }
                    (_, _, read) => {
                        if let Some(read) = read {
                            let refusal = match execution_outcome(&execution) {
                                Outcome::Refused(refusal) => refusal,
                                Outcome::Done => Refusal::Internal,
                            };
                            self.finish_read(refused(read, refusal));
                        }
                        self.release_session();
                    }
                }
            }
        }
    }

    pub(crate) fn flush_bulk(&mut self, out: &mut Outbox) {
        if let Some(chunk) = self.bulk.ready {
            if out.available(Lane::Bulk) > 0 {
                let _ = out.push(EngineMessage::Bulk(chunk));
                self.bulk.ready = None;
                self.advance_bulk();
            }
        }
    }
}
