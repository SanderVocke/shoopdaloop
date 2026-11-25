use super::*;
use shoop_app_protocol::{BackendRestartFailure, ClientId, OperationId};
use shoop_wasm_test_support::shoop_test;

fn reply(generation: u64, phase: Phase) -> Event {
    Event::Support(Ok(Some(BackendRestartStatus {
        generation,
        phase,
        operation: Some(OperationId {
            client: ClientId(1),
            sequence: 1,
        }),
        failure: (phase == Phase::Failed).then_some(BackendRestartFailure::FabricDidNotQuiesce),
    })))
}
fn deliver(controller: &mut BackendRestart, event: Event) -> bool {
    let (sender, receiver) = mpsc::channel();
    controller.receiver = Some(receiver);
    assert!(sender.send(event).is_ok());
    controller.poll()
}

#[shoop_test]
fn uncertain_restart_only_reconnects_after_a_new_generation_is_ready() {
    let mut controller = BackendRestart {
        generation: Some(1),
        intent_generation: Some(1),
        pending: true,
        ..Default::default()
    };
    assert!(!deliver(
        &mut controller,
        Event::Finished(Err("acknowledgment lost".into()))
    ));
    assert!(!controller.pending);
    assert!(!controller.available);
    assert!(!deliver(&mut controller, reply(2, Phase::Starting)));
    assert!(!deliver(&mut controller, reply(1, Phase::Ready)));
    assert!(deliver(&mut controller, reply(2, Phase::Ready)));
    assert!(!controller.pending);
}

#[shoop_test]
fn unsupported_and_failed_hosts_cannot_be_restarted_and_lost_workers_do_not_hang_ui() {
    let mut controller = BackendRestart::default();
    assert!(!deliver(&mut controller, Event::Support(Ok(None))));
    assert!(!controller.available);
    assert!(!deliver(&mut controller, reply(2, Phase::Failed)));
    assert!(!controller.available);
    assert!(controller
        .notice()
        .unwrap()
        .contains("full device reboot required"));
    controller.pending = true;
    let (sender, receiver) = mpsc::channel();
    controller.receiver = Some(receiver);
    drop(sender);
    assert!(!controller.poll());
    assert!(!controller.pending);
    assert!(controller.receiver.is_none());
    assert!(controller
        .notice()
        .unwrap()
        .contains("query management status"));
}
