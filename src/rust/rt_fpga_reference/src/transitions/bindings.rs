//! Prepared export of logical decisions to independent signal-state operands.
use super::{Decision, LoopId, TransitionError as Error};
use crate::state::{Address, StateProcessor, Value};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackBinding {
    pub enabled: Address,
    pub position: Address,
    pub length: Address,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoopBinding {
    pub id: LoopId,
    pub playback: Option<PlaybackBinding>,
    pub capture_enabled: Option<Address>,
}
pub struct StateBindings {
    bindings: Vec<LoopBinding>,
}
impl StateBindings {
    pub fn prepare(bindings: Vec<LoopBinding>, state: &StateProcessor) -> Result<Self, Error> {
        let mut addresses = Vec::new();
        for (index, binding) in bindings.iter().enumerate() {
            if binding.id.generation == 0 {
                return Err(Error::InvalidId);
            }
            if bindings[..index].iter().any(|old| old.id == binding.id) {
                return Err(Error::DuplicateId);
            }
            for address in binding
                .playback
                .into_iter()
                .flat_map(|p| [p.enabled, p.position, p.length])
                .chain(binding.capture_enabled)
            {
                if addresses.contains(&address) {
                    return Err(Error::DuplicateId);
                }
                state
                    .read(address)
                    .and_then(Value::word)
                    .map_err(|_| Error::InvalidState)?;
                addresses.push(address);
            }
        }
        Ok(Self { bindings })
    }
    pub fn export(&self, decisions: &[Decision], state: &mut StateProcessor) -> Result<(), Error> {
        // Validate the whole batch before the first state write.
        for binding in &self.bindings {
            if decisions
                .iter()
                .filter(|d| d.state.id == binding.id)
                .count()
                != 1
            {
                return Err(Error::InvalidId);
            }
            for address in binding
                .playback
                .into_iter()
                .flat_map(|p| [p.enabled, p.position, p.length])
                .chain(binding.capture_enabled)
            {
                state
                    .read(address)
                    .and_then(Value::word)
                    .map_err(|_| Error::InvalidState)?;
            }
        }
        for binding in &self.bindings {
            let decision = decisions
                .iter()
                .find(|d| d.state.id == binding.id)
                .ok_or(Error::InvalidId)?;
            if let Some(p) = binding.playback {
                state
                    .write(p.enabled, Value::Word(u64::from(decision.state.plays())))
                    .map_err(|_| Error::InvalidState)?;
                state
                    .write(p.position, Value::Word(decision.state.position))
                    .map_err(|_| Error::InvalidState)?;
                state
                    .write(p.length, Value::Word(decision.state.content_length))
                    .map_err(|_| Error::InvalidState)?;
            }
            if let Some(enabled) = binding.capture_enabled {
                state
                    .write(
                        enabled,
                        Value::Word(u64::from(decision.state.mode == super::Mode::Recording)),
                    )
                    .map_err(|_| Error::InvalidState)?;
            }
        }
        Ok(())
    }
}
