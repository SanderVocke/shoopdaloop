use shoop_rt_protocol::*;
use shoop_rt_transport::carrier::*;
use shoop_rt_transport::contracts::*;
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn lifecycle_credits_resync_bulk_and_limits_behave_alike_on_both_carriers() {
    let (in_process, looped) = carriers();
    lifecycle(in_process);
    lifecycle(looped);
}

#[shoop_test]
fn a_version_mismatch_is_rejected_on_both_carriers() {
    let (mut in_process, mut looped) = carriers();
    for carrier in [&mut in_process as &mut dyn Carrier, &mut looped] {
        carrier
            .send(ClientMessage::Hello {
                version: PROTOCOL_VERSION + 1,
            })
            .unwrap();
        assert_eq!(
            carrier.receive(),
            Some(EngineMessage::Rejected {
                version: PROTOCOL_VERSION + 1
            })
        );
    }
}

#[shoop_test]
fn telemetry_keeps_the_latest_records_while_reliable_and_bulk_lanes_refuse() {
    let mut out = Outbox::new(1, 2, 1);
    for frame in 0..5 {
        out.push(EngineMessage::Telemetry(Telemetry::Engine {
            frame,
            callbacks: frame,
            xruns: 0,
            load: 0.,
            input_peak: 0.,
            output_peak: 0.,
            driver: DRIVER,
        }))
        .unwrap();
    }
    assert_eq!(out.dropped_telemetry(), 3);
    let result = EngineMessage::Result {
        correlation: 1,
        outcome: Outcome::Done,
    };
    out.push(result).unwrap();
    assert_eq!(out.push(result), Err(CarrierError::Full));
    assert_eq!(out.available(Lane::Reliable), 0);
    assert_eq!(out.pop(), Some(result));
    let frames: Vec<u64> = std::iter::from_fn(|| out.pop())
        .map(|message| match message {
            EngineMessage::Telemetry(Telemetry::Engine { frame, .. }) => frame,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(frames, [3, 4]);
}
