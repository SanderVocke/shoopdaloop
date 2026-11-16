# Live-loading implementation

Authoritative contract: [LOADING_DESIGN.md](LOADING_DESIGN.md).

- [x] Preserve the accepted design in its own commit.
- [x] Add shared ARM-side allocation, reservation, revision and address binding.
- [x] Encode sync wiring and graph-dependent buffer operands in programs.
- [x] Separate Rust program ownership from persistent state; stage new states only.
- [x] Implement ordered switching, frame continuity, fences and deferred reclamation.
- [x] Update the reference endpoint, RtBackend and application client for live updates.
- [x] Add Rust ownership, state/media/history continuity and protocol tests.
- [ ] Implement RTL instruction regions, preparation arbitration and switch completion.
- [ ] Update Marlin loaders/runtime integration and shared Rust/RTL loading scenarios.
- [ ] Run applicable build/test/resource checks and document limitations/results.

Validation must cover retained recording/playback and planned transitions, changed
sync operands, arbitrary physical placements, new/removed/recycled instances,
cancel/refusal/reset, stale commands, publication cutoffs, response backpressure,
and delayed memory completion. No active-state initialization, full-state copying,
FPGA image unpacking, allocation tables or graph-validation passes are permitted.

Foundation checks: compiler/reference suites pass, including new allocation,
recording/plan/frame carry-over and changed-scratch history comparisons. All seven
compiler/sync RTL tests pass with inline sync-source and smoothing operands. The
warning-denying compiler/reference/Marlin build passes. Reference command FIFO,
delayed handoff, slot reuse and the end-to-end application client pass. The client
replaces graphs during recording and playback and replaces a track with eight new
loops. RTL instruction-region arbitration and switch integration remain.
