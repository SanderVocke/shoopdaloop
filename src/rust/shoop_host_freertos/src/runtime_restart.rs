use shoop_app::{BackendRestarts, Service};
use shoop_app_protocol::{BackendRestartFailure as Failure, BackendRestartPhase as Phase};

#[cfg(test)]
mod tests;

pub(crate) trait Renderer {
    fn cleanup(&mut self) -> Option<bool>;
    fn restart_supported(&mut self) -> bool {
        false
    }
    fn stop_for_restart(&mut self) -> Result<bool, Failure> {
        Err(Failure::WorkerDidNotStop)
    }
    fn reset(&mut self) -> Result<bool, Failure> {
        Err(Failure::FabricDidNotQuiesce)
    }
    fn start(&mut self) -> Result<Option<Service>, Failure> {
        Err(Failure::InitializationFailed)
    }
    fn progressed(&self) -> bool {
        false
    }
    fn quarantine(&mut self) {}
}

impl<F: FnMut() -> Option<bool>> Renderer for F {
    fn cleanup(&mut self) -> Option<bool> {
        self()
    }
}

pub(crate) struct Restart {
    pub state: BackendRestarts,
    started: Option<u64>,
    replacement: bool,
}

impl Restart {
    pub fn new() -> Self {
        Self {
            state: BackendRestarts::default(),
            started: None,
            replacement: false,
        }
    }

    pub fn poll(&mut self, renderer: &mut impl Renderer, ready: bool, now: u64) -> Option<Service> {
        let phase = self.state.status().phase;
        if phase == Phase::Failed {
            renderer.quarantine();
            return None;
        }
        if matches!(phase, Phase::Idle | Phase::Ready) {
            return None;
        }
        let started = *self.started.get_or_insert(now);
        if now.saturating_sub(started) >= 20_000 {
            self.state.fail(match phase {
                Phase::Stopping => Failure::WorkerDidNotStop,
                Phase::Resetting => Failure::FabricDidNotQuiesce,
                _ => Failure::InitializationFailed,
            });
            renderer.quarantine();
            return None;
        }
        let result = match phase {
            Phase::Accepted if now.saturating_sub(started) >= 250 => {
                self.state.advance(Phase::Stopping);
                Ok(None)
            }
            Phase::Stopping => renderer.stop_for_restart().map(|stopped| {
                if stopped {
                    self.state.advance(Phase::Resetting);
                }
                None
            }),
            Phase::Resetting => renderer.reset().map(|reset| {
                if reset {
                    self.state.advance(Phase::Starting);
                }
                None
            }),
            Phase::Starting if !self.replacement => renderer.start().map(|service| {
                self.replacement = service.is_some();
                service
            }),
            Phase::Starting if ready && renderer.progressed() => {
                self.state.advance(Phase::Ready);
                self.started = None;
                self.replacement = false;
                Ok(None)
            }
            _ => Ok(None),
        };
        match result {
            Ok(service) => service,
            Err(error) => {
                self.state.fail(error);
                renderer.quarantine();
                None
            }
        }
    }
}
