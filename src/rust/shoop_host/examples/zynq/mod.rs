use shoop_app_client::{Client, State};
use shoop_app_protocol::{ClientMessage, Command, OperationId, Outcome};
use shoop_host::{
    connection::{self, Connection},
    remote::RemoteProfile,
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};
mod fixture;
mod recovery;
#[cfg(test)]
mod tests;
mod usb;

fn pump(client: &mut Client, connection: &mut Connection) {
    fixture::advance();
    connection.poll(client).expect("client progression");
    if let Some(message) = client.ready_message() {
        connection.send(message).expect("ready acknowledgment");
        client.ready_sent().expect("ready state");
    }
    thread::sleep(Duration::from_millis(2));
}
fn attach(profile: &RemoteProfile) -> (Client, Connection) {
    let mut connection =
        connection::connect(profile, Duration::from_secs(60)).expect("data connection");
    let mut client = Client::new();
    connection
        .send(client.attach().expect("attachment"))
        .expect("send attach");
    let deadline = Instant::now() + Duration::from_secs(30);
    while client.state() != State::Ready {
        assert!(Instant::now() < deadline, "initial snapshot timeout");
        pump(&mut client, &mut connection);
    }
    (client, connection)
}
fn command(client: &mut Client, connection: &mut Connection, command: Command) -> Outcome {
    let message = client.command(command, None).expect("command");
    finish(client, connection, message)
}
fn finish(client: &mut Client, connection: &mut Connection, message: ClientMessage) -> Outcome {
    let wait_for_snapshot = matches!(message, ClientMessage::Submit { .. });
    let operation = match message {
        ClientMessage::Submit { operation, .. } | ClientMessage::Shutdown { operation, .. } => {
            operation
        }
        _ => panic!("operation expected"),
    };
    connection.send(message).expect("command delivery");
    wait_for_result(client, operation, wait_for_snapshot, |client| {
        pump(client, connection)
    })
}
fn wait_for_result(
    client: &mut Client,
    operation: OperationId,
    wait_for_snapshot: bool,
    mut progress: impl FnMut(&mut Client),
) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(outcome) = client.results().get(&operation) {
            let published = match outcome {
                Outcome::Confirmed { revision }
                | Outcome::AncestorsStopped { revision, .. }
                | Outcome::TrackCreated { revision, .. }
                | Outcome::LoopCreated { revision, .. } => {
                    !wait_for_snapshot
                        || client
                            .snapshot()
                            .is_some_and(|state| state.revision >= *revision)
                }
                Outcome::Refused { .. } => true,
            };
            if published {
                return outcome.clone();
            }
        }
        assert!(Instant::now() < deadline, "command timeout");
        progress(client);
    }
}
pub fn run() {
    let path = std::env::args().nth(1).expect("remote profile path");
    let profile = RemoteProfile::read(Path::new(&path)).expect("profile");
    if std::env::var_os("SHOOP_ZYNQ_RECOVERY").is_some() {
        recovery::run(&profile);
        return;
    }
    let (mut client, mut connection) = attach(&profile);
    if let Some(path) = std::env::var_os("SHOOP_ZYNQ_PREVIOUS_AUTHORITY") {
        let old: shoop_app_protocol::Authority =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_ne!(
            old.lease,
            client.authority().unwrap().lease,
            "boot reused authority"
        );
        assert!(matches!(
            connection::management(
                &profile,
                shoop_app_protocol::ManagementRequest::Shutdown {
                    authority: old,
                    operation: shoop_app_protocol::OperationId {
                        client: old.client,
                        sequence: 1
                    }
                },
                Duration::from_secs(5)
            )
            .unwrap(),
            shoop_app_protocol::ManagementReply::Refused(
                shoop_app_protocol::Refusal::StaleAuthority
            )
        ));
        println!("Restart at identical TCP endpoints refused previous-boot authority");
    }
    println!("Attached: {:?}", client.snapshot().unwrap().status);
    let report =
        connection::host_report(&profile, Duration::from_secs(10)).expect("management settings");
    let recording_mib = std::env::var("SHOOP_ZYNQ_RECORDING_MIB")
        .map(|value| value.parse::<u32>().expect("board recording budget"))
        .unwrap_or(128);
    assert_eq!(
        report.requested.as_ref().unwrap().pools.loop_memory_mb,
        recording_mib
    );
    println!("Management settings confirmed");
    if std::env::var_os("SHOOP_ZYNQ_DRIVER_SOCKET").is_some() {
        fixture::run(&mut client, &mut connection);
    }
    let mut usb = std::env::var_os("SHOOP_ZYNQ_USB_SOCKET")
        .map(|path| usb::run(&mut client, &mut connection, path));
    assert!(matches!(
        command(&mut client, &mut connection, Command::SetLoopSmoothingMs(7)),
        Outcome::Confirmed { .. }
    ));
    let initial_authority = client.authority().unwrap();
    if let Some(path) = std::env::var_os("SHOOP_ZYNQ_AUTHORITY_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&initial_authority).unwrap()).unwrap();
    }
    let before = client.snapshot().unwrap().status.processed_frames;
    println!("Control confirmed; dropping UI connection");
    drop(connection);
    thread::sleep(Duration::from_millis(100));
    let (mut fresh, mut connection) = attach(&profile);
    println!("Fresh attachment confirmed");
    assert!(fresh.authority().unwrap().generation > initial_authority.generation);
    assert!(
        fresh.snapshot().unwrap().status.processed_frames > before,
        "periodic rendering stopped while the UI was absent"
    );
    assert!(matches!(
        command(&mut fresh, &mut connection, Command::SetLoopSmoothingMs(3)),
        Outcome::Confirmed { .. }
    ));
    let shutdown = fresh.shutdown().expect("shutdown request");
    if std::env::var("SHOOP_ZYNQ_USB_FAULT").as_deref() == Ok("nak") {
        let started = Instant::now();
        assert!(matches!(
            finish(&mut fresh, &mut connection, shutdown),
            Outcome::Refused { .. }
        ));
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "USB shutdown exceeded the simulation harness deadline"
        );
        println!(
            "Unresponsive USB OUT refused shutdown after bounded recovery and controller stop"
        );
        return;
    }
    if std::env::var_os("SHOOP_ZYNQ_DRIVER_SOCKET").is_some() {
        assert!(matches!(
            finish(&mut fresh, &mut connection, shutdown),
            Outcome::Refused { .. }
        ));
        println!("Injected cleanup failure was reported; retrying retained render owner");
    } else {
        assert!(matches!(
            finish(&mut fresh, &mut connection, shutdown),
            Outcome::Confirmed { .. }
        ));
        println!("Zynq production host smoke passed: attach, management, control, fresh attach, confirmed shutdown");
        if let Some(peer) = usb.as_mut() {
            peer.finish();
        }
        return;
    }
    let shutdown = fresh.shutdown().expect("cleanup retry");
    assert!(matches!(
        finish(&mut fresh, &mut connection, shutdown),
        Outcome::Confirmed { .. }
    ));
    println!("Zynq fixture host passed: confirmed render/MIDI/resource cleanup after retry");
}
