# Retired state-aware MIDI measurements

These programs use the removed MIDI session, state, and phase-job APIs. They are
archived measurement recipes, not Cargo examples, and their old numbers do not
describe the new data-job implementation. Cargo does not discover them here.

Use `cargo run -p rt_fpga_reference --example midi_loop` for the active functional
example. New timing benchmarks should exercise the new common JP data jobs.

The shared old `examples/support/session_64.rs` fixture stays at its existing path
because the unported RTL benchmark still imports it. It is not an active example.
