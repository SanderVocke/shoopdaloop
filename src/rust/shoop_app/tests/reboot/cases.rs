use super::*;

#[shoop_test]
fn device_reboot_is_opt_in_versioned_and_does_not_replace_ordinary_shutdown() {
    let mut f = fixture();
    assert_eq!(
        f.service
            .management(ManagementRequest::Reboot { version: VERSION }),
        ManagementReply::Refused(Refusal::Unsupported)
    );
    assert!(!f.service.reboot_requested());
    f.service.enable_system_reboot();
    assert_eq!(
        f.service.management(ManagementRequest::Reboot {
            version: VERSION - 1
        }),
        ManagementReply::Refused(Refusal::Version)
    );
    assert!(!f.service.reboot_requested());
    assert!(!f.service.shutdown_ready());
}

#[shoop_test]
fn device_reboot_is_destructive_idempotent_and_does_not_wait_for_engine_or_ui_cleanup() {
    let mut f = fixture();
    f.service.enable_system_reboot();
    let connection = ConnectionId(921);
    let _client = attach(&mut f.service, 1, connection, false);
    let revision = f.service.snapshot().revision;
    let waits = f.waits.get();
    f.advance.set(false);
    for _ in 0..2 {
        assert_eq!(
            f.service
                .management(ManagementRequest::Reboot { version: VERSION }),
            ManagementReply::Rebooting { version: VERSION }
        );
        f.service.enable_system_reboot();
        assert!(f.service.reboot_requested());
    }
    assert!(f.service.has_connection(connection));
    assert!(!f.service.is_ready());
    assert!(!f.service.shutdown_ready());
    assert_eq!(f.service.snapshot().revision, revision);
    assert_eq!(f.waits.get(), waits);
    f.service.detach(connection);
    assert_eq!(
        f.service.handle(
            ConnectionId(922),
            ClientMessage::Attach { version: VERSION }
        ),
        Err(Refusal::NotReady)
    );
}
