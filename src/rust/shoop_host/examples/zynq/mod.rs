use shoop_app_client::{Client, State};
use shoop_app_protocol::{ClientMessage, Command, Outcome};
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
    let operation = match message {
        ClientMessage::Submit { operation, .. } | ClientMessage::Shutdown { operation, .. } => {
            operation
        }
        _ => panic!("operation expected"),
    };
    connection.send(message).expect("command delivery");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !client.results().contains_key(&operation) {
        assert!(Instant::now() < deadline, "command timeout");
        pump(client, connection);
    }
    client.results()[&operation].clone()
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
    assert_eq!(report.requested.as_ref().unwrap().pools.loop_memory_mb, 128);
    println!("Management settings confirmed");
    if std::env::var_os("SHOOP_ZYNQ_DRIVER_SOCKET").is_some() {
        fixture::run(&mut client, &mut connection);
    }
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
        return;
    }
    let shutdown = fresh.shutdown().expect("cleanup retry");
    assert!(matches!(
        finish(&mut fresh, &mut connection, shutdown),
        Outcome::Confirmed { .. }
    ));
    println!("Zynq fixture host passed: confirmed render/MIDI/resource cleanup after retry");
}
