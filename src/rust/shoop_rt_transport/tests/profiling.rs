use rt_profiling::wire::Record as AudioRecord;
use shoop_rt_protocol::{EngineMessage, Lane};
use shoop_rt_transport::carrier::Outbox;
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn profiling_saturation_preserves_other_admission_and_reliable_priority() {
    let mut out = Outbox::new(1, 1, 1);
    let packet = EngineMessage::Profiling(AudioRecord::Status(Default::default()).encode(1));
    let capacity = out.available(Lane::Profiling);
    for _ in 0..capacity {
        out.push(packet).unwrap();
    }
    assert!(out.push(packet).is_err());
    assert_eq!(out.available(Lane::Bulk), 1);
    assert_eq!(out.available(Lane::Telemetry), 1);
    out.push(EngineMessage::Rejected { version: 7 }).unwrap();
    assert_eq!(out.pop(), Some(EngineMessage::Rejected { version: 7 }));
    assert_eq!(out.pop(), Some(packet));
}
