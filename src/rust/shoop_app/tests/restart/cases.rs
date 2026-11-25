use super::*;
use shoop_app::BackendRestarts;

fn restart(operation: OperationId, expected_generation: u64) -> ManagementRequest {
    ManagementRequest::RestartBackend {
        version: VERSION,
        operation,
        expected_generation,
    }
}
fn operation(sequence: u64) -> OperationId {
    OperationId {
        client: ClientId(817),
        sequence,
    }
}

#[shoop_test]
fn restart_is_opt_in_versioned_and_independent_of_a_ui_lease() {
    let mut f = fixture();
    let query = ManagementRequest::BackendRestartStatus { version: VERSION };
    assert_eq!(
        f.service.management(query.clone()),
        ManagementReply::Refused(Refusal::Unsupported)
    );
    assert_eq!(
        f.service.management(restart(operation(1), 1)),
        ManagementReply::Refused(Refusal::Unsupported)
    );
    let state = BackendRestarts::default();
    f.service.enable_backend_restart(state.clone());
    assert_eq!(
        f.service.management(query),
        ManagementReply::BackendRestart(state.status())
    );
    assert_eq!(
        f.service
            .management(ManagementRequest::BackendRestartStatus {
                version: VERSION - 1
            }),
        ManagementReply::Refused(Refusal::Version)
    );
    assert_eq!(
        f.service.management(restart(operation(1), 0)),
        ManagementReply::Refused(Refusal::Invalid)
    );
    assert_eq!(
        f.service.management(restart(operation(1), 2)),
        ManagementReply::Refused(Refusal::StaleAuthority)
    );
    assert_eq!(state.status().generation, 1);
    let connection = ConnectionId(817);
    let _client = attach(&mut f.service, 1, connection, false);
    let revision = f.service.snapshot().revision;
    let reply = f.service.management(restart(operation(1), 1));
    assert_eq!(state.status().phase, BackendRestartPhase::Accepted);
    assert_eq!(state.status().generation, 2);
    assert_eq!(reply, ManagementReply::BackendRestart(state.status()));
    assert_eq!(f.service.management(restart(operation(1), 1)), reply);
    assert_eq!(
        f.service.management(restart(operation(2), 1)),
        ManagementReply::Refused(Refusal::Busy)
    );
    assert!(!f.service.is_ready());
    assert!(f.service.has_connection(connection));
    assert_eq!(f.service.snapshot().revision, revision);
    f.service.detach(connection);
    assert_eq!(
        f.service.handle(
            ConnectionId(818),
            ClientMessage::Attach { version: VERSION }
        ),
        Err(Refusal::NotReady)
    );
}

#[shoop_test]
fn restart_preserves_monotonic_persistence_fences_after_project_replacement() {
    let mut f = fixture();
    let state = BackendRestarts::default();
    f.service.enable_backend_restart(state.clone());
    let mut client = attach(&mut f.service, 1, ConnectionId(1), false);
    for _ in 0..2 {
        let operation = submit(
            &mut f.service,
            &mut client,
            ConnectionId(1),
            Command::NewSession,
        );
        assert!(matches!(
            complete(
                &mut f.service,
                &mut client,
                ConnectionId(1),
                operation,
                false
            ),
            Outcome::Confirmed { .. }
        ));
    }
    let previous = client.snapshot().unwrap().persistence_epoch;
    assert!(previous > 1);
    f.service.management(restart(operation(1), 1));
    for phase in [
        BackendRestartPhase::Stopping,
        BackendRestartPhase::Resetting,
        BackendRestartPhase::Starting,
        BackendRestartPhase::Ready,
    ] {
        state.advance(phase);
    }
    let mut fresh = fixture();
    fresh.service.enable_backend_restart(state);
    let client = attach(&mut fresh.service, 1, ConnectionId(2), false);
    assert_eq!(client.snapshot().unwrap().persistence_epoch, previous + 1);
}

#[shoop_test]
fn recovery_status_survives_service_replacement_and_serializes_destructive_commands() {
    let mut f = fixture();
    let state = BackendRestarts::default();
    f.service.enable_backend_restart(state.clone());
    f.service.enable_system_reboot();
    f.service.management(restart(operation(1), 1));
    assert_eq!(
        f.service
            .management(ManagementRequest::Reboot { version: VERSION }),
        ManagementReply::Refused(Refusal::Busy)
    );
    for phase in [
        BackendRestartPhase::Stopping,
        BackendRestartPhase::Resetting,
        BackendRestartPhase::Starting,
    ] {
        state.advance(phase);
    }
    let mut fresh = fixture();
    fresh.service.enable_backend_restart(state.clone());
    assert!(!fresh.service.is_ready());
    state.advance(BackendRestartPhase::Ready);
    assert!(fresh.service.is_ready());
    let query = ManagementRequest::BackendRestartStatus { version: VERSION };
    assert_eq!(
        fresh.service.management(query),
        ManagementReply::BackendRestart(state.status())
    );
    assert_eq!(
        fresh.service.management(restart(operation(1), 1)),
        ManagementReply::BackendRestart(state.status())
    );
    assert_eq!(
        fresh.service.management(restart(operation(2), 1)),
        ManagementReply::Refused(Refusal::StaleAuthority)
    );
    assert_eq!(
        fresh.service.management(restart(operation(1), 2)),
        ManagementReply::Refused(Refusal::StaleAuthority)
    );
    fresh.service.management(restart(operation(2), 2));
    assert_eq!(state.status().generation, 3);
    state.fail(BackendRestartFailure::FabricDidNotQuiesce);
    assert_eq!(
        fresh.service.management(restart(operation(3), 3)),
        ManagementReply::Refused(Refusal::Busy)
    );
    fresh.service.enable_system_reboot();
    assert_eq!(
        fresh
            .service
            .management(ManagementRequest::Reboot { version: VERSION }),
        ManagementReply::Rebooting { version: VERSION }
    );
    assert!(!fresh.service.shutdown_ready());
}
