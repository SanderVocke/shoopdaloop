# Plan: Fix all exposed external dry/wet routing failures

## Investigation summary

The completed coverage work exposes 13 failures: 11 QML assertions and 2 real-JACK Rust assertions. The current evidence points to five independent problem areas:

| Failure group | Count | Relevant implementation evidence |
|---|---:|---|
| Active MIDI-note cleanup | 3 | `Session::propagate_port()` returns immediately when MIDI passthrough is muted. It does not track what was forwarded or emit cleanup for notes already sent. |
| MIDI replacement | 2 | `MidiChannel::process()` recognizes `ProcessFlags::REPLACE` for snapshot bookkeeping but has no replacement-processing branch corresponding to `AudioChannel::process_replace()`. |
| Monitoring-off defaults/persistence | 2 | Generated explicit wet returns start with `passthrough_muted: false`, while the control initializes monitoring off. Port state is also reapplied asynchronously, so the descriptor and control can race to become authoritative. |
| Carla MIDI/descriptor routing | 4 | `FXChain.qml` recognizes `carla_patchbay_16x`, while the user-facing descriptor is `carla_patchbay_16`. Internal FX MIDI handles create a dummy capture queue but do not attach it to the engine port, so the QML observation path always reads `[]`. |
| Real JACK round trips | 2 | One-way JACK input and output tests pass, but both new bidirectional peer fixtures observe zero output. Connection errors are currently discarded, and each fixture puts source and sink ports on one peer client, creating a client-level feedback topology that must be separated from any application routing defect. |

These findings are starting hypotheses for implementation, not permission to change expected behavior. The regression assertions remain the behavioral contract.

## Goals

- Fix every behavior represented by the 13 documented failing tests.
- Preserve the intended external dry/wet routing matrix and all already-passing coverage.
- Implement coherent MIDI replacement and active-note cleanup in the engine.
- Make monitoring defaults and restored backend port state agree with the track control.
- Support the user-facing Carla Patchbay 16x descriptor and make Carla MIDI delivery observable and functional.
- Prove real-JACK dry/wet audio and MIDI routing with valid, explicitly verified JACK connections.

## Scope

Primary production files:

- `src/qml/TrackControlWidget.qml`
- `src/qml/FXChain.qml`
- `src/qml/js/generate_session.js`
- `src/rust/shoop_engine/src/midi_port.rs`
- `src/rust/shoop_engine/src/midi_storage.rs`
- `src/rust/shoop_engine/src/midi_channel.rs`
- `src/rust/shoop_engine/src/content_snapshot/midi.rs`, if replacement snapshots need a range operation
- `src/rust/shoop_engine/src/session.rs`
- `src/rust/shoop_engine/src/app_backend.rs`

Regression and supporting test files:

- `src/qml/test/tst_TrackControlAndLoop_drywet_external.qml`
- `src/qml/test/tst_Session_save_load_drywet_external.qml`
- `src/qml/test/tst_TrackControlAndLoop_drywet_carla.qml`
- `src/qml/test/tst_drywet_carla_patchbay_16_descriptor.qml`
- `src/rust/shoop_engine/tests/jack_app_backend.rs`
- Focused Rust unit tests beside the affected engine components

Unrelated routing, UI redesign, plugin-host features, and the pre-existing CPAL environmental skip are out of scope.

## Immutable acceptance criteria

1. All 11 failing QML regressions pass without weakening or deleting their intended assertions.
2. Both real-JACK regressions pass on a host where JACK is available; absence may skip only through the existing explicit missing-backend policy.
3. Muting an external MIDI passthrough path emits bounded cleanup at the next process boundary for every note previously forwarded, exactly once, and leaves the external sink with no active notes.
4. Immediate and synchronized mode changes, including forced monitor-off for dry re-recording, do not leave external MIDI notes active.
5. MIDI `Replacing` overwrites events in the processed loop interval, preserves events outside that interval, handles loop-boundary splitting, updates playback state, and publishes only the committed replacement snapshot.
6. Fresh explicit external tracks and saved monitoring-off/on sessions restore one coherent control/backend state for dry audio, dry MIDI, wet returns, and internal FX outputs.
7. Both `carla_patchbay_16` and the canonical backend spelling select `FXChainType.CarlaPatchbay16x`; existing saved descriptors remain compatible.
8. While a Carla chain is active, dry MIDI reaches its host input; while inactive, it is gated. The test must observe the real engine/host input path, not a disconnected queue.
9. The JACK tests check connection success and exercise an acyclic or explicitly latency-broken peer topology while preserving the full dry-send → processor → wet-return → wet-output and monitored/muted MIDI semantics.
10. No audio-thread path allocates, blocks, logs, or grows an unbounded collection. New cleanup/replacement storage is preallocated or bounded and is covered by realtime/no-allocation tests.
11. The 21 already-passing new QML tests, the existing 26 internal dry/wet tests, targeted Carla tests, the Rust workspace, and the complete QML suite remain green, apart from explicitly allowed unavailable-backend skips.
12. Once a regression passes, remove its stale `Failure:` annotation while retaining its purpose and use-case comments.

