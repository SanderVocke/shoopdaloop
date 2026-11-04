use std::{hint::black_box, sync::OnceLock, time::Instant};
static EPOCH: OnceLock<Instant> = OnceLock::new();
fn now() -> u64 {
    EPOCH.get().unwrap().elapsed().as_nanos() as u64
}
fn main() {
    EPOCH.get_or_init(Instant::now);
    let mut step = u64::MAX;
    let mut previous = now();
    for _ in 0..10000 {
        let next = now();
        if next > previous {
            step = step.min(next - previous);
        }
        previous = next;
    }
    let mut batches = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        for _ in 0..1000000 {
            black_box(now());
        }
        batches.push(start.elapsed().as_nanos() as f64 / 1000000.);
    }
    println!("{{\"observed_step_ns\":{step},\"ns_per_read_batches\":{batches:?}}}");
}
