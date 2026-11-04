# P12 loop timing acceptance evidence

Status: local acceptance complete; P12 is OPEN pending PR CI. Implementation
branch: `feat/p12-loop-timing`, reconciled target
`a192b0ce77325bc8bb55b7b749e8d68b324f0a43`. PR target is
`feat/fixed-quantum-runtime`. The [plan](../../../PLAN_P12.md) defines completion.

## Reference behavior matrix

Reference master: `92079fb6f00308be0f6b407b7e8c02229662e420`. Preserved
implementations and fixtures live in `shoop_app_reference`, `shoop_backend`'s
legacy/native fixtures and the independent legacy `shoop_engine`.

| Operation | Required behavior | Reference anchor |
| --- | --- | --- |
| Scalar start/pre-play/length | Preserve optional fields; one common raw offset uses the reference channel's capture alignment | `ApplicationModel::set_loop_timeline` |
| Single/all-channel start | Validate channel identity; subtract each channel's alignment; timeline All shifts starts by one common delta | `set_loop_channel_timings`, `details_pane::single_and_all_edits_preserve_channel_offsets_and_preview_the_exact_command` |
| Channel batch refusal | One invalid or duplicate channel refuses all updates | `channel_timing_contract`, `selection_publishes_alignment_adjusted_immutable_media_details` |
| Stopped/playing length | Shared period; retain absolute position or take modulo when position reaches/exceeds the new period | `BasicLoop::set_length` |
| Recording/armed length | Refuse without stopping capture or changing other fields, including a later recording transition behind an earlier Stop | `fake_and_engine_backends_reject_length_edits_while_recording_is_armed`, `native_rejects_length_edits_while_recording_is_armed` |
| Recording/armed offset/pre-play | Independent setters remain available; preserve capture alignment and capture cursor | `set_loop_timing`, `set_loop_channel_timings` |
| Aligned take | Refuse windows outside retained raw storage when required by capture alignment | `validate_take_alignment_window` |
| Pre-play | Only an eligible stopped loop with a known imminent scheduled start; no early logical Playing/pulse/cycle | `channel_process_params`, `audio_preplay`, `midi_preplay` |
| MIDI pickup | Restore context before audible events; pickup notes continue across the scheduled start | `midi_corner_case_note_started_during_pre_play` |
| Invalid loop-end click | Do not change length for an end at/before the hovered channel's start | `end_is_loop_wide_and_invalid_end_clicks_do_not_collapse_the_loop` |
| Frame precision | Exact master comparisons at Q=1; existing fixed-quantum logical-period alignment at Q>1; raw media/timestamps remain frame-precise | `ARCHITECTURE.md`, `PLAN_FIXED_QUANTUM_CLEANUP.md` |

## Acceptance mapping

The nine criteria in [PLAN_P12.md](../../../PLAN_P12.md) are unchanged.
Implementation source is `1e2faf74` (reconciled target `a192b0ce`); the final
validation inventory records exact source and report hashes.

