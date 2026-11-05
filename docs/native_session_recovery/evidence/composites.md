# Composite core restoration evidence

P03 is **OPEN**. The named partial restoration is the direct-track composite core:
regular and explicit Play/Stop/Record arrangements, nested authoring and control,
live edits, automatic monitoring, frozen file timing and accepted-work lifetime.
Replacing and dry/wet event modes/topologies depend on P05/P06/P07. Those modes
refuse atomically, including whole-document load; their tags are not advertised
as executable support. The plan's immutable full-parity gate still applies.

## Contract and acceptance mapping

The [execution contract](../composites.md) and
[plan](../../../PLAN_P03_COMPOSITES.md) define the requirements. Test paths below
are production-stack evidence unless explicitly identified as loop-library tests.

| Criterion | Implementation and executable evidence |
| --- | --- |
| 1. Frame clock | `shoop_loops/tests/frame_composites.rs::source_stop_resize_and_restart_preserve_every_prepared_frame`; guarded `shoop_rt_engine/tests/composites.rs::prepared_frame_programs_keep_their_media_under_every_quantum_and_partial_callback`. |
| 2. Frozen preparation | Service `source_resize_does_not_retime_an_active_document_and_repeat_disable_retains_its_source`, `natural_child_duration_refresh_is_versioned_and_keeps_the_existing_basis`; frozen-geometry file round trips. |
| 3. Launch/repeat | Loop `primitives_and_composites_wait_only_at_their_own_end_and_resume_on_source_edges`, `stopping_a_source_releases_an_existing_wait_without_an_extra_pass_or_tick`, `delayed_commands_still_follow_source_edges_without_owning_program_progression`; engine repeat-wait/media cases. |
| 4. Deterministic execution | Loop `program_boundaries`, `program_changes`, independent dense oracle and interloop media tests; engine `nested_starts_and_iteration_seeks_apply_child_snapshots_in_the_same_quantum`. |
| 5. UI/Lua | Service `public_lua_composes_nested_plans_in_one_turn_and_editor_intents_keep_confirmed_identity`, stable-ID group editor tests; retained actual `shoop_egui` composite widget tests; packaged companion canary. |
| 6. Independent children | Service `child_commands_do_not_move_parent_frames_and_new_authored_actions_still_apply`; nested Clear/removal cases preserve child media and report stopped ancestors. |
| 7. Live edits | Loop future-only/protected-next-unit and owning program/graph tests; adapter cancellation/supersession; service `cancellation_racing_an_activation_reconciles_the_version_that_actually_installed`. |
| 8. Auto-arm | Guarded engine monitoring tests inspect audio/MIDI at every supported quantum. Service `automatic_monitoring_and_recording_finish_without_ui_or_service_polling`, manual-policy refusal/reconciliation. |
| 9. Authoritative projection | Adapter exact activation receipts and resync tests; service `composite_documents_geometry_and_pending_versions_publish_only_after_render_confirmation`; client frozen details and pending/cancel display. |
| 10. Persistence | Service `nested_composite_capture_and_load_preserve_frozen_geometry_media_and_event_ids`, active-plan capture during pending edit, future-recording refusal; session/codec legacy migration and independently rounded frozen endpoints; atomic backend replacement and first-quantum monitoring. |
| 11. Recovery/lifetime | Service UI-loss load, owned lifecycle, New Session/shutdown; adapter generation reuse and active/pending resync; socket companion client-absent recording and fresh-VM recovery. |
| 12. Bounded portability | Guarded engine/backend at-limit 8192-span staging with a small reliable window; allocation guards, four-target dependency audits, native/Node/Chrome suites and maintained worklet/codec artifact contracts. Final public/browser platform gates are recorded separately below. |

Service test names refer to `shoop_app/tests/support/composite_cases.rs`, included
by the maintained `service` test target. Reference app/engine fixtures retain old
editor and media expectations. Their tick-driven scheduling is historical, not
evidence that the new contract uses source ticks or a client fallback.

## Source checkpoints and validation

- `730148bb`: automatic monitoring; native **2,824 passed / 7 skipped**. Node and
  Chrome app/client/protocol/editor/adapter/engine/worklet suites passed.
