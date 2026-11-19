# New MIDI engine in the Rust FPGA reference: first milestone

## Goal and scope

Build the MIDI state component and a minimal record/play path directly in
`rt_fpga_reference`. Use simple event storage and the refined musical-state rules
from the existing MIDI implementations. Keep operations suitable for a later
hardware implementation, while writing a functional Rust model rather than a
clock-by-clock simulator.

Baseline: `feat/backend-replacement` at `9eb71d0406af63012e5f55a17ab5bfe994366838`.
Working branch: `feat/fpga-reference-midi`.

The milestone covers one MIDI loop, one input and one output, all 16 MIDI channels,
and record, play from the beginning, stop, and repeat. The demonstration must run
through the existing Rust reference pipeline and use its loop timing. Programmatic
configuration and test drivers are sufficient; application/UI integration is a
later milestone.

This milestone excludes arbitrary-position seeks, rolling history, overdubbing,
general MIDI routing or merging, retained exports, live replacement of MIDI
graphs, file conversion, and SystemVerilog changes. Existing audio behavior and
the production native MIDI backend remain intact. Later work can add those
features without replacing the new state component or event representation.

## Starting points and code ownership

| Source | Use |
| --- | --- |
| Current `rt_fpga_reference` | Keep its pipeline, loop timing, commands, and fault boundaries. Add a focused `midi` module within this crate. |
| Master, `bebdd422` | Use its ordered event sequence and cursor as the storage design reference. Do not import its engine integration, output growth, or sorting machinery. |
| MIDI RTL branch, `1d081bf8` | Port selected message validation, stepped state updates, cleanup/recovery rules, and relevant tests. Do not import its directory tree, version store, or position cache. |
| Backend replacement MIDI | Use its refined state rules and existing tests to check behavior, particularly pedals, resets, pressure, and instrument/bank selection. Do not import its semantic indexes. |

Useful sources are `rt_runtime/src/midi_state.rs`, the MIDI RTL branch's
`rt_runtime/src/midi_state/step.rs` and `rt_runtime/src/midi/short.rs`, and
`shoop_loops/src/midi_restore.rs`. Adapt selected code to reference-owned types;
keep `rt_runtime` and `shoop_loops` as test dependencies of the reference crate.
Record the source revisions and meaningful changes beside ported code or tests.

## Acceptance criteria — fixed for this milestone

1. A Rust reference pipeline fixture records nontrivial timestamped MIDI, stops,
   plays the recording from its beginning, repeats it, and stops again. Playback
   preserves stored bytes, offsets within a processing block, and the order of
   events sharing a timestamp. No MIDI-specific clock advances loop time.
2. State correctly handles note-on/off, zero-velocity note-on, repeated notes,
   sustain, sostenuto, Hold 2, note/channel pressure, pitch bend, program changes,
   pending versus selected banks, channel-mode controls, and controller/system
   resets. State describes MIDI protocol observations, not synthesizer voices.
3. Starting playback restores the recording's known initial state before its
   first event. Stop and repeat perform ordered cleanup/restoration so held or
   pedal-latched notes cannot leak across playback boundaries. Unknown controller
   values are not invented.
4. Storage, state work, and output have configured capacities. After preparation,
   MIDI processing, state progress, cleanup, and playback allocate no heap memory.
   Work that touches many notes or controllers exposes resumable progress and
   measurable work counts; it is not hidden inside a supposedly constant step.
5. Invalid input, capacity exhaustion, and refused output have explicit outcomes.
   Refused output does not skip cleanup/recovery messages, and an incomplete
   recording is never reported as complete. An exhausted store preserves already
   accepted records. No supported input is silently discarded.
6. Audio-only fixtures produce identical audio and loop observations. A combined
   audio/MIDI fixture preserves expected audio while MIDI records and plays.
7. MIDI execution is available only in the Rust reference path. Existing hardware
   targets and app-facing graph compilation continue to refuse MIDI until their
   own integration is implemented and tested. No new hardware-support claim is
   made by this milestone.

The initial managed-message scope is ordinary MIDI 1.0 channel messages plus
System Reset. Parameter-editing controllers (CC 6, 38, and 96–101), SysEx, and
system-common/realtime messages other than System Reset receive explicit
unsupported results at this component boundary. There is no implicit passthrough
or silent filtering. Document this limited first-milestone policy in the API and
README; it is not a compatibility claim about the full application.

## Design rules

- Separate message/state interpretation, event storage, and loop execution. A
  state update must not depend on the choice of array, ring, or future blocks.
