# Remaining backend work

This checklist tracks backend-related work that remains after replacing and removing the old C/C++ backend. GUI work is intentionally excluded.

## Backend completeness

- [ ] Implement LV2/Carla plugin hosting.
  - [x] Add Lilv-based Carla plugin discovery and legacy URI/port metadata validation.
  - [ ] Instantiate and run Carla LV2 plugins from Rust.
  - [ ] Expose it through the existing `FxChain`/effect interface.
  - [ ] Validate state serialization, UI handling, dry/wet, bypass and tails.
- [x] Repoint the existing frontend/application stack to `shoop_engine`.
- [x] Delete the old C backend API and bindgen layer once no callers remain.
- [x] Complete the current application control API surface over the Rust engine.
  - [x] Provide `FxChain` control handles for the current built-in/test FX interface; plugin-host-specific controls remain part of LV2/Carla work.
  - [x] Provide the `AudioDriver` handle/API used by the frontend.
  - [x] Ensure Python/QML-facing handle shapes remain compatible enough for existing consumers.
- [x] Keep schedule recomputation off the realtime/audio callbacks.
- [x] Finish JACK-specific parity work.
  - [x] Cover JACK port registration.
  - [x] Cover JACK buffer reading/writing.
  - [x] Cover direction-dependent access flags.
  - [x] Validate JACK coverage against a real running JACK server when one is available.

## Test suite completeness

- [ ] Maintain and extend the Rust backend regression suite now that the C++ `test_runner` is gone.
  - [ ] Preserve intentional non-literal translations where the Rust design differs.
  - [ ] Document any remaining behavioural divergences explicitly.
- [x] Add missing JACK driver integration coverage.
  - [x] Do not count dummy ports or `MidiPort` core tests as full JACK coverage.
  - [x] Add tests that exercise actual JACK driver behaviour where practical.
- [ ] Add tests for LV2/Carla once plugin hosting exists.
- [ ] Extend `tests/no_alloc.rs` as more engine paths land.
  - [ ] Cover recording past chunk boundaries.
  - [ ] Cover any newly added process-thread/plugin/control handoff paths.
- [x] Run the existing QML `--self-test` as the final integration gate once the frontend is repointed.

## Current assessment

- [x] `shoop_engine` builds.
- [x] A large part of the C/C++ backend has been reimplemented in safe Rust.
- [x] Core loop, audio, MIDI, session, graph, port, control, driver and resampling pieces exist.
- [x] Many C++ Catch2 tests have been translated and have already found real divergences.
- [x] Mutation testing/no-allocation testing exists for important paths.
- [ ] The Rust engine is not yet a complete drop-in backend replacement.
- [ ] The backend test suite is substantial but not complete.
