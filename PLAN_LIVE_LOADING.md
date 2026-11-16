# Live-loading implementation

Authoritative contract: [LOADING_DESIGN.md](LOADING_DESIGN.md).

- [x] Preserve the accepted design in its own commit.
- [x] Add shared ARM-side allocation, reservation, revision and address binding.
- [x] Encode sync wiring and graph-dependent buffer operands in programs.
- [x] Separate Rust program ownership from persistent state; stage new states only.
- [x] Implement ordered switching, frame continuity, fences and deferred reclamation.
- [x] Update the reference endpoint, RtBackend and application client for live updates.
- [x] Add Rust ownership, state/media/history continuity and protocol tests.
- [x] Implement RTL instruction regions, preparation arbitration and switch completion.
- [x] Update Marlin loaders/runtime integration and shared Rust/RTL loading scenarios.
- [x] Run applicable build/test/resource checks and document limitations/results.

Validation must cover retained recording/playback and planned transitions, changed
sync operands, arbitrary physical placements, new/removed/recycled instances,
cancel/refusal/reset, stale commands, publication cutoffs, response backpressure,
and delayed memory completion. No active-state initialization, full-state copying,
FPGA image unpacking, allocation tables or graph-validation passes are permitted.

Completed: persistent Rust state, FIFO switching and fences, live application
graphs, two RTL instruction regions, spare-port preparation, media/AXI completion,
and Marlin upload/switch drivers. Tests cover no-heap switches, reclaim
backpressure, fence faults, delayed handoff, slot reuse, empty programs and exact
Rust/RTL samples. The application client replaces graphs during recording and
playback and replaces a track with eight new loops. Measurements, validation
results and the Marlin frontend scope are in
[docs/fpga/live-loading.md](docs/fpga/live-loading.md).
