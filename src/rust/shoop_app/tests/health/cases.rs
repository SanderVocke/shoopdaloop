use super::*;
use shoop_app_api::{diagnostics_protocol as protocol, host_health as health};

fn temperature() -> health::Snapshot {
    let mut sample = health::Snapshot::default();
    sample.temperatures[0] = Some(health::Temperature {
        name: protocol::Name::new("test die").unwrap(),
        current_millicelsius: Some(42500),
        peak_millicelsius: Some(63000),
    });
    sample.overtemperature = Some(false);
    sample
}

#[shoop_test]
fn health_is_built_in_without_sensors_or_audio_profiling() {
    for encoded in [false, true] {
        let mut f = fixture();
        assert!(f.service.remove_profiling_provider(health::SOURCE).is_err());
        let connection = ConnectionId(910);
        let mut client = attach(&mut f.service, 1, connection, encoded);
        for _ in 0..4 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, encoded);
        }
        assert!(!client.profiling().requested);
        assert_eq!(
            client.diagnostics().health,
            Some(health::Snapshot::default())
        );
        let source = client
            .diagnostics()
            .sources
            .iter()
            .find(|source| source.descriptor.stream.source == health::SOURCE)
            .unwrap();
        assert!(source.supported && source.descriptor.available);
        assert!(source.status.unwrap().active);
        assert_eq!(source.status.unwrap().interval_ms, 1000);
    }
}

#[shoop_test]
fn health_sampling_survives_reconnection_and_does_not_mutate_session_revisions() {
    for encoded in [false, true] {
        let mut f = fixture();
        f.service.set_health_reader(temperature).unwrap();
        let connection = ConnectionId(911);
        let mut client = attach(&mut f.service, 1, connection, encoded);
        assert!(f.service.set_health_reader(temperature).is_err());
        let revision = f.service.snapshot().revision;
        for _ in 0..4 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, encoded);
        }
        assert_eq!(client.diagnostics().health, Some(temperature()));
        assert_eq!(f.service.snapshot().revision, revision);
        f.service.detach(connection);
        client.disconnect();
        assert!(client.diagnostics().health.is_none());
        f.service
            .set_health_reader(|| panic!("detached health sampling"))
            .unwrap();
        f.service.tick(Duration::from_secs(5));
        f.service.set_health_reader(temperature).unwrap();
        let connection = ConnectionId(912);
        let mut client = attach(&mut f.service, 2, connection, encoded);
        assert!(client.diagnostics().health.is_none());
        for _ in 0..4 {
            f.service.tick(STEP);
            drain(&mut f.service, &mut client, connection, encoded);
        }
        assert_eq!(client.diagnostics().health, Some(temperature()));
        assert!(
            client
                .diagnostics()
                .sources
                .iter()
                .find(|source| source.descriptor.stream.source == health::SOURCE)
                .unwrap()
                .status
                .unwrap()
                .active
        );
    }
}