| Criterion | Production behavior and executable evidence |
| --- | --- |
| 1. Editor/intents | `shoop_egui::details_pane` tests single/all relative offsets, shared ends, snap/preview/zoom, invalid clicks and Q-effective previews. `shoop_app::timing_commands_are_atomic_while_recording_and_reconcile_once_after_ui_loss` exercises typed/encoded commands and service-owned identity/alignment. The browser canary executes scalar and audio/MIDI per-channel intents through the service and worklet. |
| 2. Admission | `channel_batch_refusal_and_armed_recording_length_refusal_do_not_partially_apply` checks execution-cut admission. `a_recording_behind_an_earlier_stop_blocks_length_and_capture_without_blocking_offsets` covers all armed transitions, retained capture clock and atomic combined refusal. Native JACK and socket probes edit live recording metadata and recover it after reattachment. |
| 3. Playback/sync | `manual_period_edits_preserve_absolute_position_and_refuse_misalignment_atomically`, `manual_timing_keeps_absolute_position_raw_media_and_independent_channel_windows` and all-Q engine timing tests cover absolute/modulo/zero periods, raw retention, sync pulse/countdown preservation and unrelated playback. |
| 4. Pre-play | `preplay_is_audible_while_stopped_and_pickup_notes_continue_across_start`, `cancelled_superseded_and_invalidated_pickups_release_notes_without_starting` and `retained_raw_lead_in_starts_inside_quantum_and_carries_midi_pickups` compare samples/packets, independent channel lead-in, inside-Q onset, cancellation and notes crossing the start. The browser checks quiet stopped output, audible stopped pickup and confirmed playback. |
| 5. Ownership/results | Service UI-loss test, RT `timing_edits` resync/revision tests and shared-candidate fence test cover bounded complete receipts, stale identities, exact-once reconciliation and retained old exports. `private_window_acknowledgment_keeps_replacement_cancellable` checks private preparation remains cancellable through cleanup. |
| 6. Raw media/persistence | `timing_windows_preserve_raw_audio_midi_proofs_and_preplay_through_replacement` checks independent windows, gains, shortening/zero/re-extension, shared raw buffers, save/load/save and clear. Native JACK, packaged file probes and browser compare actual PCM/exact MIDI and channel metadata after fresh load; corrupt loads preserve the session. |
| 7. Q/RT safety | Engine timing/output cases run Q=1/16/32/64/128. Shared raw-window sample oracle also runs host partitions 1/7/31/128. Render calls and ownership/refusal cleanup use heap guards. A 512 KiB callback-stack regression and compact checkpoint/reset oracle avoid full MIDI-state copies. Debug/release direct/wire/two-thread replay, no-std builds and selected Miri cases cover resource lifetime and callback boundaries. |
| 8. Deployment | Typed/encoded service tests, packaged socket/shared-memory probes, actual JACK output and hosted/isolated HTML acceptance cover the admitted paths. Only socket reattachment is claimed. Browser uses the production main-thread service, codec Worker and AudioWorklet. |
| 9. Closure | Full reference/production tests, retained legacy fixtures, capability/dependency audits and final PR checks are required below. Other parity capabilities retain their gates. |

The [validation inventory](loop-timing-validation.json) preserves source SHAs,
report/log hashes, package counts, Miri seeds and built browser artifact hashes.

## Precision and resource contract

Q is immutable and one of 1, 16, 32, 64 or 128 (default 32; browser 128).
Logical periods remain Q-aligned: a nonaligned API request refuses, and the
editor previews the effective aligned period. Exact-frame master comparisons
use Q=1. Raw media, channel offsets and MIDI timestamps retain frame precision.
This does not claim arbitrary-frame logical periods at Q>1.

Timing edits transfer fixed-size metadata, independently of media length. Raw
stored/shared media remain leased by settled/export cuts. Capture clocks and
alignment do not move. Per-channel out-of-media spans produce silence; aligned
capture windows retain their existing validation. No callback allocation, wait,
lock, second transport clock or media-sized edit upload is introduced.

The updated target imports only `shoop_profiling.now` in its raw worklet for
elapsed-time profiling. Disabled profiling makes no profiling-clock reads. That
measurement clock does not advance musical time. The built worklet contract
verifies the single allowed import, clock/toggle/capture behavior and protocol 9.
Both browser workflows also preserve the target's profiling behavior.

The reconciled target includes shared media, buffered processing, hot-path
optimizations and runtime profiling. Stored/shared audio
both support bounded window reads. Shared MIDI parameter-history validation uses
checkpoint flags and bounded prefix replay instead of full-state stack copies.
Prepared shared candidates fence the timing revision. P04/P05 remain OPEN at the
application boundary despite the target's internal shared-media foundation.

## Validation results