## Design rules and constraints

- Treat source-input state and externally forwarded MIDI state as distinct. Cleanup must be based on what was actually forwarded, not on notes that arrived while passthrough was muted.
- Emit cleanup through the normal internal MIDI route so Dummy, JACK, and hosted-FX destinations see the same behavior.
- Keep cleanup deterministic at a process boundary; do not send MIDI from the GUI/control thread.
- Define MIDI replacement as a sorted interval splice: remove old events in the replaced half-open interval and insert newly recorded events at loop-relative timestamps while preserving events outside it.
- Keep engine storage and content-snapshot publication consistent. A replacement must remain hidden from readers until commit.
- Establish one initialization authority for monitoring state. Descriptor loading may initialize the control, but late port initialization must not silently undo the effective track routing state.
- Accept both Carla Patchbay 16x spellings at the descriptor boundary; use one canonical representation when generating new descriptors without invalidating old sessions.
- Do not add a user-facing diagnostic API solely for tests. A test-only capture hook may be wired through an existing backend handle only if it observes the same events delivered to the host.
- Before changing JACK production code, prove the peer topology and every JACK connection are valid. Fix a fixture defect if that is the cause; change application routing only when an isolated one-way or latency-broken test demonstrates a production defect.
- Keep unrelated formatting and refactoring out of each milestone.

## Staged implementation

### Stage 0 — Reproduce and isolate the current failures

- [x] Rebuild and rerun the focused QML regressions, recording the exact 11 failing cases and preserving a JUnit/log baseline. Evidence: 53 passed and the same 11 cases failed in `/tmp/drywet-fixes-baseline.log`; JUnit is `/tmp/drywet-fixes-baseline.xml`.
- [x] Rerun the two real-JACK tests both with and without the missing-backend allowance. Evidence: both failed without the allowance in `/tmp/drywet-fixes-jack-baseline.log`; with the allowance, audio failed and MIDI passed once in `/tmp/drywet-fixes-jack-allowed-baseline.log`, confirming the MIDI case is timing-sensitive rather than an environmental skip.
- [x] Confirm Rack, Patchbay, Patchbay 16x, and JACK availability before interpreting any skip. Evidence: all three Carla hosts initialized in the focused QML run and both JACK tests executed real callbacks; no capability was skipped.
- [x] Add no behavior changes in this stage; record whether each hypothesis above is confirmed or revised. Evidence: the four code-grounded hypotheses remain consistent with the reproduced values. JACK MIDI is additionally nondeterministic; JACK audio remains a deterministic zero-output failure, so Stage 6 must validate each connection and remove peer-client feedback before deciding whether production code is implicated.

Verification:

```bash
cargo build

SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_*drywet*.qml" \
  --junit-xml /tmp/drywet-fixes-baseline.xml

cargo test -p shoop_engine --features app_backend,jack \
  --test jack_app_backend external_

SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test -p shoop_engine --features app_backend,jack \
  --test jack_app_backend external_
```

No empty commit is required for baseline-only work.

### Stage 1 — Carla Patchbay 16x descriptor compatibility

