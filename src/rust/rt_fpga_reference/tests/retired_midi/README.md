# Retired state-aware MIDI fixtures

These fixtures describe the removed MIDI session and instrument-state APIs. They
are kept as behavioral references for possible later work, not active tests.
Cargo does not discover Rust files in this subdirectory. `loading_capture.rs`
contains the former state-aware MIDI image-switch test.

The active event-only tests are `midi_data.rs`, `midi_content.rs`, and the explicit
MIDI data-job test in `loading.rs`, one directory above. The shared old MIDI session
fixture is still present under `tests/support` for the unported RTL testbench.
