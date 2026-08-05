I read:

- `.agents/prompts/write_plan.md`
- `.agents/index.md`
- `.agents/rules/mandates.md`
- `.agents/rules/style.md`
- `.agents/info/test.md`
- `.agents/info/build.md`

No implementation changes were made.

# Plan: Complete dry/wet external-routing test coverage

## Goals

- Add regression coverage for the identified external dry/wet routing gaps.
- Exercise routing through QML-controlled tracks with deterministic Dummy-backend tests.
- Add environment-dependent integration coverage for real JACK and Carla variants where QML alone cannot provide a deterministic peer.
- Run every new test and preserve intended assertions even when they expose bugs.
- Document failures in each affected test’s comment without fixing production behavior.

## Scope

Primary files:

- `src/qml/test/tst_TrackControlAndLoop_drywet_external.qml`
- New focused QML files for transitions, multiple loops, persistence, and Carla if splitting keeps fixtures manageable.
- `src/rust/shoop_engine/tests/jack_app_backend.rs` or a new test-only JACK integration file.
- A new test-only Carla integration file if QML cannot observe the required backend behavior deterministically.

Production routing code is out of scope.

## Immutable acceptance criteria

1. Every new test has an adjacent concise comment containing:
   - **Purpose:** what behavior is asserted.
   - **Use case:** the user workflow represented.
2. Every new test is run.
3. A failing test retains its intended assertion and has its comment extended with:
   - Exact expected result.
   - Exact observed result or initialization error.
   - A plausible potential root cause.
4. No failing test is skipped, weakened, or changed to match faulty behavior.
5. Environmental skips are allowed only for unavailable real JACK or Carla dependencies and must identify the unavailable capability.
6. No production fix is made as part of this work.
7. The external mode matrix covers audio dry send, MIDI dry send, wet return/output, and relevant recorded channel contents.
8. Synchronized boundaries, active MIDI-note cleanup, multi-loop routing, persistence/defaults, real JACK routing, and Carla activation are each covered.
9. All pre-existing failures are distinguished from failures introduced by the new assertions.

## Expected external routing matrix

| Loop mode | Monitoring off | Monitoring on |
|---|---|---|
| Stopped | No live dry sends; wet return silent | Live dry sends and wet return audible |
| Recording | Live dry sends; dry/wet content recorded; wet return not heard | Same recording plus wet return heard |
| Replacing | Live dry sends; dry/wet content replaced; wet return not heard | Same replacement plus wet return heard |
| Playing | No dry sends; only recorded wet playback heard | Live dry sends; recorded wet playback plus wet return heard |
| PlayingDryThroughWet | Recorded dry content sent; live input excluded; wet return heard | Recorded dry plus live input sent; wet return heard |
| RecordingDryIntoWet | Recorded dry content sent; live input excluded; wet return heard and replaces wet content | Monitoring is forced off; otherwise identical |

## Design rules and constraints

- Use controlled Dummy-backend frames for sample-accurate QML assertions.
- Prefer separate named tests over one opaque loop over scenarios, so failures identify the exact mode and monitoring state.
- Use shared helpers only for setup, queueing, collection, and comparison; expected behavior remains explicit in each test.
- Use distinct sample values and MIDI notes for live input, recorded dry content, recorded wet content, and wet return.
- MIDI cleanup assertions may accept explicit note-off, note-on with zero velocity, or channel-appropriate all-notes/all-sound-off, but must prove the external sink has no active note.
- Exact boundary timing belongs in controlled Dummy tests; real JACK tests only require eventual end-to-end delivery because server buffering is nondeterministic.
- If existing test APIs cannot observe a required behavior without adding production instrumentation, stop and report that specific blocker rather than adding behavioral code.
- Keep unrelated formatting and refactoring out of the changes.

## Failure-documentation procedure

After each focused run:

1. Capture the failing assertion and concrete values.
2. Extend the test comment, for example:
   - `Purpose: ...`
   - `Use case: ...`
   - `Failure: expected [...], observed [...]. Potential root cause: ...`
3. Do not alter the assertion.
4. Rerun the test to ensure the documentation edit did not introduce a syntax/setup error.
5. Commit the documented failing test as part of its stage.

## Staged implementation

### Stage 0 — Baseline and fixture design

- [x] Run the current full QML suite and save a JUnit/log baseline. Evidence: 202 passed, 0 failed, 1 CPAL skip in `/tmp/drywet-baseline.log`; JUnit in `/tmp/drywet-baseline.xml`.
- [x] Confirm the existing six external tests still pass. Evidence: 6 passed in `/tmp/drywet-external-baseline.log`.
- [x] Define distinct audio/MIDI markers and reusable collection helpers. Decision: use disjoint single-digit audio sequences and MIDI note ranges per source, with queue/process/dequeue helpers in each focused fixture.
- [x] Decide whether the external fixture should gain multiple loops or whether transition/multi-loop tests need dedicated files. Decision: keep the single-loop matrix and cleanup cases in the existing file; use dedicated transition, multiple-loop, persistence, and Carla files.
- [x] Record available JACK and Carla capabilities before writing conditional integration tests. Evidence: real JACK registration test passed; Carla Rack discovery passed; available backends are dummy, JACK, JACK test, CPAL, and CPAL test.

Verification:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  --junit-xml /tmp/drywet-baseline.xml

QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_external.qml"
```

No empty commit is required for baseline-only work.

### Stage 1 — Active MIDI-note cleanup

- [x] Add a test for monitor-on note-on followed by monitor-off before note-off.
- [x] Add a test for a held note across immediate Recording→Playing.
- [x] Add a test for a held note across synchronized Recording→Playing.
- [x] Add a test for monitoring being forced off when entering dry re-recording with a live note active.
- [x] Reconstruct external sink note state and assert no active note remains after each transition.
- [x] Run, document failures, rerun, and commit this milestone. Evidence: synchronized cleanup passed; monitor-off, immediate play, and forced re-record cleanup failed with `[]` and are documented inline; final log `/tmp/drywet-stage1-final.log`.

Verification:

```bash
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_external*.qml" \
  --filter '.*midi.*(cleanup|held|mute|boundary).*'
```

### Stage 2 — Complete external mode matrix

- [x] Add monitoring-off and monitoring-on tests for Stopped.
- [x] Add monitoring-off and monitoring-on tests for Recording.
- [x] Add monitoring-off and monitoring-on tests for Replacing.
- [x] Complete normal Playing coverage for both monitoring states.
- [x] Add both monitoring states for PlayingDryThroughWet.
- [x] Add requested monitoring-off/on cases for RecordingDryIntoWet and assert monitoring is forced off.
- [x] For every case, inspect:
  - Audio dry send.
  - MIDI dry send.
  - Wet return contribution to wet output.
  - Dry/wet recorded content where the mode records or replaces.
- [x] Run, document failures, rerun, and commit. Evidence: 10 matrix cases passed; both Replacing cases retained old loaded MIDI instead of replacement input and are documented inline; final log `/tmp/drywet-stage2-final.log`.

Verification:

```bash
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_external*.qml"
```

### Stage 3 — Synchronized boundary timing

- [ ] Add Stopped→Recording coverage with unique markers immediately before and at the sync boundary.
- [ ] Add Recording→Playing coverage proving dry input closes at the boundary without one-buffer leakage.
- [ ] Add Playing→RecordingDryIntoWet coverage proving the first wet-return frame is captured and live input is excluded.
- [ ] Assert audio samples and MIDI timestamps on both sides of each boundary.
- [ ] Include held-note cleanup in the boundary assertions rather than testing only complete note pairs.
- [ ] Run, document failures, rerun, and commit.

Verification:

```bash
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_external*transition*.qml"
```

### Stage 4 — Multiple loops on one track

- [ ] Add one loop Recording while another plays wet.
- [ ] Add normal wet playback alongside PlayingDryThroughWet.
- [ ] Add normal wet playback alongside RecordingDryIntoWet.
- [ ] Assert aggregate `any_loop_*` routing behavior through actual send/return data, not only QML properties.
- [ ] Verify which loop records or replaces content and which contributes playback.
- [ ] Include MIDI sends where dry MIDI playback or live input is relevant.
- [ ] Run, document failures, rerun, and commit.

Verification:

```bash
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_external*multiple*.qml"
```

### Stage 5 — Defaults and session persistence

- [ ] Add a fresh explicit-external track test asserting the effective initial monitoring state of dry inputs and wet returns.
- [ ] Add save/load coverage for monitoring off.
- [ ] Add save/load coverage for monitoring on.
- [ ] After loading, verify both control-widget state and backend `passthrough_muted` state for every dry input and wet return.
- [ ] Verify routing with actual queued data after reload, not descriptor equality alone.
- [ ] Run, document failures, rerun, and commit.

Verification:

```bash
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_*save_load*drywet_external*.qml"
```

### Stage 6 — Real JACK round trip

- [ ] Reuse the existing raw JACK peer-client test pattern.
- [ ] Add an explicit external dry-send→peer processor→wet-return→wet-output audio round trip.
- [ ] Add a MIDI source fanout case with monitored and input-muted routes.
- [ ] Assert eventual transformed audio and MIDI delivery without assuming exact JACK-cycle alignment.
- [ ] Fail by default when JACK is expected but broken; skip only under the existing explicit missing-backend policy.
- [ ] Run with and without the missing-backend allowance where the host supports JACK.
- [ ] Apply the same purpose/use-case/failure comments to Rust tests.
- [ ] Run formatting and warning checks, then commit.

Verification:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test -p shoop_engine --features app_backend,jack \
  --test jack_app_backend

cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

### Stage 7 — Carla Rack and Patchbay variants

- [ ] Add QML coverage for `carla_rack`, `carla_patchbay`, and `carla_patchbay_16`.
- [ ] For each available variant, assert FX activation for:
  - Stopped with monitoring off/on.
  - Recording and Replacing.
  - Normal Playing with monitoring off/on.
  - Dry playback.
  - Dry re-recording.
- [ ] Assert dry MIDI reaches the FX input only while routing requires the processor to be active.
- [ ] Assert deactivation prevents unintended processing/tails where the installed host provides a deterministic observable output.
- [ ] Report each unavailable Carla variant explicitly rather than treating it as a behavioral pass.
- [ ] Run QML and targeted Rust Carla tests, document failures, rerun, format/build, and commit.

Verification:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_*drywet*carla*.qml"

SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test -p shoop_engine --features app_backend,lv2 carla

cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

### Stage 8 — Final end-to-end validation

- [ ] Run all focused external dry/wet tests together.
- [ ] Run existing internal dry/wet suites to detect fixture or expectation regressions.
- [ ] Run the complete QML suite.
- [ ] Run the Rust workspace suite because JACK/Carla test code was added.
- [ ] Produce a final list of:
  - Passing new tests.
  - Failing new tests and their documented expected/actual behavior.
  - Environmental skips.
  - Unrelated baseline failures.
- [ ] Confirm no production code was changed and the worktree contains only committed test milestones.

Commands:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  --junit-xml /tmp/drywet-expanded-final.xml

SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test --workspace --features shoop_engine/app_backend

git diff --check
git status --short
```

A nonzero test result caused by a newly exposed, correctly documented regression does not trigger a production fix in this work.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