- Keep the crate `no_std` with `alloc` and its existing safe-Rust boundary. Allocate
  buffers during preparation. Use checked, fixed-width logical IDs, timestamps,
  and counters; ordinary Rust structs are sufficient without freezing a hardware
  binary layout now.
- Begin with a capacity-limited flat event buffer per take, a saved initial state,
  and a playback cursor. Recording appends ordered inline messages. Do not add
  semantic trees, paging, reference counting, or a general storage abstraction.
- Track observed supported input while stopped, so record start saves the state
  immediately before its first captured event. Recording starts a new take; reject
  an attempt to overwrite a take still in use before changing it. More elaborate
  take lifetimes belong to later work.
- Use half-open processing windows: an event at a window's end belongs to the
  next window. Preserve sample offsets inside each window. Existing loop timing
  supplies record/play/stop and repeat boundaries.
- Let state updates and cleanup/restoration advance in bounded steps. Count work
  in meaningful units such as record reads, note/controller visits, and accepted
  output messages. A Rust step is not a promise of one future hardware cycle.
- Keep pending state private until the operation completes. Advance output
  progress only after the sink accepts the message. Specify how recovery blocks
  ordinary MIDI emission and how unrecoverable errors use existing pipeline faults.
- Preserve existing audio APIs where practical. Add explicit MIDI bindings and
  buffers to the Rust reference execution path; do not route MIDI through audio
  sample storage or create a second loop scheduler.
- Keep shared compiler and RTL capability checks honest. This milestone may use
  programmatic Rust bindings; it does not require broadening the shared graph
  compiler or implementing host/UI MIDI ports.
- Use existing implementations for comparison, plus independently specified
  timelines and a small test-only note/pedal model. Ported code and ported tests
  alone are insufficient evidence of correctness.

## Staged implementation

### 1. Establish the branch, contracts, and baseline

- [x] Create `feat/fpga-reference-midi` from the baseline above. Preserve unrelated
  working-tree changes and record any later base update.
- [x] Read repository style, build, and test guidance before implementation.
- [x] Define the message policy, event ordering, take ownership, capacity/failure
  outcomes, and cleanup order. Identify the smallest Rust pipeline integration
  point using the existing loop render state.
- [x] Record passing baseline reference pipeline, transition, and compiler tests.
  Commit the contracts and any necessary test fixtures as a reviewable milestone.

Verification: demonstrate the baseline audio fixture and retain the current MIDI
refusal tests. No new production behavior is enabled in this stage.

### 2. Add the message and musical-state component

- [x] Port the selected validator and state rules into `rt_fpga_reference::midi`,
  preserving known/unknown values and the distinction between pending and loaded
  instrument banks. Keep repeated events in the stream even when state retains
  only the latest velocity for a key.
- [x] Add resumable update, cleanup, restoration, and recovery operations with
  explicit progress and output acceptance. Preserve source attribution/revisions.
- [x] Add independent examples for pedal-held notes, sostenuto capture/release,
  resets, bank/program ordering, channel isolation, and repeated notes. Check
  supported cases against the existing MIDI state implementation.

Verification: focused message/state tests, malformed/unsupported input tests,
work-step assertions, refusal/resume tests, and heap guards after preparation.
Commit after these checks pass; do not run the whole workspace on every iteration.

### 3. Add the simple take store and playback cursor

- [x] Add bounded inline event records, append-only capture, saved initial state,
  recording length, and a sequential playback cursor.
- [x] Validate timestamps and stable equal-time ordering. Define and test record
  start/finalization and zero-event recordings without relying on event count as
  the recording duration.
- [x] Preserve accepted records on exhaustion; return the failure and completed
  extent explicitly. Refuse invalid take reuse before mutation.
- [x] Test exact round trips, same-time note/control sequences, window boundaries,
  minimal capacities, exhausted storage, and counter overflow.

Verification: small-capacity unit tests against a plain test-only event list,
byte/order equality, and allocation guards. Commit before pipeline integration.

### 4. Connect record/play to the Rust reference pipeline

- [x] Add prepared MIDI bindings and input/output buffers for one loop. Consume
  existing loop render decisions alongside audio execution, with no duplicate
  transition resolver or frame clock.
- [x] Implement record, stop/finalize, play from the beginning, repeat, initial
  restoration, and stop cleanup. Refuse unsupported operations such as a nonzero
  playback start before changing MIDI state.
- [x] Keep output/recovery progress on refusal. Distinguish MIDI capacity results
  from fatal pipeline faults, preserve the existing audio fault contract, and
  expose enough status to prevent a partial capture from appearing successful.
