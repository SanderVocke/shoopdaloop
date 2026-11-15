#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_testbench::timing::{
    Fixed, Timing,
    dram::{Bus, Dram, Snapshot},
    histogram_summary, summary,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn fixed_latency_and_population_statistics() {
    let values = summary(&[1, 2, 3, 4, 5]);
    assert_eq!(values["mean"].as_f64().unwrap(), 3.0);
    assert_eq!(values["median"].as_f64().unwrap(), 3.0);
    assert_eq!(values["p99"], 5);
    assert!((values["stddev"].as_f64().unwrap() - 2f64.sqrt()).abs() < 1e-12);
    assert_eq!(summary(&[]), serde_json::json!({"count":0}));
    assert_eq!(
        histogram_summary(&[(1, 1), (2, 1), (3, 1), (4, 1), (5, 1)].into()),
        values
    );
    let mut fixed = Fixed::new(7);
    let token = fixed.submit(0, false).unwrap();
    for _ in 0..7 {
        assert!(!fixed.ready(token));
        fixed.advance();
    }
    assert!(fixed.ready(token));
}
fn model(name: &str) -> Dram {
    Dram::default(
        &rt_fpga_testbench::root().join(format!("work/marlin-suite/dram/{name}")),
        100_000_000,
    )
    .unwrap()
}
fn transact(d: &mut Dram, address: u64, write: bool) -> u64 {
    let token = d.submit(address, write).unwrap();
    for _ in 0..10_000 {
        d.advance();
        if d.ready(token) {
            let e = d.events.last().unwrap();
            return e.completed - e.submitted;
        }
    }
    panic!("DRAM transaction timeout");
}
#[shoop_test]
fn native_dram_refresh_geometry_conflicts_overlap_queue_reset_and_physical_accounting() {
    let mut d = model("refresh");
    assert_eq!(d.native_bytes, 16);
    assert_eq!(d.capacity, 512 * 1024 * 1024);
    for _ in 0..1000 {
        d.advance();
    }
    assert_eq!(d.ticks(), 1000 * 10_000_000 / 1_875_000);
    assert!(d.ticks() > 4160);
    assert!(transact(&mut d, 0x1000000, false) > 0);
    let report = d.report(true);
    assert!(report["refresh_commands"].as_u64().unwrap() > 0);
    assert_eq!(report, d.report(true));
    let mut d = model("rows");
    let cold = transact(&mut d, 0x1000000, false);
    let hit = transact(&mut d, 0x1000010, false);
    let conflict = transact(&mut d, 0x1020000, false);
    assert!(hit < cold && conflict > hit);
    transact(&mut d, 0x1000040, true);
    transact(&mut d, 0x1000040, false);
    d.drain(1_000_000);
    assert!(d.idle());
    let mut d = model("queue");
    let tokens: Vec<_> = (0..64)
        .filter_map(|i| d.submit(0x1000000 + (i % 4) * 16, false))
        .collect();
    assert_eq!(tokens.len(), 32);
    assert_eq!(
        tokens
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        32
    );
    d.drain(1_000_000);
    assert!(tokens.iter().all(|&t| d.ready(t)));
    transact(&mut d, 0x1000800, true);
    assert!(!d.idle(), "posted callback is not physical completion");
    d.drain(1_000_000);
    assert!(d.idle());
    let mut d = model("banks");
    let mut pair = |second| {
        d.reset();
        let start = d.ticks();
        let a = d.submit(0x1000000, false).unwrap();
        let b = d.submit(second, false).unwrap();
        d.drain(1_000_000);
        assert!(d.ready(a) && d.ready(b));
        d.ticks() - start
    };
    assert!(pair(0x1000800) < pair(0x1004000));
    let mut d = model("reset");
    let old = d.submit(0x1000000, false).unwrap();
    d.reset();
    let new = d.submit(0x1000000, false).unwrap();
    assert_ne!(old, new);
    d.drain(1_000_000);
    assert!(!d.ready(old) && d.ready(new));
    for address in [u64::MAX - 15, 1, d.capacity] {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.submit(address, false)))
                .is_err()
        );
    }
    let mut d = model("final");
    transact(&mut d, 0x1000000, true);
    let first = d.report(true);
    assert_eq!(first, d.report(true));
    assert_eq!(first["dram_write_commands"], 1);
    assert_eq!(first["dram_write_service_dram_cycles"]["count"], 1);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.advance())).is_err());
    let mut d = model("bus");
    let start = d.snapshot();
    let tokens: Vec<_> = (0..4)
        .map(|_| d.submit(0x1000000, false).unwrap())
        .collect();
    d.drain(1_000_000);
    assert!(tokens.iter().all(|&t| d.ready(t)));
    let read = d.utilization(start, d.snapshot());
    assert_eq!(read["read_commands"], 1);
    assert_eq!(read["read_cycles"], 4);
    assert_eq!(read["transferred_bytes"], 16);
    assert!(read["command_cycles"].as_u64().unwrap() > 1);
    let utilization = read["data_bus_utilization"].as_f64().unwrap();
    assert!(utilization > 0.0 && utilization <= 1.0);
    assert!(
        (read["transfer_bytes_per_second"].as_f64().unwrap()
            / read["peak_bytes_per_second"].as_f64().unwrap()
            - utilization)
            .abs()
            < 1e-12
    );
    let start = d.snapshot();
    transact(&mut d, 0x1001000, true);
    d.drain(1_000_000);
    let write = d.utilization(start, d.snapshot());
    assert_eq!(write["write_commands"], 1);
    assert_eq!(write["write_cycles"], 4);
    assert_eq!(write["read_cycles"], 0);
    let start = d.snapshot();
    for _ in 0..20 {
        d.advance();
    }
    assert_eq!(
        d.utilization(start, d.snapshot())["data_bus_utilization"]
            .as_f64()
            .unwrap(),
        0.0
    );
    d.reset();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || d.utilization(start, d.snapshot())
        ))
        .is_err()
    );
    let mut d = model("windows");
    let start = d.snapshot();
    let token = d.submit(0x1000000, false).unwrap();
    let mut previous = start;
    let mut accumulated = 0;
    for _ in 0..100 {
        d.advance();
        let current = d.snapshot();
        accumulated += d.utilization(previous, current)["data_busy_cycles"]
            .as_u64()
            .unwrap();
        previous = current;
        if d.ready(token) {
            break;
        }
    }
    assert!(d.ready(token));
    assert_eq!(accumulated, 4);
    assert_eq!(
        d.utilization(start, d.snapshot())["data_busy_cycles"],
        accumulated
    );
    let _ = d.utilization(
        Snapshot {
            generation: d.generation,
            counters: Bus::default(),
        },
        d.snapshot(),
    );
}
