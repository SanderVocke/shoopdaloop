use super::*;

#[shoop_test(no_wasm = "Native management client")]
fn restart_client_sends_one_destructive_request_then_polls_the_same_generation() {
    let mut operation = None;
    let mut requests = 0;
    let mut phases = [
        BackendRestartPhase::Stopping,
        BackendRestartPhase::Resetting,
        BackendRestartPhase::Starting,
        BackendRestartPhase::Ready,
    ]
    .into_iter();
    let mut observed = Vec::new();
    restart_backend_on(
        Duration::from_secs(3),
        |request, _| {
            let phase = match request {
                ManagementRequest::BackendRestartStatus { .. } if operation.is_none() => {
                    BackendRestartPhase::Idle
                }
                ManagementRequest::BackendRestartStatus { .. } => phases.next().unwrap(),
                ManagementRequest::RestartBackend {
                    operation: id,
                    expected_generation,
                    ..
                } => {
                    requests += 1;
                    assert_eq!(expected_generation, 7);
                    operation = Some(id);
                    BackendRestartPhase::Accepted
                }
                _ => panic!("unexpected lifecycle command"),
            };
            Ok(ManagementReply::BackendRestart(BackendRestartStatus {
                generation: if operation.is_some() { 8 } else { 7 },
                operation,
                phase,
                failure: None,
            }))
        },
        |status| observed.push(status.phase),
    )
    .unwrap();
    assert_eq!(requests, 1);
    assert_eq!(
        observed,
        [
            BackendRestartPhase::Accepted,
            BackendRestartPhase::Stopping,
            BackendRestartPhase::Resetting,
            BackendRestartPhase::Starting,
            BackendRestartPhase::Ready
        ]
    );
}

#[shoop_test(no_wasm = "Native management client")]
fn a_lost_restart_reply_is_not_replayed_and_a_stale_completion_is_not_success() {
    for lost in [false, true] {
        let mut calls = 0;
        let error = restart_backend_on(
            Duration::from_secs(1),
            |request, _| {
                calls += 1;
                match request {
                    ManagementRequest::BackendRestartStatus { .. } => {
                        Ok(ManagementReply::BackendRestart(BackendRestartStatus {
                            generation: 7,
                            operation: None,
                            phase: BackendRestartPhase::Idle,
                            failure: None,
                        }))
                    }
                    ManagementRequest::RestartBackend { operation, .. } if !lost => {
                        Ok(ManagementReply::BackendRestart(BackendRestartStatus {
                            generation: 7,
                            operation: Some(operation),
                            phase: BackendRestartPhase::Ready,
                            failure: None,
                        }))
                    }
                    ManagementRequest::RestartBackend { .. } => Err(anyhow!("reply lost")),
                    _ => panic!("unexpected lifecycle command"),
                }
            },
            |_| panic!("stale or unacknowledged operation reported progress"),
        )
        .unwrap_err();
        assert_eq!(calls, 2);
        assert!(error
            .to_string()
            .contains(if lost { "query status" } else { "superseded" }));
    }
}