- Final combined native reference/production suite after target reconciliation
  and the upload-window refinement: **2,778 passed**, seven intentionally skipped.
  All adapter regressions also pass in the combined run.
- Formatting, warning-denying workspace builds, test attributes, all **428**
  tracing-inventory modules, parity coverage and runtime/loop/engine dependency
  checks pass. All **76** Python runner/report tests pass; updated inventory
  assertions pass after adding the compact shared-MIDI Miri case.
- Native loop-host debug and release direct/wire/two-thread replay: pass, with
  complete media/receipt/owner reports and SHA-256 hashes.
- Bare-metal builds: Cortex-M runtime/buffered/loops/ports plus no-default-feature
  engine/service/adapter on `armv7a-none-eabi` and `aarch64-unknown-none` pass.
- Pinned Miri `nightly-2026-05-04`, seed 0: four exact timing cases plus the compact
  shared MIDI checkpoint/reset case pass on `1e2faf74`. Ten indexed MIDI cases
  also passed at the earlier stack-safety checkpoint. These are selected-case
  claims, not whole-workspace Miri claims.
- Strict JACK test runs without `SHOOP_ALLOW_MISSING_BACKENDS`: exact 0.25 PCM and
  captured CC packets pass through local and independent hosts. Recording timing,
  playing length edits, raw retention, socket recovery and persistence pass.
- Packaged socket/shared-memory transport probes pass. Both large-file probes
  pass >32 MiB WAV/exact-MIDI, native input/output, corrupt-file refusal, fresh load,
  exact resave and graceful shutdown; the socket probe also abandons in-flight
  uploads/captures and reconciles service-owned work.
- Worklet and codec artifact contracts pass with RT protocol **9** (app protocol
  **22**). Hosted Chromium timing/pickup/save/load/save passes with **zero xruns**.
  The isolated single-file HTML also passes with **zero xruns**, including fresh
  owners, exact resave and restored playback. All **904 Wasm/Node executions across fifteen affected packages** pass.

Browser diagnostics found serial initial-state packets could consume the load
step's deadline under contention. Initial packets now share the negotiated bounded
8/64 upload-credit window, preserving order. The regression measures both limits
and compares all original initial-state packets after replacement. Private timing
headers do not count as committing a replacement. This retains cancellation and
atomic live publication while making the browser workflow reliable.

The organization browser policy blocks `file://` navigation. Self-contained
acceptance serves a directory containing only the embedded HTML over localhost,
as supported by the existing runner; hosted assets cannot satisfy those reads.

## Reproduction

Enter the project's configured build environment first. The non-Nix validation
host used Rust 1.99.1, nextest 0.9.116, Node 22.22.2, wasm-pack 0.15.0,
wasm-bindgen 0.2.127 and Chromium. Use the repository-pinned CI tooling on CI.

```sh
cargo fmt --all -- --check
RUSTFLAGS='-D warnings' cargo build --locked --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/check_feature_parity.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loops_boundary.py
python3 scripts/check_rt_engine_boundary.py
python3 scripts/run_loops_hosts.py --native-only --profile debug
python3 scripts/run_loops_hosts.py --native-only --profile release
python3 scripts/run_wasm_tests.py --runtime node --profile dev
```

Native packaged and browser commands are documented in
[the frontend README](../../../src/rust/shoopdaloop/README.md#verification).
For strict JACK use the isolated dummy-server runner, or an available server
without the missing-backend allowance. Miri's exact inventory is available from
`scripts/run_rt_runtime_miri.py --list`; the four first `shoop_loops` shards and
`rt_runtime` shard 12/34 select these five cases without reducing their workloads.

## PR and review

[PR #943](https://github.com/SanderVocke/shoopdaloop/pull/943) targets
`feat/fixed-quantum-runtime`. The workflow filters include that requested base.
Final-head CI and automated review are pending; the ledger remains OPEN until
closure. No automated review or review comments had been provided at this checkpoint.
