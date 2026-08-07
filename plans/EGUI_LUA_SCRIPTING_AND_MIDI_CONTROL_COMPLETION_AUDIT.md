# egui Lua scripting and MIDI-control completion audit

## Audit contract

Completion requires direct artifact evidence for every immutable acceptance criterion in `EGUI_LUA_SCRIPTING_AND_MIDI_CONTROL_PLAN.md`, honest agreement across every document in `plans/`, current passing output from every named final gate, and a clean committed tree. A checked plan item is not evidence by itself.

## Prompt-to-artifact acceptance checklist

| # | Immutable requirement | Implementation evidence | Verification evidence | Result |
|---|---|---|---|---|
| 1 | Native standalone `mlua` runtime; no Qt/frontend dependency; isolated lifecycle | `src/rust/shoop_scripting`; actor-local construction/destruction in `shoop_app`; target-gated runner dependencies | `runtime_is_constructed_and_used_on_its_actor_thread`; lifecycle/isolation tests; native/browser/scripting dependency and source scans | Covered |
| 2 | Complete retained `shoop_control` API, constants, shapes, selectors, conversion, validation, ordering, errors, and committed state | `control.rs`, shared application reducers, backend operations | `every_control_function_is_invoked_with_retained_shapes_and_selectors` proves all 61 names are actually called; `complete_control_surface_is_installed_with_legacy_constants` compares every generated key/modifier and all 17 semantic constants with no extras; validation, read-your-writes, reorder, Fake application, Engine representative, and retained 45-case QML tests pass | Covered |
| 3 | GUI, keyboard, Lua, and MIDI share authoritative state/backend policy; regular composition is serial/parallel | `ControlOperation` application reducer; GUI actions; script-composition coordinator over primitive backend transitions | expanded GUI action/Fake-backend tests; Lua batch test; keyboard/APC application workflows; deterministic serial wrap/state test; parallel EngineBackend execution and session round-trip | Covered |
| 4 | Production keyboard and APC scripts run from shared sources with documented defect exceptions only | `include_str!` of `src/lua/builtins/keyboard.lua` and `akai_apc_mini_mk1.lua`; no egui copies | embedded-source syntax tests; complete keyboard workflow; 8×8 APC workflow including separate serial and parallel composition paths; native virtual-MIDI smoke follows unavailable-device policy | Covered |
| 5 | Native MIDI inputs/outputs, all-match anchored autoconnect, hotplug, exact bytes, bounded FIFO, real positive maximum, diagnostics, cleanup | fake/native `MidiControlService`; non-bursting limiter; per-rule API diagnostics with direction/pattern/matched/connected endpoints/latest error | exact-byte/multi-match/hotplug/retry/cleanup/overflow tests; two-output fake-clock broadcast test proves 99/100 ms boundaries and no catch-up burst; UI rendering test; APC reconnect/rate assertions | Covered |
| 6 | Typed committed loop/global/key callbacks and monotonic one-shot timers; deterministic non-reentrant bounded dispatch and ownership | callback queues, cloned dispatch lists, timer registrations, application snapshot diff | all five loop event kinds and every payload field; both key kinds across tests; global payload; nested registration deferral; duplicate committed-state suppression; timer due order, zero-timer deferral, 256 cap, stop cancellation, callback operations, and cross-script failure tests | Covered |
| 7 | Lifecycle/settings integration; bundled keyboard first-run; user add/reload/remove; atomic preservation; missing/malformed diagnostics | typed `script_settings.1`; embedded resolver; stable startup-ID handshake; composition-root path map | settings preservation/default/malformed tests; invalid-before-valid and duplicate-name startup ID test; rejected-slot path-association test; lifecycle/reload/resource cleanup tests | Covered |
| 8 | Transactional source-bearing session scripts; machine/session separation; browser rejection | session staging/validation/commit and encoder adapters | exact source/ID/enabled round-trip; pre-commit syntax rejection; cancellation rollback; machine preservation; unsupported browser capability checks | Covered |
| 9 | Realtime safety, bounded work, script-local failure isolation, trusted-local-extension model | scripts run on control actor; engine contracts unchanged; queue/message/callback/log caps | 24 no-allocation tests, lock guards, bounded callback/MIDI tests, slow/failing script isolation behavior, documented trust model | Covered |
| 10 | Native egui manager and key safety; backend-free GUI; no Qt dependency | `app_widget.rs`, `key_input.rs`, plain `shoop_app_api` snapshots/intents | lifecycle/error/help/log/per-rule MIDI presentation; min/common viewport; stable IDs; text-entry/repeat/focus-loss tests; dependency scans | Covered |
| 11 | Browser remains buildable/runnable but scripting/Web MIDI stay explicitly unsupported and absent | wasm target gates and browser capability messaging/rejection | warning-denying wasm UI/preview checks; release Trunk/worklet build; hosted/direct-file Chrome output-only workflows; packages verify; browser/worklet trees exclude `mlua`, `midir`, frontend, and Qt | Covered |
| 12 | Native/QML/browser/realtime regression suite and docs/project ledgers pass | implementation and all listed docs | workspace all-target build/tests; retained 45/45 QML session-control test; focused crates; package/browser workflows; scans; consistency review below | Covered |

