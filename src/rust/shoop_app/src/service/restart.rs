use super::*;
use alloc::rc::Rc;
use core::cell::RefCell;
use shoop_app_protocol::{BackendRestartFailure, BackendRestartPhase, BackendRestartStatus};

#[derive(Clone)]
pub struct BackendRestarts(Rc<RefCell<State>>);

struct State {
    status: BackendRestartStatus,
    expected: u64,
    persistence_epoch: u64,
}

impl Default for BackendRestarts {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(State {
            status: BackendRestartStatus {
                generation: 1,
                operation: None,
                phase: BackendRestartPhase::Idle,
                failure: None,
            },
            expected: 0,
            persistence_epoch: 1,
        })))
    }
}

impl BackendRestarts {
    pub fn status(&self) -> BackendRestartStatus {
        self.0.borrow().status
    }

    pub fn unavailable(&self) -> bool {
        !matches!(
            self.status().phase,
            BackendRestartPhase::Idle | BackendRestartPhase::Ready
        )
    }

    pub(super) fn request(
        &self,
        operation: OperationId,
        expected: u64,
        persistence_epoch: u64,
    ) -> ManagementReply {
        let mut state = self.0.borrow_mut();
        if state.status.operation == Some(operation) {
            return if state.expected == expected {
                ManagementReply::BackendRestart(state.status)
            } else {
                ManagementReply::Refused(Refusal::StaleAuthority)
            };
        }
        if self.unavailable_status(state.status.phase) {
            return ManagementReply::Refused(Refusal::Busy);
        }
        if expected != state.status.generation {
            return ManagementReply::Refused(Refusal::StaleAuthority);
        }
        let Some(generation) = expected.checked_add(1) else {
            return ManagementReply::Refused(Refusal::Overloaded);
        };
        let Some(persistence_epoch) = persistence_epoch.checked_add(1) else {
            return ManagementReply::Refused(Refusal::Overloaded);
        };
        state.expected = expected;
        state.persistence_epoch = persistence_epoch;
        state.status = BackendRestartStatus {
            generation,
            operation: Some(operation),
            phase: BackendRestartPhase::Accepted,
            failure: None,
        };
        ManagementReply::BackendRestart(state.status)
    }

    fn unavailable_status(&self, phase: BackendRestartPhase) -> bool {
        !matches!(
            phase,
            BackendRestartPhase::Idle | BackendRestartPhase::Ready
        )
    }

    pub fn advance(&self, phase: BackendRestartPhase) {
        let mut state = self.0.borrow_mut();
        let allowed = matches!(
            (state.status.phase, phase),
            (BackendRestartPhase::Accepted, BackendRestartPhase::Stopping)
                | (
                    BackendRestartPhase::Stopping,
                    BackendRestartPhase::Resetting
                )
                | (
                    BackendRestartPhase::Resetting,
                    BackendRestartPhase::Starting
                )
                | (BackendRestartPhase::Starting, BackendRestartPhase::Ready)
        );
        assert!(allowed, "invalid backend restart transition");
        state.status.phase = phase;
    }

    pub fn fail(&self, failure: BackendRestartFailure) {
        let mut state = self.0.borrow_mut();
        state.status.phase = BackendRestartPhase::Failed;
        state.status.failure = Some(failure);
    }
}

impl Service {
    pub fn enable_backend_restart(&mut self, restarts: BackendRestarts) {
        self.persistence_epoch = self
            .persistence_epoch
            .max(restarts.0.borrow().persistence_epoch);
        self.backend_restarts = Some(restarts);
    }
}
