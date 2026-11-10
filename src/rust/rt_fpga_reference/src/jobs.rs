//! Ordered sample work. This processor sees jobs and storage, never graph nodes.

use alloc::{collections::VecDeque, vec, vec::Vec};

use crate::{
    fixed::{Fixed, Ramp},
    storage::ViewId,
    Error,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BufferId(
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub usize,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Job {
    Input {
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
        destination: BufferId,
    },
    Silence {
        destination: BufferId,
    },
    ReadView {
        view: ViewId,
        position: u64,
        length: u64,
        destination: BufferId,
    },
    Gain {
        source: BufferId,
        destination: BufferId,
        ramp: Ramp,
    },
    Mix {
        left: BufferId,
        right: BufferId,
        destination: BufferId,
    },
    Capture {
        source: BufferId,
        view: ViewId,
    },
    Output {
        source: BufferId,
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
    },
}

/// A finite FIFO. This functional model emits a whole quantum before draining it.
pub struct JobQueue {
    jobs: VecDeque<Job>,
    capacity: usize,
}

impl JobQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            jobs: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn pop(&mut self) -> Option<Job> {
        self.jobs.pop_front()
    }

    pub fn remaining(&self) -> usize {
        self.capacity - self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Job> {
        self.jobs.iter()
    }

    pub fn push(&mut self, job: Job) -> Result<(), Error> {
        if self.remaining() == 0 {
            return Err(Error::QueueFull);
        }
        self.jobs.push_back(job);
        Ok(())
    }
}

pub trait Memory {
    fn read(
        &mut self,
        view: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error>;
    fn write(&mut self, view: ViewId, samples: &[Fixed]) -> Result<(), Error>;
}

pub struct JobProcessor {
    quantum: usize,
    buffers: Vec<Vec<Fixed>>,
    ready: Vec<bool>,
}

impl JobProcessor {
    pub fn quantum(&self) -> usize {
        self.quantum
    }
    pub fn new(quantum: usize, buffers: usize) -> Self {
        Self {
            quantum,
            buffers: vec![vec![Fixed::ZERO; quantum]; buffers],
            ready: vec![false; buffers],
        }
    }

    pub fn run(
        &mut self,
        queue: &mut JobQueue,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), Error> {
        if inputs.iter().any(|input| input.len() != self.quantum)
            || outputs.iter().any(|output| output.len() != self.quantum)
        {
            return Err(Error::InvalidShape);
        }
        self.ready.fill(false);
        for output in outputs.iter_mut() {
            output.fill(Fixed::ZERO);
        }
        while let Some(job) = queue.jobs.pop_front() {
            self.execute(job, memory, inputs, outputs)?;
        }
        Ok(())
    }

    fn require(&self, buffer: BufferId) -> Result<(), Error> {
        if self.ready.get(buffer.0).copied() != Some(true) {
            return Err(Error::InvalidBuffer);
        }
        Ok(())
    }

    fn destination(&self, buffer: BufferId) -> Result<(), Error> {
        if self.ready.get(buffer.0).copied() != Some(false) {
            return Err(Error::InvalidBuffer);
        }
        Ok(())
    }

    pub fn begin(&mut self) {
        self.ready.fill(false);
    }

    pub fn buffer(&self, id: BufferId) -> Result<&[Fixed], Error> {
        self.require(id)?;
        Ok(&self.buffers[id.0])
    }

    pub fn execute(
        &mut self,
        job: Job,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), Error> {
        match job {
            Job::Input { port, destination } => {
                self.destination(destination)?;
                self.buffers[destination.0]
                    .copy_from_slice(inputs.get(port).ok_or(Error::InvalidBuffer)?);
                self.ready[destination.0] = true;
            }
            Job::Silence { destination } => {
                self.destination(destination)?;
                self.buffers[destination.0].fill(Fixed::ZERO);
                self.ready[destination.0] = true;
            }
            Job::ReadView {
                view,
                position,
                length,
                destination,
            } => {
                self.destination(destination)?;
                memory.read(view, position, length, &mut self.buffers[destination.0])?;
                self.ready[destination.0] = true;
            }
            Job::Gain {
                source,
                destination,
                ramp,
            } => {
                self.require(source)?;
                self.destination(destination)?;
                for offset in 0..self.quantum {
                    self.buffers[destination.0][offset] =
                        self.buffers[source.0][offset].multiply(ramp.at(offset as u32));
                }
                self.ready[destination.0] = true;
            }
            Job::Mix {
                left,
                right,
                destination,
            } => {
                self.require(left)?;
                self.require(right)?;
                self.destination(destination)?;
                for offset in 0..self.quantum {
                    self.buffers[destination.0][offset] =
                        self.buffers[left.0][offset].saturating_add(self.buffers[right.0][offset]);
                }
                self.ready[destination.0] = true;
            }
            Job::Capture { source, view } => {
                self.require(source)?;
                memory.write(view, &self.buffers[source.0])?;
            }
            Job::Output { source, port } => {
                self.require(source)?;
                outputs
                    .get_mut(port)
                    .ok_or(Error::InvalidBuffer)?
                    .copy_from_slice(&self.buffers[source.0]);
            }
        }
        Ok(())
    }
}
