use rt_runtime::{fixed::Quantum, Error};

pub const MAX_BLOCK_FRAMES: usize = 65_536;
pub const MAX_LATENCY_FRAMES: usize = 262_144;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    pub block_quanta: usize,
    pub slack_quanta: usize,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            block_quanta: 2,
            slack_quanta: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedTiming {
    quantum: Quantum,
    timing: Timing,
    block_frames: usize,
    latency_frames: usize,
    slots: usize,
}

impl Timing {
    pub fn prepare(self, quantum: Quantum) -> Result<PreparedTiming, Error> {
        if self.block_quanta == 0 {
            return Err(Error::InvalidConfig);
        }
        let total = self
            .block_quanta
            .checked_add(self.slack_quanta)
            .ok_or(Error::Capacity)?;
        let block_frames = self
            .block_quanta
            .checked_mul(quantum.frames())
            .ok_or(Error::Capacity)?;
        let latency_frames = total.checked_mul(quantum.frames()).ok_or(Error::Capacity)?;
        if block_frames > MAX_BLOCK_FRAMES || latency_frames > MAX_LATENCY_FRAMES {
            return Err(Error::Capacity);
        }
        // One collecting slot plus all blocks intersecting the delay window.
        let slots = total
            .div_ceil(self.block_quanta)
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        Ok(PreparedTiming {
            quantum,
            timing: self,
            block_frames,
            latency_frames,
            slots,
        })
    }
}

impl PreparedTiming {
    pub fn quantum(self) -> Quantum {
        self.quantum
    }
    pub fn timing(self) -> Timing {
        self.timing
    }
    pub fn block_frames(self) -> usize {
        self.block_frames
    }
    pub fn latency_frames(self) -> usize {
        self.latency_frames
    }
    pub fn slots(self) -> usize {
        self.slots
    }
}