- [x] Add descriptor-boundary support for both `carla_patchbay_16` and `carla_patchbay_16x`, mapping both to `CarlaPatchbay16x`.
- [x] Choose and document one canonical generated spelling, while keeping the other as a load-time compatibility alias. Decision: generated/user-facing descriptors remain `carla_patchbay_16`; the schema, generator input, and FX-chain mapping also accept the backend-style `carla_patchbay_16x` alias.
- [x] Remove the temporary descriptor rewrite from the Carla fixture so it exercises the user-facing value end to end.
- [x] Verify Rack and ordinary Patchbay mappings are unchanged. Evidence: all three Carla activation-mode cases passed using their generated descriptors in `/tmp/drywet-fixes-stage1-activation.log`.
- [x] Run the descriptor and Carla activation tests, then commit the milestone. Evidence: both Patchbay 16 spellings passed schema validation and selected `CarlaPatchbay16x` in `/tmp/drywet-fixes-stage1-descriptor-final.log`; all 3 activation cases passed.

Verification:

```bash
cargo build
SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_drywet_carla_patchbay_16_descriptor.qml"
SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_carla.qml" \
  --filter '.*activation_modes.*'
```

### Stage 2 — Coherent monitoring defaults and session restoration

- [x] Make newly generated explicit wet-return and internal FX-output passthrough state agree with monitoring-off defaults.
- [x] Establish a deterministic load sequence in which all relevant ports are initialized before the effective track monitor state is applied. Decision: persisted descriptor state remains the port-initialization authority; generated dry inputs, wet returns, and FX outputs now encode the same monitoring-off value, and later monitor changes update them together.
- [x] Ensure a late port `push_all()` cannot overwrite the control’s effective routing state after fresh creation or reload. Evidence: late descriptor application now reapplies the same coherent value; fresh/off/on backend assertions all pass.
- [x] Preserve monitoring-on and per-port gain/mute restoration. Evidence: monitoring-on persistence and the six existing save/load cases pass.
- [x] Add focused initialization-order coverage if the fix relies on late registry/port availability. Not needed: the fix removes the conflicting initial values rather than adding a timing dependency; existing fresh/load tests exercise late object creation, and Carla activation tests now assert FX-output passthrough state off→on→off.
- [x] Run fresh, monitoring-off, and monitoring-on persistence tests and existing session-save tests, then commit. Evidence: 3/3 external persistence tests passed in `/tmp/drywet-fixes-stage2-persistence.log`, 6/6 existing save/load tests passed in `/tmp/drywet-fixes-stage2-existing-save.log`, and 3/3 Carla FX-output default/activation cases passed in `/tmp/drywet-fixes-stage2-fx-defaults.log`.

Verification:

```bash
cargo build
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_Session_save_load_drywet_external.qml"
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_Session_save_load.qml"
```

### Stage 3 — MIDI replacement semantics

- [ ] Add a bounded `MidiStorage` operation for replacing a half-open timestamp interval without allocating in the process callback.
- [ ] Implement the missing `ProcessFlags::REPLACE` branch in `MidiChannel::process()` using the current process position and input-buffer cursor.
- [ ] Preserve events outside the replaced interval, insert incoming events in timestamp order, and handle negative offsets and loop-boundary splits consistently with audio replacement.
- [ ] Update recording-start/input/playback state so the replaced sequence starts and loops without stale note state.
- [ ] Extend snapshot mutation support as needed so partial replacement is hidden until commit and publishes the same events as engine storage.
- [ ] Add Rust unit tests for full replacement, partial replacement, empty-input erasure, boundary splitting, state restoration, capacity behavior, and snapshot publication.
- [ ] Run the two QML Replacing cases plus Rust MIDI/channel/content tests, then commit.

Verification:

```bash
cargo test -p shoop_engine --features app_backend midi_storage
cargo test -p shoop_engine --features app_backend midi_channel
cargo test -p shoop_engine --features app_backend content_snapshot::midi

cargo build
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_external.qml" \
  --filter '.*external_matrix_replacing.*'
```

### Stage 4 — Active MIDI-note cleanup on passthrough gating

- [ ] Track the MIDI state actually forwarded by each passthrough source independently from its incoming/capture state.
- [ ] On an unmuted-to-muted transition, queue bounded note cleanup for delivery through normal internal connections at the next process boundary.
- [ ] Clear forwarded state only after cleanup is emitted; do not repeatedly emit cleanup on later muted cycles.
- [ ] Handle monitor toggles, immediate mode changes, synchronized boundaries, forced monitoring-off, and mute/unmute changes before the next cycle.
- [ ] Add engine unit tests for one note, multiple channels/notes, already-released notes, repeated mute, and events received while muted.
- [ ] Run the four cleanup regressions and relevant no-allocation tests, then commit.