- [x] Add a combined pipeline fixture and confirm existing shared compiler and
  hardware targets still refuse MIDI graphs.

Verification: exact output traces at Q=1/16/32/64/128, including equal-time events,
events immediately before a boundary, empty takes, repeated wraps, pedal-held
stops, full output, and refusal followed by recovery. Compare multi-quantum calls
with equivalent single-quantum calls. Commit the integrated path when green.

### 5. Validate locally and deliver the PR

- [x] Run the complete milestone fixture through the Rust reference pipeline:
  observe input while stopped, record notes/pedals/controllers, stop, play, repeat,
  stop, and start a new take. Check bytes, timestamps, state, and accepted output
  against an independently specified trace.
- [x] Run audio-only regressions and the combined audio/MIDI fixture. Exercise
  storage/output exhaustion and recovery with small capacities; verify supported
  processing and recovery perform no allocations after preparation.
- [x] Record configured capacities, memory footprint, and work counts for ordinary
  notes, pedal release, reset, and cleanup/restoration. Include a small dense-event
  timing measurement as descriptive evidence, without claiming FPGA cycle counts
  or a hardware deadline.
- [x] Update the reference README and this plan with supported operations, message
  policy, limits, test commands, results, and the next integration boundaries.
- [x] Run formatting, the test-policy checker if Rust tests changed, a
  warning-denying workspace build, and the required native suite before pushing.
  Follow `.agents/info/build.md` and `.agents/info/test.md`; use the Nix development
  shell when applicable. Run targeted checks during development and the broader
  gates once the integrated implementation is stable.
- [x] Commit the completed milestone, push the feature branch, and open a PR
  against `feat/backend-replacement`. Keep its description scoped to Rust reference
  MIDI and include validation evidence and remaining limitations.
- [x] Complete the milestone using local validation and recorded evidence. Per
  the user's updated instruction, disregard CI results and review feedback unless
  explicitly asked. Do not wait for CI or review approval, rerun remote jobs, or
  make changes in response to that feedback. Leave the PR for maintainer review;
  do not merge it autonomously.

Verification commands include:

```sh
cargo test --locked -p rt_fpga_reference --all-features
cargo fmt --all -- --check
python3 scripts/check_shoop_test_usage.py
RUSTFLAGS="-D warnings" cargo build --workspace
```

Also run the affected compiler/RTL target tests and the documented complete
native-suite command for the final gate. Report actual failures or unavailable
facilities explicitly; software tests do not establish physical MIDI or FPGA
timing results.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Disregard CI results and review feedback unless the user explicitly asks.
  Local milestone checks and transparent reporting of baseline failures determine
  completion; CI or automated review approval is not a completion gate.

## Execution evidence

All five stages are complete. The user explicitly removed CI and review follow-up
from the scope; no CI or review outcome is claimed here.

- Baseline reference/compiler suite: 74 passed. Final all-feature reference and
  compiler suite: **99 passed**, including **25 new MIDI tests**.
- Exact traces cover Q=1/16/32/64/128, callback partitioning, initial state,
  record/play/repeat/stop, synchronization waits, direct play-to-record changes,
  and acknowledged cleanup with one accepted output message per call.
- Heap guards, independent note/pedal models, storage/counter limits, refused
  note-offs, partial restoration, and fatal audio-fault recovery pass.
- Warning-denying workspace and reference builds, formatting, test-attribute
  policy, and feature-parity checks pass. Cargo's existing manifest-level
  unused-dependency notices are separate from Rust warnings.
- The broad native run completed: **3,456 passed, 13 failed, 11 skipped**. Twelve
  startup checks fail with native `Runtime(Capacity)` and one host check fails
  while polling shutdown after endpoint retirement. Both representative failures
  also reproduce on untouched base commit `9eb71d04`. Native production code was
  left unchanged; these are recorded baseline failures, not passing checks.
- Full native validation used `CARGO_PROFILE_DEV_DEBUG=0`,
  `CARGO_PROFILE_TEST_DEBUG=0`, and `CARGO_INCREMENTAL=0` after debug-symbol output
  exhausted workspace disk space. Assertions and test logic remained enabled;
  no timeout or test-selection change hid failures.
- Resource/CPU sample and validation limits:
  [milestone evidence](docs/fpga/midi-reference-milestone.md).
- Branch: `feat/fpga-reference-midi`.
- PR: https://github.com/SanderVocke/shoopdaloop/pull/1016.
