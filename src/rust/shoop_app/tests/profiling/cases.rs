use super::*;
use shoop_app_api::{diagnostics_protocol as protocol, task_profiling as tasks};

struct Tasks {
    active: bool,
    pending: bool,
    clock: u64,
    samples: Rc<Cell<usize>>,
}
impl protocol::broker::Provider for Tasks {
    fn descriptor(&self) -> protocol::Descriptor {
        protocol::Descriptor {
            stream: tasks::stream(77),
            name: protocol::Name::new("host.freertos.tasks").unwrap(),
            available: true,
            min_interval_ms: 1,
            max_interval_ms: 1000,
            snapshot: true,
            inspect: false,
        }
    }
    fn control(&mut self, action: protocol::Action) -> protocol::Status {
        if let protocol::Action::Subscribe { enabled, .. } = action {
            self.active = enabled;
        }
        protocol::Status {
            stream: tasks::stream(77),
            requested: self.active,
            active: self.active,
            interval_ms: 1,
            outcome: protocol::Outcome::Applied,
            dropped: 0,
            coalesced: 0,
        }
    }
    fn advance(&mut self, _: Duration) {
        if self.active {
            self.pending = true;
            self.clock += 1000;
            self.samples.set(self.samples.get() + 1);
        }
    }
    fn take_report(&mut self) -> Option<Result<Vec<u8>, protocol::Outcome>> {
        if !core::mem::take(&mut self.pending) {
            return None;
        }
        Some(
            protocol::encode_report(&tasks::Snapshot {
                generation: 1,
                clock_domain: 77,
                captured_us: self.clock,
                collection_us: 10,
                total_runtime: Some(self.clock),
                counter_bits: 64,
                counter_hz: 1000000,
                complete: true,
                tasks: vec![tasks::Task {
                    id: 3,
                    name: protocol::Name::new("control").unwrap(),
                    state: 0,
                    priority: 3,
                    base_priority: Some(3),
                    runtime: Some(self.clock / 4),
                    stack_free_bytes: Some(4096),
                }],
            })
            .map_err(|_| protocol::Outcome::Capacity),
        )
    }
}

#[shoop_test]
fn host_profiling_is_independent_of_audio_revisions_and_stops_on_detach() {
    for encoded in [false, true] {
        let mut f = fixture();
        let samples = Rc::new(Cell::new(0));
        f.service
            .register_profiling_provider(Box::new(Tasks {
                active: false,
                pending: false,
                clock: 0,
                samples: samples.clone(),
            }))
            .unwrap();
        let connection = ConnectionId(900);
        let mut client = attach(&mut f.service, 9, connection, encoded);
        let id = submit(
            &mut f.service,
            &mut client,
            connection,
            Command::Diagnostic(protocol::Request {
                source: tasks::SOURCE,
                epoch: 77,
                action: protocol::Action::Subscribe {
                    enabled: true,
                    interval_ms: 1,
                },
            }),
        );
        complete(&mut f.service, &mut client, connection, id, encoded);
        let before = f.service.snapshot().revision;
        for _ in 0..4 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, encoded);
        }
        assert_eq!(f.service.snapshot().revision, before);
        assert!(!client.profiling().requested);
        let host = &client.diagnostics().tasks;
        assert_eq!(
            host.sample.as_ref().unwrap().tasks[0].name.as_str(),
            "control"
        );
        assert_eq!(host.cpu_percent, [Some(25.)]);
        let id = submit(
            &mut f.service,
            &mut client,
            connection,
            new_track("rebuild", false),
        );
        complete(&mut f.service, &mut client, connection, id, encoded);
        for _ in 0..4 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, encoded);
        }
        assert_eq!(
            client
                .diagnostics()
                .tasks
                .sample
                .as_ref()
                .unwrap()
                .clock_domain,
            77
        );
        assert_eq!(client.diagnostics().tasks.cpu_percent, [Some(25.)]);
        // A stalled consumer retains bounded reports while controls still complete.
        for _ in 0..32 {
            f.service.tick(STEP);
        }
        drain(&mut f.service, &mut client, connection, encoded);
        assert!(
            client
                .diagnostics()
                .sources
                .iter()
                .find(|s| s.descriptor.stream.source == tasks::SOURCE)
                .unwrap()
                .status
                .unwrap()
                .coalesced
                > 0
        );
        f.service.detach(connection);
        let count = samples.get();
        for _ in 0..4 {
            f.service.tick(STEP);
        }
        assert_eq!(samples.get(), count);
        let mut fresh = attach(&mut f.service, 10, ConnectionId(901), encoded);
        f.service.tick(STEP);
        drain(&mut f.service, &mut fresh, ConnectionId(901), encoded);
        assert!(fresh.diagnostics().tasks.sample.is_none());
    }
}
