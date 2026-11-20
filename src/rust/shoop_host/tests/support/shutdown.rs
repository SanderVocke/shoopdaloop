use shoop_app_client::Client;
use shoop_app_protocol::{ClientMessage, ManagementReply, ManagementRequest, Outcome};
use shoop_host::{connection, remote::Target};
use std::{
    process::{Child, ExitStatus},
    thread,
    time::{Duration, Instant},
};

pub fn shutdown(child: &mut Child, entry: &Target, client: &mut Client) -> ExitStatus {
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = client.shutdown().unwrap()
    else {
        unreachable!()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut request = ManagementRequest::Shutdown {
        authority,
        operation,
    };
    loop {
        match connection::management(
            entry,
            request,
            deadline.saturating_duration_since(Instant::now()),
        )
        .unwrap()
        {
            ManagementReply::Finished { result, .. } => {
                assert!(
                    matches!(result.outcome, Outcome::Confirmed { .. }),
                    "{:?}",
                    result.outcome
                );
                break;
            }
            ManagementReply::Accepted { .. } => {}
            other => panic!("shutdown refused: {other:?}"),
        }
        assert!(Instant::now() < deadline, "shutdown timed out");
        thread::sleep(Duration::from_millis(10));
        request = ManagementRequest::Result {
            authority,
            operation,
        };
    }
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "host did not exit after cleanup");
        thread::sleep(Duration::from_millis(10));
    }
}