Verification:

```bash
cargo test -p shoop_engine --features app_backend midi_port
cargo test -p shoop_engine --features app_backend session
cargo test -p shoop_engine --features app_backend --test no_alloc

cargo build
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_external.qml" \
  --filter '.*midi_cleanup.*'
```

### Stage 5 — Carla MIDI host delivery and observation

- [ ] Trace one monitored MIDI event from the external dry input through `Session::propagate_port()` into the internal FX MIDI port and Carla host input.
- [ ] Wire the existing backend MIDI capture handle to internal FX MIDI ports, or replace it with an equally bounded observation of the exact events delivered to the host.
- [ ] Ensure observation does not consume, duplicate, reorder, or bypass host delivery.
- [ ] Add a Rust test proving active host input receives MIDI and inactive host input does not.
- [ ] Run all three QML Carla MIDI-gating cases and the targeted Rust Carla suite, then commit.

Verification:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test -p shoop_engine --features app_backend,lv2 carla

cargo build
SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet_carla.qml" \
  --filter '.*midi_activation_gating.*'
```

### Stage 6 — Real-JACK dry/wet audio and MIDI round trips

- [ ] Make every peer-to-application JACK connection checked and fail with the concrete JACK error or missing connection name.
- [ ] Split MIDI source and sinks into separate peer clients so the test graph is acyclic, then confirm monitored delivery and muted silence.
- [ ] Split the audio processor’s receive and return sides, or add an explicit bounded one-cycle handoff, so JACK can schedule the external insert without an implicit client feedback cycle.
- [ ] Preserve causal end-to-end proof: wet return must begin only after dry send is observed, and the application wet output must contain the transformed marker.
- [ ] Add intermediate assertions for source→dry input, dry send→processor, processor→wet return, and wet output→consumer so a future failure identifies the broken leg.
- [ ] If a valid topology still fails, isolate the failing application leg with a one-way port-to-port test and fix the corresponding JACK staging, session propagation, or output publication path.
- [ ] Avoid blocking or allocation in peer callbacks by using preallocated bounded handoff/capture state.
- [ ] Run all six JACK integration tests with and without the missing-backend allowance, then commit.

Verification:

```bash
cargo test -p shoop_engine --features app_backend,jack \
  --test jack_app_backend

SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test -p shoop_engine --features app_backend,jack \
  --test jack_app_backend
```

### Stage 7 — Remove stale failure annotations and run focused integration

- [ ] Rerun every formerly failing assertion individually and confirm all 13 now pass.
- [ ] Remove only the now-stale `Failure:` lines; retain purpose, use-case, and behavioral assertions.
- [ ] Run all focused external dry/wet QML tests together.
- [ ] Run the existing internal dry/wet suites and targeted Rust JACK/Carla tests.
- [ ] Confirm no environmental skip is used on the currently available JACK/Carla host.
- [ ] Commit the focused integration milestone.

Verification:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_*drywet*.qml" \
  --junit-xml /tmp/drywet-fixes-focused.xml

QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControlAndLoop_drywet.qml"
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  -f "${PWD}/src/qml/test/tst_TrackControl_drywet.qml"

SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test -p shoop_engine --features app_backend,jack --test jack_app_backend
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test -p shoop_engine --features app_backend,lv2 carla
```

### Stage 8 — Final end-to-end validation

- [ ] Run formatting and a warnings-as-errors build.
- [ ] Run the complete Rust workspace suite serially if host resource contention recurs.
- [ ] Run the complete QML suite and save its JUnit/log result.
- [ ] Confirm all 235 QML cases pass except explicitly unavailable backend capabilities; track the unrelated CPAL capability separately.
- [ ] Confirm the Rust workspace has no behavioral failures on available JACK/Carla backends.
- [ ] Review the final diff for realtime safety, compatibility aliases, stale failure comments, unrelated formatting, and test weakening.
- [ ] Update this plan with final evidence and commit the completed validation stage.

Verification:

```bash
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build

SHOOP_ALLOW_MISSING_BACKENDS=1 RUST_TEST_THREADS=1 \
  cargo test --workspace --features shoop_engine/app_backend

SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh --self-test \
  --junit-xml /tmp/drywet-fixes-final.xml

git diff --check
git status --short
```

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