- `bf9cef95`: corrected the maintained JavaScript artifact's stale engine protocol
  pin; actual rebuilt worklet and codec contracts passed.
- `b40ec30b`: persistence, app protocol **23**, engine protocol **11**, file
  document **14**, host setup **7**, worklet configuration **4**. Native warning-
  denying full-feature build and **2,835 passed / 7 skipped**. Node and Chrome:
  app **58**, client **11**, app protocol **17**, legacy backend **87**, editor
  **234**, adapter **52**, engine **93**, worklet **17**, engine protocol **6**,
  session **54**, codec **21**. Boundaries, test policy, parity/tracing and actual
  worklet/codec artifact contracts passed. Its complete GitHub platform matrix,
  native JACK acceptance, coverage, docs and CodeQL checks passed. The debug-web
  job subsequently passed too; final-head validation must rerun the matrix.
- Public-core milestone after `b40ec30b`: capability and public raw creation,
  compose-then-Play admission and same-turn nested Lua preparation. Focused backend
  **54 passed**, app/client/protocol **87 passed**. Actual packaged socket and
  shared-memory first-attachment canaries passed, including exact **4,096-frame**
  recording, uninterrupted nested playback and exact save/load/save media/geometry.
  Socket recovery reconnects without reconstructing its client-local Lua VM.
  Final core validation: full native **2,838 passed / 7 skipped**; Node and Chrome
  each passed app **59**, client **11**, app protocol **17**, legacy backend **87**,
  editor **234**, loops **191**, codec **21**, adapter **53**, engine **94**, worklet
  host **17**, engine protocol **6**, session **54**. The final warning-denying
  build and socket/shared-memory canaries passed, including real piano MIDI data.
  Hosted and direct-file actual AudioWorklet composite canaries passed with zero
  xruns, exact audible recording through the main-thread stall, and exact file
  round trips. Under concurrent full builds, two debug browser attempts hit the
  existing core-load deadline; isolated final runs passed. The composite submachine
  has its own bounded per-step deadlines, rather than sharing one 30-second timer.
- A completed explicit nested child now reenters on a new parent pass, including
  coincident endings, while unfinished children retain their phase. Guarded tests
  inspect actual captured/output PCM at every supported quantum. The independent
  dense oracle explicitly models those entries; all **60** standalone host tests,
  **24** runner-validation tests and the maintained debug native/Node/offline/live
  AudioWorklet replay passed. Prior compressed reference fixtures are retained;
  the new nested-entry fixtures are separately named.
- Pinned Miri (`nightly-2026-05-04`, seed **0**) passed **25** selected executions:
  owning program changes/capture preflight **12**, media ownership **7**, boundary
  preview **3**, bounded client projection **2**, engine slot reuse **1**. These
  runs exercised the public-core working tree based on `b40ec30b`; their report
  HEAD field alone does not identify the uncommitted patch. Four-target runtime,
  loop and engine audits, Wasm isolation, test policy, parity/tracing (**441
  modules**), formatting and actual worklet/codec artifact contracts passed.

Full P03 is still gated on dependent modes and the final integrated-head platform
matrix/review. No narrower closure has been approved.

## Reference surface and dependency integration

| Retained surface | Production route and disposition |
| --- | --- |
| Convert an empty primitive or create a standalone composite | Service conversion and raw backend creation use generation fences and the same prepared plan receiver; initial Play waits for its accepted plan. |
| Serial/parallel append and nesting | Unchanged Lua `loop_compose_add_to_end` and UI intents enter service documents; parent preparation can use a child's accepted pending duration. |
| Positioned insertion, group move/copy/delete, natural/forced duration | Editor widget gestures emit stable event IDs; service edits retain relative positions and frozen geometry. Natural child changes create new versions. |
| Kind/event modes, Play/Stop/iteration seek and child control | Service prevalidates complete target sets. Frame snapshots and arbitration execute in the resolver; the outer Play/Stop restriction remains. |
| Live edits, cancellation and structural lifecycle | Service-owned candidate/results and adapter owning swaps preserve working plans; Clear/removal/New Session/shutdown reclaim references and report stopped ancestors. |
| Auto-arm, files and accepted-work recovery | Render demand, confirmed captures, frozen timing, fresh-ID replacement and socket authority recovery replace client-local execution. VMs remain client-local. |
| Source tick/elapsed fallback behavior | Deliberately replaced by the agreed frame timeline and shared launch/repeat policy, with completed nested one-pass reentry and unfinished nested phase preservation. |
| Replacing/dry-wet event modes and corresponding tracks/processors | Still unavailable; full parity cannot close. Atomic edit/mixed-control refusal and whole-file diagnostics cover each mode until actual execution support is integrated. |

