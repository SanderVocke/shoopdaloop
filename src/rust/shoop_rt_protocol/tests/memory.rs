use shoop_rt_protocol::{codec::*, *};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn bounded_and_unbounded_memory_reports_round_trip_without_losing_precision() {
    let mut report = MemoryReport {
        frame: u64::MAX,
        accounted_bytes: Some(u64::MAX),
        ..Default::default()
    };
    for (i, resource) in report.resources.iter_mut().enumerate() {
        *resource = Some(MemoryResource {
            name: Name::new(&format!("{i:064}")).unwrap(),
            unit: MemoryUnit::Blocks,
            used: 1 << 54,
            reserved: 7,
            capacity: (i % 2 == 0).then_some(u64::MAX),
        });
    }
    let message = EngineMessage::Telemetry(Telemetry::Memory(report));
    let mut buffer = [0; MAX_RECORD_BYTES];
    let len = encode_engine(&message, &mut buffer).unwrap();
    assert_eq!(decode_engine(&buffer[..len]).unwrap(), message);
    assert!(decode_engine(&buffer[..len - 1]).is_err());
    let resource = report.resources[0].as_mut().unwrap();
    resource.used = 0;
    resource.reserved = 0;
    resource.capacity = Some(0);
    assert_eq!(
        encode_engine(
            &EngineMessage::Telemetry(Telemetry::Memory(report)),
            &mut buffer
        ),
        Err(CodecError::Invalid)
    );
}

#[shoop_test]
fn pressure_uses_the_fullest_pool_and_unknown_limits_do_not_mean_zero() {
    let mut report = MemoryReport::default();
    for (i, (used, capacity)) in [(30, Some(100)), (95, Some(100)), (9999, None)]
        .into_iter()
        .enumerate()
    {
        report.resources[i] = Some(MemoryResource {
            name: Name::new(&format!("Pool {i}")).unwrap(),
            unit: MemoryUnit::Pages,
            used,
            reserved: 0,
            capacity,
        });
    }
    assert_eq!(report.limiting_resource().unwrap().name.as_str(), "Pool 1");
    assert_eq!(report.limiting_resource().unwrap().pressure(), Some(0.95));
    assert_eq!(report.resources[2].unwrap().pressure(), None);
    report.resources[0] = report.resources[1];
    assert!(!report.valid());
    report.resources[0] = None;
    report.resources[1].as_mut().unwrap().reserved = 6;
    assert!(!report.valid());
}
