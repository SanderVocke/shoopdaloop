use super::{command, pump, Client, Connection, Duration, Instant, Outcome};
use shoop_app_api::{
    diagnostics_protocol::{Action, Request},
    task_profiling,
};
use shoop_app_protocol::Command;

pub(super) fn run(client: &mut Client, connection: &mut Connection) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let stream = loop {
        if let Some(source) = client
            .diagnostics()
            .sources
            .iter()
            .find(|s| s.descriptor.stream.source == task_profiling::SOURCE)
        {
            break source.descriptor.stream;
        }
        assert!(Instant::now() < deadline, "task catalog missing");
        pump(client, connection);
    };
    assert!(
        !client.profiling().requested,
        "host profiling must not enable audio profiling"
    );
    assert!(matches!(
        command(
            client,
            connection,
            Command::Diagnostic(Request {
                source: stream.source,
                epoch: stream.epoch,
                action: Action::Subscribe {
                    enabled: true,
                    interval_ms: 1000
                }
            })
        ),
        Outcome::Confirmed { .. }
    ));
    loop {
        if client
            .diagnostics()
            .tasks
            .cpu_percent
            .iter()
            .any(Option::is_some)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "task interval data missing: {:?}",
            client.diagnostics()
        );
        pump(client, connection);
    }
    let view = &client.diagnostics().tasks;
    let sample = view.sample.as_ref().unwrap();
    assert!(sample.valid() && sample.complete);
    assert!(sample.tasks.iter().any(|t| t.name.as_str() == "render"));
    assert!(sample.tasks.iter().any(|t| t.name.as_str() == "control"));
    assert!(sample
        .tasks
        .iter()
        .all(|t| t.stack_free_bytes.is_some_and(|bytes| bytes > 0)));
    assert!(view
        .cpu_percent
        .iter()
        .flatten()
        .all(|cpu| cpu.is_finite() && (0. ..=100.).contains(cpu)));
    assert!(!client.profiling().requested);
    println!("FreeRTOS profiling: tasks={} capture_us={} collection_us={} total_runtime={} counter_hz={} cpu={:?}", sample.tasks.len(), sample.captured_us, sample.collection_us, sample.total_runtime.unwrap(), sample.counter_hz, view.cpu_percent);
    assert!(matches!(
        command(
            client,
            connection,
            Command::Diagnostic(Request {
                source: stream.source,
                epoch: stream.epoch,
                action: Action::Subscribe {
                    enabled: false,
                    interval_ms: 1000
                }
            })
        ),
        Outcome::Confirmed { .. }
    ));
    while client
        .diagnostics()
        .sources
        .iter()
        .find(|s| s.descriptor.stream.source == stream.source)
        .and_then(|s| s.status)
        .is_none_or(|s| s.active)
    {
        assert!(
            Instant::now() < deadline,
            "task unsubscribe was not acknowledged"
        );
        pump(client, connection);
    }
    assert!(
        client.diagnostics().tasks.sample.is_some(),
        "disable lost final task report"
    );
}
