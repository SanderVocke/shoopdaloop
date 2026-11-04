# Loop runtime test support

This dev-only crate assembles the loop parts and optional independent processors
into the same `GraphRuntime` used by production. `LoopRuntime<P, Q>` and
`LoopControl<P, Q>` support a processor's typed operations alongside loop operations.
The resource store supplies the immutable processing quantum. No separate engine,
service queue, controller identity or scheduler is introduced.

The integration tests retain full mixed DSP/loop audio and MIDI oracles, live graph
cuts and immutable media reads, indexed startup imports, physical delay markers and
native reference latency windows. Callback bodies retain heap guards. Test hosts
read output only for nonempty callbacks and reclaim retired storage off-render.

Run `cargo nextest run -p shoop_loops_test_support --profile ci` for native coverage.
The portable scenarios also run through `scripts/run_wasm_tests.py --runtime node
--package shoop_loops_test_support`; native reference-channel comparisons remain
native-only. These are correctness fixtures and make no device-deadline claim.
