# 14. Shutdown

<!-- rust-source: ../examples/first_loop.rs -->

[Contents](../README.md) · [Previous: Capacities, refusals, and results](13-capacities-refusals-and-results.md)

Every example ends the same way, and that ending is the shutdown sequence. From the [first loop example](../examples/first_loop.rs):

```rust
    drop(control);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    resources.begin_callback();
    resources.reclaim();
    let usage = resources.usage();
    assert_eq!(usage.free_pages, 16);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(planner.media_usage().references, 0);
```

## 1. The order, and why

1. **Stop the music while callbacks still run.** Submit Stop (and wait for its execution result) before tearing anything down. Destruction alone does not perform a musical Stop, send device output, or turn pending requests into successes.
2. **Stop and join the callback owner.** A device-backed host must stop its audio callback and join its thread before touching the runtime, so no `process` call is in flight.
3. **Drop the control endpoint, then extract.** `runtime.into_planner_and_resources()` hands the planner and store back once no callback is running; the runtime's processors are destroyed with it.
4. **Clean up the planner explicitly.** `planner.cleanup(&mut resources)` releases media references, capture groups, and internally created resources. It is permanent teardown — the loop cannot resume from it.
5. **Reclaim storage.** `begin_callback()` opens a maintenance pass; `reclaim()` recycles retired pages under a work allowance. Small takes need one pass; large content may need several. During rendering the runtime owns this accounting — do not reset it per operation.

## 2. MIDI devices need one more step

If any loop was MIDI-bearing, deliver the loop's cleanup/restoration packets to your device before reclaiming (chapter 10), or send your own all-notes-off. The library's MIDI restore covers the loop-owned state; device-specific resets are yours.

After the sequence above, all pools report their full capacity free — the examples assert exactly that, which is also a handy leak check in your own tests.

[Previous: Capacities, refusals, and results](13-capacities-refusals-and-results.md) · [Contents](../README.md)