## Retained QML compatibility-table mapping

The retained `tst_LuaEngine_SessionControlHandler.qml` table contains 45 cases. Its current run reports **45 passed, 0 failed, 0 skipped**. The frontend-independent replacement evidence maps as follows:

| Retained case group | Framework-independent/native evidence |
|---|---|
| Loop count, all, selected, targeted, mode, next mode/delay, length, by mode/track | complete-function invocation/shape assertions, read-your-writes test, reorder test |
| Track/loop reorder coordinate updates | `control_queries_follow_track_and_loop_coordinate_reordering` |
| Loop gain/fader/balance | complete invocation/conversion assertions, bridge operations, Fake application/APC paths |
| Transition, trigger, record-N, targeted record, grab/adopt, repeat sync | complete operation table; invalid/sentinel validation; application Fake operations; Engine timing/realtime suite |
| Composition append | serial deterministic Fake coordinator, parallel EngineBackend workflow, APC serial/parallel source workflow, session round-trip |
| Selection, target/toggles, clear/all | complete table, read-your-writes, GUI and bundled application workflows |
| Track output/input gain/fader/balance/mutes | complete selector/shape table and APC authoritative state/backend assertions |
| Apply-N, solo, sync, play-after-record, default action | synchronous read-your-writes, complete table, GUI/keyboard/APC workflows |
| Loop/global callbacks | all typed payload fields/kinds, committed application dispatch, duplicate/non-reentrant and failure-isolation tests |
| Key constants and keyboard callback | exhaustive generated constants, semantic constants, both event types, modifier translation and production keyboard workflow |
| One-shot timer | due-order/non-reentrancy/cap/cancellation test and application callback-operation test |

The native compatibility surface also covers retained API additions not represented by those 45 old cases: default-recording action, MIDI autoconnect input/output, granular MIDI diagnostics, and strict rate pacing.

## Stage deliverable review

| Stage | Evidence conclusion |
|---|---|
| 0 contract | Compatibility contract documents API, lifecycle, events, MIDI, settings/session, trust, browser, and approved shared APC defect fix. |
| 1 runtime | `shoop_scripting` is frontend-independent, actor-owned, isolated, embedded, and syntax/error tested. |
| 2 lifecycle | Stable IDs and all transitions/actions/errors/cleanup are observable and tested. |
| 3 API/backend | Complete function invocation and constants, retained 45/45 QML suite, Fake application paths, representative Engine paths, serial/parallel composition, realtime guards. |
| 4 callbacks/keyboard | Complete event payload/kind and timer edge evidence plus key safety and production keyboard workflow. |
| 5 MIDI | Fake contract is exhaustive for deterministic behavior; native virtual smoke supplements it; strict non-bursting rate and per-rule diagnostics are present. |
| 6 settings/session | Preservation-aware machine settings, exact startup mapping, transactional session scripts, browser rejection, and cancellation semantics pass. |
| 7 presentation | Native manager/actions/help/logs/activity/granular MIDI/error states and browser-unavailable state render at supported sizes. |
| 8 bundled workflows | Embedded keyboard and APC workflows exercise authoritative application/backend state, bytes, rate, serial/parallel composition, reconnect, and cleanup. |
| 9 final validation | Command ledger below is current after the final implementation changes. |

