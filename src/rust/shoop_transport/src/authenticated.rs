//! Credential admission for provisioned embedded hosts, independent of native discovery.
use crate::stream::{ByteIo, IoError};
use core::time::Duration;

pub type Token = [u8; 32];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Timeout,
    Disconnected,
    Credential,
}

pub struct Handshake {
    token: Token,
    received: Token,
    written: usize,
    read: usize,
    server: bool,
    remaining: Duration,
    done: bool,
    failure: Option<Error>,
}
impl Handshake {
    pub fn new(token: Token, server: bool, timeout: Duration) -> Self {
        Self {
            token,
            received: [0; 32],
            written: 0,
            read: 0,
            server,
            remaining: timeout,
            done: false,
            failure: None,
        }
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
        for _ in 0..4 {
            let reading = if self.server {
                self.read < 32
            } else {
                self.written == 32
            };
            let result = if reading {
                let end = if self.server { 32 } else { 1 };
                io.try_read(&mut self.received[self.read..end])
            } else {
                let bytes: &[u8] = if self.server { &[1] } else { &self.token };
                io.try_write(&bytes[self.written..])
            };
            match result {
                Ok(0) => return Err(Error::Disconnected),
                Ok(count) => {
                    if reading {
                        self.read += count;
                    } else {
                        self.written += count;
                    }
                    if self.server && self.read == 32 && self.received != self.token {
                        return Err(Error::Credential);
                    }
                    if !self.server && self.read == 1 && self.received[0] != 1 {
                        return Err(Error::Credential);
                    }
                    self.done = if self.server {
                        self.written == 1
                    } else {
                        self.read == 1
                    };
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
