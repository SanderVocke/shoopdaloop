# Remaining backend work

This checklist tracks backend-related work that remains after replacing and removing the old C/C++ backend. GUI work is intentionally excluded.

## Backend completeness

- [ ] Implement LV2/Carla plugin hosting.
  - [ ] Keep plugin hosting behind the planned C++/Rust boundary for now.
  - [ ] Expose it through the existing `FxChain`/effect interface.
  - [ ] Validate state serialization, UI handling, dry/wet, bypass and tails.
- [x] Repoint the existing frontend/application stack to `shoop_engine`.
- [x] Delete the old C backend API and bindgen layer once no callers remain.
- [ ] Complete the control API surface over the Rust engine.
  - [ ] Add/finish `FxChain` control handles once plugin hosting is settled.
  - [ ] Add/finish the `AudioDriver` handle/API.
  - [ ] Ensure Python/QML-facing handle shapes remain compatible enough for existing consumers.
- [ ] Move or otherwise handle schedule recomputation so it does not occur on the audio thread.
- [ ] Finish JACK-specific parity work.
  - [ ] Cover JACK port registration.
  - [ ] Cover JACK buffer reading/writing.
  - [ ] Cover direction-dependent access flags.
  - [ ] Validate against a real running JACK server, not only delegated core logic.

## Test suite completeness

- [ ] Continue translating the C++ Catch2 backend suite into Rust tests.
  - [ ] Treat the C++ `test_runner` as the differential oracle while it still exists.
  - [ ] Preserve intentional non-literal translations where the Rust design differs.
  - [ ] Document any remaining behavioural divergences explicitly.
- [ ] Add missing JACK driver integration coverage.
  - [ ] Do not count dummy ports or `MidiPort` core tests as full JACK coverage.
  - [ ] Add tests that exercise actual JACK driver behaviour where practical.
- [ ] Add tests for LV2/Carla once plugin hosting exists.
- [ ] Extend `tests/no_alloc.rs` as more engine paths land.
  - [ ] Cover recording past chunk boundaries.
  - [ ] Cover any newly added process-thread/plugin/control handoff paths.
- [ ] Run the existing QML `--self-test` as the final integration gate once the frontend is repointed.

## Current assessment

- [x] `shoop_engine` builds.
- [x] A large part of the C/C++ backend has been reimplemented in safe Rust.
- [x] Core loop, audio, MIDI, session, graph, port, control, driver and resampling pieces exist.
- [x] Many C++ Catch2 tests have been translated and have already found real divergences.
- [x] Mutation testing/no-allocation testing exists for important paths.
- [ ] The Rust engine is not yet a complete drop-in backend replacement.
- [ ] The backend test suite is substantial but not complete.
