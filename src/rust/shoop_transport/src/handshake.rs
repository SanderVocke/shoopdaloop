//! Cooperative compatibility and connection-intent negotiation.
use crate::stream::{ByteIo, IoError};
use core::time::Duration;

pub const MAGIC: [u8; 8] = *b"SHOOP\0\0\x02";
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Intent {
    Connect,
    Inspect,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Timeout,
    Disconnected,
    Protocol,
}

pub struct Handshake {
    received: [u8; 9],
    written: usize,
    read: usize,
    server: bool,
    intent: Intent,
    remaining: Duration,
    done: bool,
    failure: Option<Error>,
}
impl Handshake {
    pub fn new(server: bool, timeout: Duration) -> Self {
        Self {
            received: [0; 9],
            written: 0,
            read: 0,
            server,
            intent: Intent::Connect,
            remaining: timeout,
            done: false,
            failure: None,
        }
    }
    pub fn inspect(timeout: Duration) -> Self {
        Self {
            intent: Intent::Inspect,
            ..Self::new(false, timeout)
        }
    }
    pub fn intent(&self) -> Intent {
        self.intent
    }
    pub fn remaining(&self) -> Duration {
        self.remaining
    }
    pub fn advance(&mut self, io: &mut impl ByteIo, elapsed: Duration) -> Result<bool, Error> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.done {
            return Ok(true);
        }
        self.remaining = self.remaining.saturating_sub(elapsed);
        let result = self.step(io);
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }
    fn step(&mut self, io: &mut impl ByteIo) -> Result<bool, Error> {
        if self.remaining.is_zero() {
            return Err(Error::Timeout);
        }
        let receive = if self.server { 9 } else { 8 };
        let transmit = if self.server { 8 } else { 9 };
        let mut message = [0; 9];
        message[..8].copy_from_slice(&MAGIC);
        message[8] = if self.intent == Intent::Inspect { 1 } else { 0 };
        for _ in 0..4 {
            let reading = if self.server {
                self.read < receive
            } else {
                self.written == transmit
            };
            let result = if reading {
                io.try_read(&mut self.received[self.read..receive])
            } else {
                io.try_write(&message[self.written..transmit])
            };
            match result {
                Ok(0) => return Err(Error::Disconnected),
                Ok(count) => {
                    if reading {
                        self.read += count;
                    } else {
                        self.written += count;
                    }
                    if self.read >= 8 && self.received[..8] != MAGIC {
                        return Err(Error::Protocol);
                    }
                    if self.server && self.read == 9 {
                        self.intent = match self.received[8] {
                            0 => Intent::Connect,
                            1 => Intent::Inspect,
                            _ => return Err(Error::Protocol),
                        };
                    }
                    self.done = self.read == receive && self.written == transmit;
                    if self.done {
                        return Ok(true);
                    }
                }
                Err(IoError::WouldBlock) => return Ok(false),
                Err(IoError::Interrupted) => continue,
                Err(IoError::Disconnected) => return Err(Error::Disconnected),
            }
        }
        Ok(false)
    }
}
