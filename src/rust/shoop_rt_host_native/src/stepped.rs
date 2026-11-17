use std::sync::Arc;

use crate::{cycle::Cycle, status::Shared};
use shoop_rt_engine::{ControlEndpoint, EngineDescription, Phase, PrepareError};
use shoop_rt_protocol::{DriverKind, DriverRunState, DriverStatus};
use shoop_rt_transport::carrier::Outbox;

/// A synchronous dummy driver. Each call processes one prepared callback without pacing.
pub struct SteppedDummy {
    cycle: Cycle,
    frames: usize,
}

impl SteppedDummy {
    pub fn prepare(
        description: &EngineDescription,
    ) -> Result<(Self, ControlEndpoint, Outbox), PrepareError> {
        let driver = DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: description.sample_rate,
            buffer_frames: description.max_frames as u32,
        };
        let engine = shoop_rt_engine::prepare(description, driver)?;
        let outbox = engine.outbox();
        let shared = Arc::new(Shared::new(engine.host, driver));
        let cycle = Cycle::new(engine.render, shared, description, Vec::new(), Vec::new())?;
        Ok((
            Self {
                cycle,
                frames: description.max_frames,
            },
            engine.endpoint,
            outbox,
        ))
    }

    pub fn next_frame(&self) -> u64 {
        self.cycle.next_frame()
    }
    pub fn inputs_mut(&mut self) -> &mut [Box<[f32]>] {
        self.cycle.inputs_mut()
    }
    pub fn outputs(&self) -> &[Box<[f32]>] {
        self.cycle.outputs()
    }
    pub fn process(&mut self) -> Result<(), rt_runtime::Error> {
        self.cycle.render(self.frames, 0, false);
        match self.cycle.render_owner().phase() {
            Phase::Running => Ok(()),
            Phase::Faulted(error) => Err(error),
            Phase::Disconnected => Err(rt_runtime::Error::InvalidState),
        }
    }
    pub fn resource_usage(&self) -> rt_runtime::resource::PoolUsage {
        self.cycle.render_owner().resource_usage()
    }
    pub fn resource_work(&self) -> rt_runtime::resource::Work {
        self.cycle.render_owner().resource_work().0
    }
}
