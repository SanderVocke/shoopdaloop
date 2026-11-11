//! Node-level controls retained for the standalone audio fixtures.
use crate::{
    compiler::{Binding, NodeId},
    state::{Address, StateProcessor, Value},
    Error,
};
use alloc::{vec, vec::Vec};

#[derive(Clone, Copy)]
struct LoopBinding {
    enabled: Address,
    clock: Option<(Address, Address)>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OnWrap {
    pub source: NodeId,
    pub target: NodeId,
    pub enabled: bool,
}

pub struct SignalTransitions {
    loops: Vec<Option<LoopBinding>>,
    wrapped: Vec<bool>,
    pending: Vec<OnWrap>,
    frame: u64,
    chunk_frames: u32,
    in_quantum: bool,
}
impl SignalTransitions {
    pub fn new(bindings: &[Binding]) -> Self {
        Self::with_chunk_frames(bindings, 1)
    }
    pub fn with_chunk_frames(bindings: &[Binding], chunk_frames: u32) -> Self {
        let loops = bindings
            .iter()
            .map(|binding| match *binding {
                Binding::Playback {
                    position,
                    length,
                    enabled,
                } => Some(LoopBinding {
                    enabled,
                    clock: Some((position, length)),
                }),
                Binding::Capture { enabled } => Some(LoopBinding {
                    enabled,
                    clock: None,
                }),
                _ => None,
            })
            .collect();
        Self {
            loops,
            wrapped: vec![false; bindings.len()],
            pending: Vec::with_capacity(bindings.len().max(1) * 4),
            frame: 0,
            chunk_frames,
            in_quantum: false,
        }
    }
    fn binding(&self, node: NodeId) -> Result<LoopBinding, Error> {
        self.loops
            .get(node.0)
            .copied()
            .flatten()
            .ok_or(Error::InvalidState)
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }
    pub fn set_enabled(
        &mut self,
        state: &mut StateProcessor,
        node: NodeId,
        enabled: bool,
    ) -> Result<(), Error> {
        if self.in_quantum {
            return Err(Error::InvalidState);
        }
        state.write(self.binding(node)?.enabled, Value::Word(u64::from(enabled)))
    }
    pub fn seek(
        &mut self,
        state: &mut StateProcessor,
        node: NodeId,
        frame: u64,
    ) -> Result<(), Error> {
        if self.in_quantum {
            return Err(Error::InvalidState);
        }
        let (position, length) = self.binding(node)?.clock.ok_or(Error::InvalidState)?;
        if frame >= state.read(length)?.word()? {
            return Err(Error::InvalidState);
        }
        state.write(position, Value::Word(frame))
    }
    pub fn on_wrap(&mut self, action: OnWrap) -> Result<(), Error> {
        if self.binding(action.source)?.clock.is_none() {
            return Err(Error::InvalidState);
        }
        self.binding(action.target)?;
        if self.pending.len() == self.pending.capacity() {
            return Err(Error::QueueFull);
        }
        self.pending.push(action);
        Ok(())
    }
    /// All pulses were computed before any target is changed. Declaration and
    /// signal-execution order therefore cannot change same-boundary eligibility.
    pub fn begin(&mut self, state: &mut StateProcessor) -> Result<(), Error> {
        if self.in_quantum {
            return Err(Error::InvalidState);
        }
        let mut index = 0;
        while index < self.pending.len() {
            let action = self.pending[index];
            if self.wrapped[action.source.0] {
                self.set_enabled(state, action.target, action.enabled)?;
                self.pending.remove(index);
            } else {
                index += 1;
            }
        }
        self.wrapped.fill(false);
        self.in_quantum = true;
        Ok(())
    }
    /// Advance once after a successful quantum, never once per signal node.
    pub fn finish(&mut self, state: &mut StateProcessor, quantum: u32) -> Result<(), Error> {
        if !self.in_quantum || self.chunk_frames == 0 || quantum % self.chunk_frames != 0 {
            return Err(Error::InvalidState);
        }
        let end = self
            .frame
            .checked_add(u64::from(quantum))
            .ok_or(Error::InvalidState)?;
        for (index, binding) in self.loops.iter().enumerate() {
            let Some(binding) = binding else {
                continue;
            };
            let Some((position, length)) = binding.clock else {
                continue;
            };
            if state.read(binding.enabled)?.word()? == 0 {
                continue;
            }
            let length = state.read(length)?.word()?;
            let current = state.read(position)?.word()?;
            if length == 0 || current >= length {
                return Err(Error::InvalidState);
            }
            let next = u128::from(current) + u128::from(quantum / self.chunk_frames);
            self.wrapped[index] = next >= u128::from(length);
            state.write(position, Value::Word((next % u128::from(length)) as u64))?;
        }
        self.frame = end;
        self.in_quantum = false;
        Ok(())
    }
}
