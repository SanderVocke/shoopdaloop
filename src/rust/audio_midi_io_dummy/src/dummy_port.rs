use crate::external_connections::ExternalConnectionsState;
use anyhow::{anyhow, Result};
use audio_midi_io_traits::{types::PortDirection};
use std::sync::{Arc, Mutex, Weak};

struct DummyPortSharedData {
    pub name: String,
    pub direction: PortDirection,
    pub external_connections: Weak<ExternalConnectionsState>,
}

pub struct DummyPort {
    shared_data: Arc<Mutex<DummyPortSharedData>>,
}

impl DummyPort {
    pub fn name(&self) -> Result<String> {
        Ok(self
            .shared_data
            .try_lock()
            .map_err(|e| anyhow!("{e}"))?
            .name
            .clone())
    }

    pub fn direction(&self) -> Result<PortDirection> {
        Ok(self
            .shared_data
            .try_lock()
            .map_err(|e| anyhow!("{e}"))?
            .direction)
    }
}
