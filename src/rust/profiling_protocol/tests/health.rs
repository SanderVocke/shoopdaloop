use profiling_protocol::{health::*, *};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn temperature(name: &str) -> Temperature {
    Temperature {
        name: Name::new(name).unwrap(),
        current_millicelsius: Some(-21250),
        peak_millicelsius: Some(74500),
    }
}

#[shoop_test]
fn health_roundtrips_absence_failed_reads_negative_temperatures_and_alarm_states() {
    assert!(descriptor(9).valid());
    for alarm in [None, Some(false), Some(true)] {
        let mut sample = Snapshot {
            overtemperature: alarm,
            ..Default::default()
        };
        for sensor in [
            None,
            Some(temperature("SoC die")),
            Some(Temperature {
                current_millicelsius: None,
                peak_millicelsius: None,
                ..temperature("SoC die")
            }),
        ] {
            sample.temperatures[0] = sensor;
            assert!(sample.valid());
            let bytes = encode_report(&sample).unwrap();
            assert!(bytes.len() <= health::MAX_REPORT_BYTES);
            assert_eq!(decode::<Snapshot>(&bytes).unwrap(), sample);
        }
    }
}

#[shoop_test]
fn health_has_bounded_unique_named_sensors_and_rejects_impossible_values() {
    let mut sample = Snapshot::default();
    for (index, slot) in sample.temperatures.iter_mut().enumerate() {
        *slot = Some(temperature(&format!("sensor {index} {}", "x".repeat(54))));
    }
    assert!(sample.valid());
    let bytes = encode_report(&sample).unwrap();
    assert!(bytes.len() <= health::MAX_REPORT_BYTES);
    assert_eq!(decode::<Snapshot>(&bytes).unwrap(), sample);
    sample.temperatures[1] = sample.temperatures[0];
    assert!(!sample.valid());
    for value in [i32::MIN, -273151, 250001, i32::MAX] {
        let mut sample = Snapshot::default();
        sample.temperatures[0] = Some(Temperature {
            current_millicelsius: Some(value),
            ..temperature("die")
        });
        assert!(!sample.valid());
    }
    sample = Snapshot::default();
    sample.temperatures[0] = Some(temperature(""));
    assert!(!sample.valid());
}

#[shoop_test]
fn successful_health_reports_clear_transient_provider_failure_status() {
    let mut broker = broker::Broker::default();
    broker.register(descriptor(1), None).unwrap();
    broker.report_failed(SOURCE, Outcome::Unavailable);
    let mut failed = false;
    while let Some(record) = broker.next_record() {
        if let Record::Status(status) = record {
            failed |= status.outcome == Outcome::Unavailable;
        }
    }
    assert!(failed);
    broker
        .publish(SOURCE, encode_report(&Snapshot::default()).unwrap())
        .unwrap();
    let mut recovered = false;
    while let Some(record) = broker.next_record() {
        if let Record::Status(status) = record {
            recovered |= status.outcome == Outcome::Applied;
        }
    }
    assert!(recovered);
}
