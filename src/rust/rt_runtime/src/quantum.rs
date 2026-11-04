//! Validated processing quantum and checked frame alignment.
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
    pub fn effective(self, requested: Frame, next: Frame) -> Result<Frame, Error> {
        if !self.aligned(next) {
            return Err(Error::InvalidFrame);
        }
        self.ceil(Frame(requested.0.max(next.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn alignment_never_selects_an_earlier_frame_and_checks_clock_overflow() {
        for frames in [1, 16, 32, 64, 128] {
            let quantum = Quantum::new(frames).unwrap();
            for frame in 0..1024_u64 {
                let expected = frame.div_ceil(frames as u64) * frames as u64;
                assert_eq!(quantum.ceil(Frame(frame)), Ok(Frame(expected)));
                assert!(quantum.aligned(Frame(expected)));
                assert_eq!(
                    quantum.effective(Frame(frame), Frame(1024)),
                    Ok(Frame(1024))
                );
            }
            assert_eq!(
                quantum.effective(Frame(0), Frame(1025)).is_ok(),
                frames == 1
            );
            assert_eq!(
                quantum.ceil(Frame(u64::MAX)),
                if frames == 1 {
                    Ok(Frame(u64::MAX))
                } else {
                    Err(Error::InvalidFrame)
                }
            );
        }
        assert_eq!(Quantum::default().frames(), 32);
        for frames in [0, 15, 33, 256] {
            assert_eq!(Quantum::new(frames), Err(Error::InvalidConfig));
        }
    }
}
