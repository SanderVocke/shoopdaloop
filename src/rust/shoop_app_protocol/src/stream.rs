//! Incremental encoding of one ordered service update. The sender retains an unsent record.
use alloc::vec::Vec;

use crate::*;
use shoop_transport::CodecError;

pub struct StreamEncoder {
    state: State,
}
enum State {
    Empty,
    One(WireMessage),
    Begin {
        authority: Authority,
        transfer: TransferId,
        revision: Option<Revision>,
        bytes: Vec<u8>,
    },
    Chunks {
        transfer: TransferId,
        snapshot: bool,
        bytes: Vec<u8>,
        offset: usize,
        index: u32,
    },
}
impl StreamEncoder {
    pub fn new(message: ServiceMessage, snapshot_transfer: TransferId) -> Result<Self, CodecError> {
        if !message.valid() || snapshot_transfer.0 == 0 {
            return Err(CodecError::Invalid);
        }
        let state = match message {
            ServiceMessage::CapturedImage { .. } => return Err(CodecError::Invalid),
            ServiceMessage::Persistence { authority, reply } => {
                State::One(WireMessage::Persistence { authority, reply })
            }
            ServiceMessage::Snapshot { authority, state } => State::Begin {
                authority,
                transfer: snapshot_transfer,
                revision: Some(state.revision),
                bytes: codec::encode_snapshot(&state)?,
            },
            ServiceMessage::Media {
                authority,
                transfer,
                data,
            } => State::Begin {
                authority,
                transfer,
                revision: None,
                bytes: codec::encode_media(&data)?,
            },
            ServiceMessage::Attached { authority } => {
                State::One(WireMessage::Attached { authority })
            }
            ServiceMessage::Rejected(reason) => State::One(WireMessage::Rejected(reason)),
            ServiceMessage::Accepted {
                authority,
                operation,
            } => State::One(WireMessage::Accepted {
                authority,
                operation,
            }),
            ServiceMessage::Finished { authority, result } => {
                State::One(WireMessage::Finished { authority, result })
            }
            ServiceMessage::Detached { authority } => {
                State::One(WireMessage::Detached { authority })
            }
            ServiceMessage::ShutdownComplete { authority, result } => {
                State::One(WireMessage::ShutdownComplete { authority, result })
            }
            ServiceMessage::MediaRefused {
                authority,
                transfer,
                reason,
            } => State::One(WireMessage::MediaRefused {
                authority,
                transfer,
                reason,
            }),
        };
        Ok(Self { state })
    }
    pub fn next_message(&mut self) -> Option<WireMessage> {
        let state = core::mem::replace(&mut self.state, State::Empty);
        match state {
            State::Empty => None,
            State::One(message) => Some(message),
            State::Begin {
                authority,
                transfer,
                revision,
                bytes,
            } => {
                let length = bytes.len() as u32;
                self.state = State::Chunks {
                    transfer,
                    snapshot: revision.is_some(),
                    bytes,
                    offset: 0,
                    index: 0,
                };
                Some(match revision {
                    Some(revision) => WireMessage::SnapshotBegin {
                        authority,
                        transfer,
                        revision,
                        bytes: length,
                    },
                    None => WireMessage::MediaBegin {
                        authority,
                        transfer,
                        bytes: length,
                    },
                })
            }
            State::Chunks {
                transfer,
                snapshot,
                bytes,
                offset,
                index: _,
            } if offset == bytes.len() => Some(if snapshot {
                WireMessage::SnapshotEnd { transfer }
            } else {
                WireMessage::MediaEnd { transfer }
            }),
            State::Chunks {
                transfer,
                snapshot,
                bytes,
                offset,
                index,
            } => {
                let end = (offset + CHUNK_BYTES).min(bytes.len());
                let chunk = bytes[offset..end].to_vec();
                self.state = State::Chunks {
                    transfer,
                    snapshot,
                    bytes,
                    offset: end,
                    index: index + 1,
                };
                Some(if snapshot {
                    WireMessage::SnapshotChunk {
                        transfer,
                        index,
                        bytes: chunk,
                    }
                } else {
                    WireMessage::MediaChunk {
                        transfer,
                        index,
                        bytes: chunk,
                    }
                })
            }
        }
    }
}
