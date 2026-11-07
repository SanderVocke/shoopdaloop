//! MIDI 1.0 message validation and bounded output-recovery state.
//!
//! Packets are complete messages, not running-status byte streams. Hosts must
//! separate interleaved realtime bytes from SysEx before submitting packets.

use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    SysEx,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message<'a> {
    data: &'a [u8],
    kind: MessageKind,
}

impl<'a> Message<'a> {
    /// Check the byte limit before scanning payloads. Undefined statuses, running
    /// status, truncated messages, and interleaved packets are explicit errors.
    pub fn parse(data: &'a [u8], max_bytes: usize) -> Result<Self, Error> {
        if data.len() > max_bytes {
            return Err(Error::Capacity);
        }
        let status = *data.first().ok_or(Error::InvalidEvent)?;
        if status == 0xf0 {
            if data.len() < 2
                || data.last() != Some(&0xf7)
                || data[1..data.len() - 1].iter().any(|&byte| byte >= 0x80)
            {
                return Err(Error::InvalidEvent);
            }
            return Ok(Self {
                data,
                kind: MessageKind::SysEx,
            });
        }
        let len = match status {
            0x80..=0xbf | 0xe0..=0xef => 3,
            0xc0..=0xdf => 2,
            0xf1 | 0xf3 => 2,
            0xf2 => 3,
            0xf6 | 0xf8 | 0xfa..=0xfc | 0xfe | 0xff => 1,
            _ => return Err(Error::InvalidEvent),
        };
        if data.len() != len || data[1..].iter().any(|&byte| byte >= 0x80) {
            return Err(Error::InvalidEvent);
        }
        let channel = status & 0x0f;
        let kind = match status & 0xf0 {
            0x80 => MessageKind::NoteOff {
                channel,
                key: data[1],
                velocity: data[2],
            },
            0x90 if data[2] == 0 => MessageKind::NoteOff {
                channel,
                key: data[1],
                velocity: 0,
            },
            0x90 => MessageKind::NoteOn {
                channel,
                key: data[1],
                velocity: data[2],
            },
            0xb0 => MessageKind::ControlChange {
                channel,
                controller: data[1],
                value: data[2],
            },
            _ => MessageKind::Other,
        };
        Ok(Self { data, kind })
    }
    pub fn data(self) -> &'a [u8] {
        self.data
    }
    pub fn kind(self) -> MessageKind {
        self.kind
    }
}

/// MIDI overload recovery uses a separate reserved path: all-sound-off followed
/// by all-notes-off for each of 16 channels. Ordinary traffic must remain refused
/// while this state is pending. The sink acknowledges a packet only after it is
/// accepted; backpressure cannot skip a reset or restart an unfinished reset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResetState {
    next: u8,
    pending: bool,
}

impl ResetState {
    pub const EVENTS: usize = 32;
    pub const BYTES: usize = Self::EVENTS * 3;

    pub fn request(&mut self) {
        if !self.pending {
            self.next = 0;
            self.pending = true;
        }
    }
    pub fn pending(&self) -> bool {
        self.pending
    }
    pub fn remaining(&self) -> usize {
        if self.pending {
            Self::EVENTS - self.next as usize
        } else {
            0
        }
    }
    pub fn peek(&self) -> Option<[u8; 3]> {
        if !self.pending {
            return None;
        }
        Some([
            0xb0 | (self.next / 2),
            if self.next % 2 == 0 { 120 } else { 123 },
            0,
        ])
    }
    /// Must be called only after the downstream sink accepted `peek()`.
    pub fn acknowledge(&mut self) -> Result<(), Error> {
        if !self.pending {
            return Err(Error::InvalidState);
        }
        self.next += 1;
        if self.next as usize == Self::EVENTS {
            self.pending = false;
        }
        Ok(())
    }
}
