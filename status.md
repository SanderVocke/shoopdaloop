# Overall Status

## Objective

The coverage plan in `plan.md` is fully executed. All stages and checklist items are marked complete with evidence. This work added tests and documented exposed failures; it intentionally did not fix production behavior.

## Delivered

- Added 32 QML tests covering:
  - Active external MIDI-note cleanup.
  - The complete external dry/wet mode and monitoring matrix.
  - Sample-accurate synchronized routing boundaries.
  - Multiple loops sharing one external processor track.
  - Fresh-track defaults and save/load persistence.
  - Carla Rack, Patchbay, and Patchbay 16x activation and MIDI routing.
- Added 2 real-JACK Rust integration tests covering:
  - Audio dry-send → external processor → wet-return → wet-output.
  - Shared MIDI source fanout to monitored and passthrough-muted tracks.
- Every new test has an adjacent purpose and use-case comment.
- Every failing new test retains its intended assertion and has an adjacent comment with expected output, actual output, and a potential root cause.
- No production fixes were made.
- The goal-range diff from `3b29fc80` contains only `plan.md`, QML test files, and the Rust JACK integration test.

## New Test Results

- New QML tests: 32 total
  - 21 passed.
  - 11 failed as intended regression coverage.
- New Rust JACK tests: 2 total
  - 2 failed as intended regression coverage.
- Total newly exposed/documented failures: 13.

### Documented failure groups

1. **External MIDI note cleanup — 3 failures**
   - Disabling monitoring after note-on emits no cleanup event.
   - Immediate Recording → Playing with a held note emits no cleanup event.
   - Dry re-recording forcing monitoring off emits no cleanup event.
   - Observed output: `[]` instead of note-off, zero-velocity note-on, CC120, or CC123.
   - Synchronized Recording → Playing cleanup passed.

2. **Replacing dry MIDI — 2 failures**
   - Monitoring off and monitoring on both retain loaded dry MIDI instead of replacement input.
   - Expected note 80 at times 0/3; observed old note 81 at times 1/2.

3. **External wet-return defaults and persistence — 2 failures**
   - Fresh monitoring-off external tracks leave the wet return unmuted.
   - Saving/loading monitoring off also leaves the wet return unmuted.
   - Expected `passthrough_muted=true` and `[0,0,0,0]`; observed `false` and `[10,20,30,40]`.
   - Monitoring-on persistence passed.

4. **Carla MIDI and descriptor routing — 4 failures**
   - Rack, Patchbay, and Patchbay 16x active MIDI inputs each captured `[]` instead of `{time:0,data:[0x90,72,100]}`.
   - User-facing `carla_patchbay_16` selected `FXChainType.CarlaRack` (0) instead of `CarlaPatchbay16x` (2).
   - Activation/deactivation mode coverage passed for all three installed Carla variants.

5. **Real JACK round trips — 2 failures**
   - Audio expected transformed sample `2.0`; observed maximum `0` across 96,256 captured samples.
   - MIDI expected monitored note-on/note-off and muted-track `[]`; observed both paths as `[]`.
   - The four pre-existing JACK integration tests passed.

## Final Validation

- Focused dry/wet QML run:
  - 64 total.
  - 53 passed.
  - 11 documented failures.
  - Log: `/tmp/drywet-focused-final.log`
  - JUnit: `/tmp/drywet-focused-final.xml`
- Existing internal dry/wet suites:
  - Loop suite: 16/16 passed.
  - Control suite: 10/10 passed.
  - Logs: `/tmp/drywet-internal-loop-final.log`, `/tmp/drywet-internal-control-final.log`
- Full QML run:
  - 235 total.
  - 223 passed.
  - 11 documented failures.
  - 1 unrelated CPAL environmental skip.
  - Log: `/tmp/drywet-expanded-final.log`
  - JUnit: `/tmp/drywet-expanded-final.xml`
- Rust workspace run:
  - Reached the JACK integration target.
  - Four existing JACK tests passed.
  - Two new documented JACK tests failed.
  - Log: `/tmp/drywet-rust-workspace-final.log`
- Nine targeted Rust Carla tests passed.
  - Log: `/tmp/drywet-stage7-rust.log`
- `cargo fmt --all` passed.
- `RUSTFLAGS="-D warnings" cargo build` passed.
- `git diff --check` passed.
- Worktree was clean at completion before this status file was created.

## Files Added

- `plan.md`
- `src/qml/test/tst_TrackControlAndLoop_drywet_external_transitions.qml`
- `src/qml/test/tst_TrackControlAndLoop_drywet_external_multiple.qml`
- `src/qml/test/tst_Session_save_load_drywet_external.qml`
- `src/qml/test/tst_TrackControlAndLoop_drywet_carla.qml`
- `src/qml/test/tst_drywet_carla_patchbay_16_descriptor.qml`

## Files Extended

- `src/qml/test/tst_TrackControlAndLoop_drywet_external.qml`
- `src/rust/shoop_engine/tests/jack_app_backend.rs`

## Milestone Commits

- `94aa3905` Add external dry-wet coverage plan
- `f4c6d8f1` Record dry-wet coverage baseline
- `e11053b4` Test external MIDI note cleanup
- `b2c3acb8` Cover external dry-wet mode matrix
- `f57a7b52` Test external dry-wet sync boundaries
- `56a4387c` Test external dry-wet multi-loop routing
- `a4bdf9d0` Test external dry-wet monitor persistence
- `2234d5b9` Test real JACK external dry-wet routes
- `86de90a2` Test Carla dry-wet activation variants
- `d6fab4d2` Complete external dry-wet coverage plan
- `3f21be07` Finalize dry-wet coverage plan record

## Recommended Follow-up

Production fixes can now be implemented independently for each documented failure group. Preserve the intended assertions and rerun the focused tests first, followed by the full QML and Rust suites. The unrelated CPAL playback-port skip should remain tracked separately.
