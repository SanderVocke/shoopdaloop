//! Measure only production processing; JSON, setup and observations stay outside.
use rt_backend_testbench::{
    production::{self, ProcessMonitor},
    spec::Scenario,
};
use shoop_wasm_test_support::heap::{measure, Counts, TrackingAllocator};
use std::{
    io::{self, Read},
    time::Instant,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
struct Monitor {
    rows: Vec<u128>,
}
impl ProcessMonitor for Monitor {
    fn process<T>(&mut self, run: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let (value, counts) = measure(run);
        let ns = start.elapsed().as_nanos();
        assert_eq!(
            counts,
            Counts::default(),
            "production quantum used the heap"
        );
        self.rows.push(ns);
        value
    }
}
fn main() {
    let numeric = std::env::args().nth(1).unwrap_or_else(|| "float".into());
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let scenario: Scenario = serde_json::from_str(&input).unwrap();
    let mut monitor = Monitor {
        rows: Vec::with_capacity(scenario.steps.len()),
    };
    let result = match numeric.as_str() {
        "float" => production::run_monitored::<f32, _>(&scenario, &mut monitor),
        "q824" => production::run_monitored::<rt_runtime::Q824, _>(&scenario, &mut monitor),
        _ => panic!("usage: cpu float|q824 < scenario.json"),
    }
    .unwrap();
    std::hint::black_box(&result);
    println!(
        "{}",
        serde_json::json!({"name":scenario.name,"numeric":numeric,"quantum":scenario.quantum,
        "quanta":monitor.rows.len(),"nanoseconds":monitor.rows,"callback_allocations":0,"callback_reallocations":0,"callback_deallocations":0})
    );
}
