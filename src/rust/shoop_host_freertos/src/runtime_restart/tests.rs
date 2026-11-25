use super::*;
use alloc::{boxed::Box, vec, vec::Vec};
use core::time::Duration;
use shoop_app::ServiceOptions;
use shoop_app_protocol::{ClientId, ManagementReply, ManagementRequest, OperationId, VERSION};
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::{testing::OfflineRender, EngineDescription, HostPorts, Segment};
use shoop_wasm_test_support::shoop_test;

struct Clock(OfflineRender);
impl EngineClock for Clock {
    fn advance(&mut self, elapsed: Duration) {
        self.0.run_for(elapsed, |_, _| 0.).unwrap();
    }
    fn wait(&mut self) -> bool {
        self.0.run(16, |_, _| 0.).unwrap();
        true
    }
}
fn service() -> Service {
    let (render, client) = OfflineRender::new(&EngineDescription {
        quantum: 1,
        sample_rate: 1000,
        max_frames: 16,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 8,
            loops: 80,
        }],
        strip_channels: 2,
        resource_pages: 1024,
        page_bytes: 256,
        midi_events: 64,
        midi_bytes: 512,
        ..Default::default()
    })
    .unwrap();
    let backend = RtBackend::connect(client, Clock(render)).unwrap();
    let mut service = Service::new(
        Box::new(backend),
        ServiceOptions {
            master_auto_connect: false,
            loop_smoothing_ms: 0,
        },
    )
    .unwrap();
    for _ in 0..64 {
        service.tick(Duration::from_millis(1));
    }
    assert!(service.is_ready());
    service
}
struct Fake {
    stop: bool,
    reset: bool,
    start: bool,
    progress: bool,
    quarantined: usize,
    log: Vec<u8>,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            stop: true,
            reset: true,
            start: true,
            progress: true,
            quarantined: 0,
            log: Vec::new(),
        }
    }
}
impl Renderer for Fake {
    fn cleanup(&mut self) -> Option<bool> {
        Some(true)
    }
    fn restart_supported(&mut self) -> bool {
        true
    }
    fn stop_for_restart(&mut self) -> Result<bool, Failure> {
        self.log.push(1);
        Ok(self.stop)
    }
    fn reset(&mut self) -> Result<bool, Failure> {
        self.log.push(2);
        Ok(self.reset)
    }
    fn start(&mut self) -> Result<Option<Service>, Failure> {
        self.log.push(3);
        if self.start {
            Ok(Some(service()))
        } else {
            Err(Failure::InitializationFailed)
        }
    }
    fn progressed(&self) -> bool {
        self.progress
    }
    fn quarantine(&mut self) {
        self.quarantined += 1;
    }
}
fn accepted() -> Restart {
    let mut service = service();
    let restart = Restart::new();
    service.enable_backend_restart(restart.state.clone());
    let request = ManagementRequest::RestartBackend {
        version: VERSION,
        operation: OperationId {
            client: ClientId(3),
            sequence: 1,
        },
        expected_generation: 1,
    };
    assert!(matches!(
        service.management(request),
        ManagementReply::BackendRestart(_)
    ));
    restart
}

#[shoop_test]
fn supervisor_yields_for_ack_then_requires_stop_reset_and_fresh_frame_progress() {
    let mut restart = accepted();
    let mut fake = Fake {
        progress: false,
        ..Default::default()
    };
    assert!(fake.restart_supported());
    assert_eq!(fake.cleanup(), Some(true));
    assert!(restart.poll(&mut fake, false, 100).is_none());
    assert!(restart.poll(&mut fake, false, 349).is_none());
    assert!(fake.log.is_empty());
    assert!(restart.poll(&mut fake, false, 350).is_none());
    assert!(restart.poll(&mut fake, false, 351).is_none());
    assert!(restart.poll(&mut fake, false, 352).is_none());
    let replacement = restart.poll(&mut fake, false, 353).unwrap();
    assert!(replacement.backend_ready());
    assert_eq!(fake.log, vec![1, 2, 3]);
    assert_eq!(restart.state.status().phase, Phase::Starting);
    assert!(restart.poll(&mut fake, true, 354).is_none());
    assert_eq!(restart.state.status().phase, Phase::Starting);
    fake.progress = true;
    assert!(restart.poll(&mut fake, false, 355).is_none());
    assert_eq!(restart.state.status().phase, Phase::Starting);
    restart.poll(&mut fake, true, 356);
    assert_eq!(restart.state.status().phase, Phase::Ready);
}

#[shoop_test]
fn failure_does_not_reset_a_running_worker_or_start_with_unfenced_memory() {
    for failure in [
        Failure::WorkerDidNotStop,
        Failure::FabricDidNotQuiesce,
        Failure::InitializationFailed,
    ] {
        let mut restart = accepted();
        let mut fake = Fake {
            stop: failure != Failure::WorkerDidNotStop,
            reset: failure != Failure::FabricDidNotQuiesce,
            start: failure != Failure::InitializationFailed,
            ..Default::default()
        };
        for now in [0, 250, 251, 252, 253, 20_001] {
            restart.poll(&mut fake, false, now);
        }
        assert_eq!(restart.state.status().phase, Phase::Failed);
        assert_eq!(restart.state.status().failure, Some(failure));
        if failure == Failure::WorkerDidNotStop {
            assert!(fake.log.iter().all(|step| *step == 1));
        }
        if failure == Failure::FabricDidNotQuiesce {
            assert!(!fake.log.contains(&3));
        }
        let count = fake.log.len();
        restart.poll(&mut fake, true, 30_000);
        assert_eq!(fake.log.len(), count);
        assert!(fake.quarantined >= 2);
    }
}
