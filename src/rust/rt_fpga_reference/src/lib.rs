//! Executable, audio-only reference for a compiler -> state processor -> job processor.
//! This is a readable functional model, not a cycle simulator or an RT implementation.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
use alloc::vec::Vec;

pub mod compiler;
pub mod fixed;
pub mod jobs;
pub mod state;
pub mod storage;

use compiler::{Binding, Graph, NodeId, Program};
use fixed::{Fixed, Ramp};
use jobs::{JobProcessor, JobQueue};
use state::{StateProcessor, Value};
use storage::MediaStore;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidGraph,
    InvalidState,
    InvalidBuffer,
    InvalidView,
    InvalidShape,
    QueueFull,
}

/// Convenience host adapter. The two processors themselves have no graph access.
pub struct Backend {
    pub program: Program,
    pub state: StateProcessor,
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
        self.state.run(&self.program, &mut self.queue)?;
        self.jobs
            .run(&mut self.queue, &mut self.media, inputs, outputs)
    }

    pub fn set_enabled(&mut self, node: NodeId, enabled: bool) -> Result<(), Error> {
        let address = match self.bindings.get(node.0).ok_or(Error::InvalidGraph)? {
            Binding::Playback { enabled, .. } | Binding::Capture { enabled, .. } => *enabled,
            _ => return Err(Error::InvalidState),
        };
        self.state.write(address, Value::Word(u64::from(enabled)))
    }

    pub fn seek(&mut self, node: NodeId, position: u64) -> Result<(), Error> {
        let Binding::Playback {
            position: address,
            length,
            ..
        } = *self.bindings.get(node.0).ok_or(Error::InvalidGraph)?
        else {
            return Err(Error::InvalidState);
        };
        let length = self.state.read(length)?.word()?;
        if position >= length {
            return Err(Error::InvalidState);
        }
        self.state.write(address, Value::Word(position))
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
