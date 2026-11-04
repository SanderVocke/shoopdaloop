use rt_runtime::{Error, Quantum};

pub const MAX_BATCH_FRAMES: usize = 65_536;
pub const MAX_LATENCY_FRAMES: usize = 262_144;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    pub batch_quanta: usize,
    pub slack_quanta: usize,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            batch_quanta: 2,
            slack_quanta: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedTiming {
    quantum: Quantum,
    timing: Timing,
    batch_frames: usize,
    latency_frames: usize,
    slots: usize,
}

impl Timing {
    pub fn prepare(self, quantum: Quantum) -> Result<PreparedTiming, Error> {
        if self.batch_quanta == 0 {
            return Err(Error::InvalidConfig);
        }
        let total = self
            .batch_quanta
            .checked_add(self.slack_quanta)
            .ok_or(Error::Capacity)?;
        let batch_frames = self
            .batch_quanta
            .checked_mul(quantum.frames())
            .ok_or(Error::Capacity)?;
        let latency_frames = total.checked_mul(quantum.frames()).ok_or(Error::Capacity)?;
        if batch_frames > MAX_BATCH_FRAMES || latency_frames > MAX_LATENCY_FRAMES {
            return Err(Error::Capacity);
        }
        // One collecting slot plus all blocks intersecting the delay window.
        let slots = total
            .div_ceil(self.batch_quanta)
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        Ok(PreparedTiming {
            quantum,
            timing: self,
            batch_frames,
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
    pub fn batch_frames(self) -> usize {
        self.batch_frames
    }
    pub fn latency_frames(self) -> usize {
        self.latency_frames
    }
    pub fn slots(self) -> usize {
        self.slots
    }
}
