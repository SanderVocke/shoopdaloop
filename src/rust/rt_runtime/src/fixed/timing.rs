use crate::{Error, Frame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Quantum(u16);

impl Default for Quantum {
    fn default() -> Self {
        Self(32)
    }
}

impl Quantum {
    pub fn new(frames: usize) -> Result<Self, Error> {
        match frames {
            1 | 16 | 32 | 64 | 128 => Ok(Self(frames as u16)),
            _ => Err(Error::InvalidConfig),
        }
    }
    pub fn frames(self) -> usize {
        self.0 as usize
    }
    pub fn aligned(self, frame: Frame) -> bool {
        frame.0 % self.0 as u64 == 0
    }
    pub fn ceil(self, frame: Frame) -> Result<Frame, Error> {
        let q = self.0 as u64;
        let remainder = frame.0 % q;
        frame
            .0
            .checked_add(if remainder == 0 { 0 } else { q - remainder })
            .map(Frame)
            .ok_or(Error::InvalidFrame)
    }
    pub fn duration_ticks(self, frames: u64) -> Result<u64, Error> {
        if frames == 0 || frames % self.0 as u64 != 0 {
            return Err(Error::InvalidFrame);
        }
        Ok(frames / self.0 as u64)
    }
    pub fn effective(self, requested: Frame, next: Frame) -> Result<Frame, Error> {
        if !self.aligned(next) {
            return Err(Error::InvalidFrame);
        }
        self.ceil(Frame(requested.0.max(next.0)))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tick {
    pub frame: Frame,
    pub quantum: Quantum,
}
impl Tick {
    pub fn end(self) -> Result<Frame, Error> {
        self.frame.checked_add(self.quantum.frames())
    }
}
