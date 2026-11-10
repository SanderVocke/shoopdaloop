//! Audio-only reference with separate compilation, transitions, state, and jobs.
//! This is a readable functional model, not a cycle simulator or an RT implementation.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
use alloc::vec::Vec;

pub mod compiler;
pub mod fixed;
pub mod jobs;
pub mod pipeline;
pub mod state;
pub mod storage;
pub mod transitions;

#[cfg(feature = "serde")]
mod serde_index;

use compiler::{Binding, Graph, NodeId, Program};
use fixed::{Fixed, Ramp};
use jobs::{JobProcessor, JobQueue};
use state::{StateProcessor, Value};
use storage::MediaStore;
use transitions::SignalTransitions;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Error {
    InvalidGraph,
    InvalidState,
    InvalidBuffer,
    InvalidView,
    InvalidShape,
    QueueFull,
    MemoryFull,
}

/// Convenience host adapter. The two processors themselves have no graph access.
pub struct Backend {
    pub program: Program,
    pub state: StateProcessor,
    pub transitions: SignalTransitions,
    pub jobs: JobProcessor,
    pub media: MediaStore,
    pub queue: JobQueue,
    bindings: Vec<Binding>,
}

impl Backend {
    pub fn new(graph: &Graph, media: MediaStore) -> Result<Self, Error> {
        let compiled = compiler::compile(graph)?;
        Ok(Self {
            queue: JobQueue::new(compiled.program.jobs_per_quantum),
            transitions: SignalTransitions::new(&compiled.bindings),
            state: StateProcessor::new(compiled.initial_state),
            jobs: JobProcessor::new(graph.quantum, graph.nodes.len()),
            media,
            program: compiled.program,
            bindings: compiled.bindings,
        })
    }

    pub fn process(
        &mut self,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), Error> {
        if !self.queue.is_empty() || self.queue.remaining() < self.program.jobs_per_quantum {
            return Err(Error::QueueFull);
        }
        self.transitions.begin(&mut self.state)?;
        self.state.run(&self.program, &mut self.queue)?;
        self.jobs
            .run(&mut self.queue, &mut self.media, inputs, outputs)?;
        self.transitions
            .finish(&mut self.state, self.program.quantum)
    }

    pub fn set_enabled(&mut self, node: NodeId, enabled: bool) -> Result<(), Error> {
        self.transitions.set_enabled(&mut self.state, node, enabled)
    }

    pub fn seek(&mut self, node: NodeId, position: u64) -> Result<(), Error> {
        self.transitions.seek(&mut self.state, node, position)
    }

    pub fn set_gain(&mut self, node: NodeId, target: Fixed, frames: u32) -> Result<(), Error> {
        let Binding::Gain { ramp } = *self.bindings.get(node.0).ok_or(Error::InvalidGraph)? else {
            return Err(Error::InvalidState);
        };
        let current = self.state.read(ramp)?.ramp()?.at(0);
        self.state
            .write(ramp, Value::Ramp(Ramp::new(current, target, frames)))
    }
}
