- [x] Baseline sanity before next migration steps:
  - [x] `cargo build`
  - [x] `cargo test`
  - [x] backend `test_runner`
  - [x] `./target/debug/shoopdaloop_dev.sh --self-test`
  - nuance: ran as one end-to-end command chain; self-test passed (186/186).

- [x] Phase A: Add regression coverage for decoupled MIDI lifetime/race behavior
  - [x] Add backend tests for close/unregister during active processing
  - [x] Add repeated open/close + message queue stress test
  - [x] Add stale handle access behavior tests in API layer where applicable
  - [x] Milestone: `cargo test`, backend `test_runner`, self-test
    - [x] `cargo test`
    - [x] backend `test_runner`
    - [x] self-test
  - nuance: implemented as new unit test `DummyAudioMidiDriver - decoupled midi open/close stress` in `test_DummyAudioMidiDriver.cpp` (200 open/close/unregister iterations while processing).
  - nuance: added API-level stale-handle safety test in `test_libshoopdaloop_if.cpp`; after close, stale operations are asserted to be safe (no crash) with current returned default values.

- [ ] Phase B: Move decoupled MIDI ownership to Rust-managed lifecycle
  - [ ] Introduce Rust-side decoupled port registry/handle ownership
  - [ ] Refactor C++ decoupled wrapper to thin handle-forwarding role
  - [ ] Route pop/push/process/close through Rust-owned state
  - [ ] Remove C++ decoupled keepalive set once safety is guaranteed
  - [ ] Milestone: `cargo build`, `cargo test`, backend `test_runner`, self-test

- [ ] Phase C: Formalize processor callback handle lifecycle
  - [ ] Replace raw pointer assumptions with explicit stable registration handles/tokens
  - [ ] Guarantee safe unregister semantics under concurrent process-thread activity
  - [ ] Keep trampoline callback behavior but with stricter lifetime boundaries
  - [ ] Milestone: `cargo test`, backend `test_runner`

- [ ] Phase D: Shrink `AudioMidiDriver` C++ to forwarding-only shell
  - [ ] Move remaining mutable state responsibilities to Rust runtime
  - [ ] Keep C++ signatures and virtual API compatibility unchanged
  - [ ] Remove non-essential C++ state containers/logic
  - [ ] Milestone: `cargo build`, `cargo test`, backend `test_runner`, self-test

- [ ] Phase E: Simplify dummy wrapper/template/settings boundary
  - [ ] Route template instantiations to shared Rust backend path
  - [ ] Replace unchecked settings cast with typed bridge/config conversion
  - [ ] Keep wrapper externally compatible and thin
  - [ ] Milestone: `cargo test`, backend `test_runner`, self-test

- [ ] Phase F: Cleanup and final verification
  - [ ] Remove dead code and stale comments
  - [ ] Update docs/comments for Rust-vs-C++ ownership split
  - [ ] Run `cargo fmt --all`
  - [ ] Run `RUSTFLAGS="-D warnings" cargo build`
  - [ ] Run `cargo test`
  - [ ] Run backend `test_runner`
  - [ ] Run `./target/debug/shoopdaloop_dev.sh --self-test`
  - [ ] Confirm end state: all tests pass, no warnings in strict gate, C++ wrappers are thin