## Documentation and project-ledger consistency

| Artifact | Audited state |
|---|---|
| `EGUI_LUA_SCRIPTING_AND_MIDI_CONTROL_PLAN.md` | Complete; every item checked only after this audit closed its evidence gap. |
| `EGUI_FEATURE_PARITY_MATRIX.md` | All Lua milestone rows use concrete final evidence; no stale “Partial” audit findings remain. |
| `EGUI_REPLACEMENT_PROJECT.md` | Milestone status and roadmap agree that native parity is complete and generic MIDI-rule editing remains separate. |
| `docs/egui_lua_compatibility_contract.md` | Exact API/environment/lifecycle/event/MIDI/settings/session/browser contract and approved defect fixes. |
| `docs/source/developers.scripting.rst` | Runtime, manager, trust, settings/session, matching, pacing, granular diagnostics, cleanup, browser limitation. |
| `docs/source/usage.keyboard.rst` | Native manager/key routing and focus/release behavior. |
| `docs/source/usage.midicontrol.rst` | Scripted native MIDI directions, anchored matching, non-bursting rate behavior, per-rule diagnostics, and generic-editor boundary. |
| `src/rust/shoopdaloop_egui/README.md` | Embedded native scripting resources and browser limitation. |

## Final command and artifact ledger

All commands were run from the repository root unless noted.

| Gate | Current result |
|---|---|
| `cargo fmt --all --check`; `git diff --check` | Pass |
| `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend` | Pass; external linker deprecation diagnostic is not a Rust warning lint |
| Focused `shoop_scripting`, `shoop_app_api`, `shoop_app`, `shoop_backend`, `shoop_engine`, `shoop_settings`, `shoop_egui`, runner tests | Pass; one timing-sensitive pre-final engine test failed once and passed its exact retry; the final workspace run passed it |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 RUSTFLAGS="-D warnings" cargo test --workspace --features shoop_engine/app_backend` | Pass, including 24 no-allocation tests and all final scripting/application tests |
| Native virtual MIDI | Deterministic fake tests pass; host test emits the documented ALSA `/dev/snd/seq` unavailable diagnostic and exits under policy when no sequencer exists |
| `cargo check --locked -p shoopdaloop_egui --target wasm32-unknown-unknown` and preview equivalent with warnings denied | Pass |
| Release Trunk UI + release worklet build using repository environment's Trunk 0.21.14 and lld | Pass |
| Web package generation/verification | Hosted zip and self-contained HTML pass verifier |
| Chrome workflows | Hosted output-only and direct-file self-contained output-only pass at 900×600 |
| Native package generation/verification | Linux x86_64 debug archive passes verifier |
| Retained QML | `tst_LuaEngine_SessionControlHandler.qml`: 45/45 pass after initializing the tracked icon submodule; prior `tst_LuaEngine.qml` evidence also passes |
| Dependency/source boundaries | `shoop_scripting`/`shoop_egui` exclude frontend/Qt; browser/worklet exclude Lua/native MIDI/frontend/Qt; worklet Wasm has zero imports |
| Git tree | Final diff/format checks pass; clean tree verified after the final audit commit |

## Conclusion

Every original acceptance criterion and every plan deliverable now has concrete implementation and current verification evidence. The audit found and closed five real gaps: complete retained-surface proof, strict MIDI pacing and granular diagnostics, invalid-startup path mapping, callback/timer edge coverage, and serial composition execution/state. No requirement remains open.