Readiness checked after `dc313d59`, against fixed-runtime target `a192b0ce`:

- [P04/P05 PR #940](https://github.com/SanderVocke/shoopdaloop/pull/940), branch
  `feat/finish-grab-replace` at `0d2f525f`, explicitly remains unfinished. Its
  current work covers Grab policy, storage sizing/ownership and indexing proofs;
  its status still says normal application Replacing is unavailable. Its loop
  mode representation has not supplied an executable replacing mode for P03.
- No P06/P07 production implementation branch was published among the inspected
  remote heads/open PRs. The current adapter accepts direct tracks and refuses
  dry/wet track construction and track FX. `DryWetModes`, `DryWetTracks` and
  `TrackFx` cannot be inferred from composite capability publication.
- Once supplied, integrate real mode semantics through the loop/compiler and RT
  representations, adapter conversion, service control/event admission, monitoring
  and persistence requirements. Preserve the distinct recorded/playback roles and
  original native/browser processor scope. Do not translate the mode tags to core
  playback/recording or enable capabilities from encoding alone.

`unavailable_dependent_modes_refuse_edits_and_mixed_transport_while_the_active_plan_keeps_playing`
checks typed and encoded commands for Replacing, PlayingDryThroughWet and
RecordingDryIntoWet. Codec legacy-document checks require the corresponding P05
or P06 diagnostic for all three modes. These are safety gates, not evidence of
restored execution.

The first core CI matrix found a macOS canary assertion that incorrectly froze an
end wait across attachment. Its follow-up permits the source clock to continue,
requires unchanged plan identity and nondecreasing completed passes, then observes
a repeat wait before testing source-stop release. The socket canary now also saves,
loads fresh stopped IDs and resaves after live source growth and recovered editing.
The prior full matrix is not represented as green at `dc313d59`.
Follow-up local verification passed the warning-denying full-feature build and
native **2,839 passed / 7 skipped**, app **60** / codec **21** in Node and Chrome,
and the expanded actual socket workflow. Production runtime code is unchanged
by these canary/dependency-audit follow-ups. Latest-head platform status is tracked
on [PR #941](https://github.com/SanderVocke/shoopdaloop/pull/941); earlier failing
or cancelled runs are not substituted for that head's results.
The staged-edit probe changes the current target set and waits for
`AwaitingRestart` before disconnecting, so safe early adoption cannot satisfy that
recovery case accidentally. Its complete socket/file workflow and warning-denying
build passed after this final probe refinement.
The next macOS debug run (`97f5c210`) reached a generic state-wait timeout after
the first file round trip. Recovery waits now identify their stage and report
bounded mode/length/version summaries. Reattachment also waits for management
status to report the old controller released: closing its socket alone does not
guarantee that the service has processed detach.
The warning-denying workspace build passed with these probe changes. Ten complete
socket workflows passed with `SHOOP_CI_TRACING_ENGINE_DETAIL=1`, including five
pinned to one CPU (`taskset -c 0`); the shared-memory first-attachment workflow
passed too. This Linux repetition did not reproduce the macOS timeout and is not
claimed as its platform acceptance.

Target `eb4287a5` subsequently added profiling graph PR #939. Its integration
retains composite admission/snapshots/activation receipts alongside profiling
inspection and graph projection. Combined versions are app protocol **25** and
engine protocol **12**, with file **14**, native setup **7** and worklet config
**4** unchanged. The warning-denying workspace build and **14** focused adapter
cases passed, including playing composite/profiling state across conversion and
resync. Full native regression passed **2,864 tests / 7 skipped**, along with
four-target boundaries and Wasm dependency isolation. Native and Wasm builds
first exhausted disk space; obsolete caches were cleared and the native rerun
passed. The seven unexecuted Node packages passed on rerun, completing all twelve:
app **61**, client **12**, app protocol **17**, legacy backend **87**, editor **244**,
loops **191**, codec **21**, adapter **55**, engine **95**, worklet **17**, engine
protocol **6**, session **54**. Actual socket/shared-memory composite workflows
passed. At integration commit `c98ee1e1`, pinned Miri seed 0 passed three boundary
and two client-projection cases. The prior
`280c12a9` matrix passed all six native profiles and web release; its remaining
web-debug job is separate from this newer integration.

The replacing branch advanced to `b6ac9443` at this recheck, but PR #940 still
explicitly reports application Replacing unavailable. Pooled audio and persistent
MIDI-index work does not satisfy the dependent-mode gate. A later recheck found
PR #940 rebased to `2707c466`, with application Replacing still unavailable.
New [PR #946](https://github.com/SanderVocke/shoopdaloop/pull/946) at `ac70645e`
provides concrete loop/RT dry-wet modes and processor foundations, while public
admission and service/session-file persistence remain gated. It is now the
integration candidate; full P03 remains OPEN.

The user revised P03's testing strategy on 2026-11-05: small targeted local checks,
with broad regression and cross-platform/browser/artifact validation supplied by
CI. The plan records this split and risk-based Miri selection; the completed broad
local runs above are historical evidence, not mandatory repeats before each push.
Further merged-head Chrome/browser/artifact results await CI.

An integration review exposed a resync defect in the existing core: per-loop span
indexes 10/11 entered snapshot-stage branches, preventing completion after the
seventh span. The new production adapter regression
`resync_preserves_a_playing_plan_with_more_than_seven_sparse_spans` reproduces the
hang with 20 spans; its pre-fix run was interrupted after exceeding 120 seconds.
Stage transitions now remain in the outer snapshot cursor. The affected packages
built with warnings denied and all four focused composite resync cases passed,
including pending versions, automatic monitoring and profiling coexistence.

PR #941 was retargeted to `feat/backend-replacement` at the user's request. The
`e1f28c87` reconciliation retains the verified shared fixed-runtime/profiling
baseline while combining P03 with P12 timing and the incoming platform work.
Combined versions are app **27** and engine **13**; file **14**, native setup **7**
and worklet config **4** remain unchanged. Timing/composite requests, outcomes and
resync records have distinct wire tags. One coherent publication includes timing
revision and composite cursor state. Primitive timing requests refuse virtual
composite clocks; kind conversions clear stale timing metadata. Natural child
dependencies include effective length so P12 length edits refresh natural events
without changing their accepted time basis. The warning-denying application/host
build and **17** selected protocol, resync, service and persistence cases passed,
including both new cross-feature regressions. Final-head CI is tracked separately;
no cross-platform local rerun is implied.

## Reproduction and limits

Use the repository build/test setup and deny Rust warnings. Native full-feature
checks select `shoop_engine/app_backend,shoop_backend/native-fx`; the former is
only a retained-reference test feature. The production frontend uses the RT stack.
Local preparation used Rust **1.99.1**, nextest **0.9.116**, Node **22.22.2** and
Chrome/ChromeDriver **147.0.7727.137**, compact debug profiles and no incremental
compilation. Node/Chrome package suites use `scripts/run_wasm_tests.py --profile dev`.

Run the companion commands and browser `COMPOSITES=1` commands in the
[frontend guide](../../../src/rust/shoopdaloop/README.md). Socket and shared-memory
canaries use **Q32**, 20 logical loop slots and a 16-MiB media budget. The browser
canary uses actual AudioWorklet capture and codec-worker/download paths, with a
1.8-second blocked main thread spanning an entire authored recording occurrence.
MessagePort simulation or model-only success does not replace that acceptance.

Default prepared program budgets are **256 entries, 1024 segments and 512
relations**, with relation capacity at least the loop-slot count. The adapter
streams 32 span records at a time. At-limit tests explicitly raise the segment
budget to **8192**; they do not claim every host's default pool accepts that plan.
All supported quanta **1, 16, 32, 64, 128** have guarded render coverage.

There is no host-crash durability, browser-page crash recovery, simultaneous
controlling-client support, shared-memory reattachment, tempo following or DSP
time stretching. No merge or reviewer approval is implied by these results.
