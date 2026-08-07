# egui Lua scripting and MIDI-control completion audit

## Objective and success criteria

The objective is complete only when:

1. Every immutable acceptance criterion in `EGUI_LUA_SCRIPTING_AND_MIDI_CONTROL_PLAN.md` has direct implementation and verification evidence.
2. Every checked implementation/verification item in that plan is supported by evidence that covers the full wording, not merely a proxy test.
3. Every Lua milestone row in `EGUI_FEATURE_PARITY_MATRIX.md` reports the evidence actually present in the repository.
4. `EGUI_REPLACEMENT_PROJECT.md`, the milestone plan, and related user/developer/runner documentation agree on status and boundaries.
5. Every named final command/gate has current passing evidence, with unavailable-device skips following the documented policy.
6. The working tree is clean after the audited changes are committed.

## Prompt-to-artifact checklist

| Requirement | Artifact/evidence inspected | Audit status | Remaining evidence or work |
|---|---|---|---|
| Native standalone `mlua` runtime without Qt/frontend dependency | `src/rust/shoop_scripting`, actor construction in `shoop_app`, native/browser dependency-tree scans, warning-denying builds | Covered | Re-run boundary scans at final audit. |
| Every retained function/constant, validation rule, return shape/order, selector, conversion, and error behavior | 61 names in `CONTROL_FUNCTION_NAMES`; 45 retained cases in `src/qml/test/tst_LuaEngine_SessionControlHandler.qml`; current `complete_control_surface_is_installed_with_legacy_constants` only checks function presence and three constants; current read/write test is representative | **Not covered** | Port the complete retained table to framework-independent tests and add invalid-argument/error cases plus exact generated-constant comparison. Run against control bridge and authoritative Fake/Engine application paths where state/backend behavior matters. |
| One authoritative GUI/Lua/MIDI control path | `ControlOperation`, application operation handling, GUI/application tests, bundled workflows | Partially covered | Add a systematic GUI-vs-Lua equivalence table, including stale selectors/failures. Verify regular composition timing instead of relying only on stored sections and parallel start. |
| Production keyboard and APC workflows | Production embedded-source application tests in `shoop_app`; shared APC N-cycle defect documented | Largely covered | Preserve evidence that the shared APC fix is frontend-neutral; add any cases discovered by the full compatibility port. |
| MIDI input/output, autoconnect, hotplug, exact bytes, real rate maximum, and actionable failures | Fake/native MIDI services and tests; current diagnostics expose only aggregate counts; current limiter can emit multiple queued messages in one pump after elapsed-time accumulation | **Not fully covered** | Make positive-rate output non-bursting, add exact fake-clock boundary tests, and publish per-rule direction/pattern/matched endpoint/connection/error state to app/UI. |
| Event/timer payloads, ordering, non-reentrancy, and teardown | Callback/timer implementation and representative test | **Weakly covered** | Assert every loop/key payload field, every event kind, registration-during-dispatch non-reentrancy, duplicate suppression, multi-timer order/cap/cancellation, and teardown. |
| Lifecycle/settings and first-run bundled scripts | `shoop_settings`, startup adapters, lifecycle/UI tests | Partially covered | Fix startup ID/path association: current runner zips accepted script states with all configured paths, so an invalid earlier startup script can associate a later ID with the wrong path. Test invalid/duplicate-name startup ordering and persistence. |
| Transactional session scripts | Syntax staging, commit replacement, round-trip/cancellation tests | Covered | Re-run focused and browser capability-rejection tests after subsequent changes. |
| Realtime/boundedness/error isolation | Engine no-allocation/lock tests, bounded queues/callback caps, cross-script tests | Covered for current design | Re-run realtime and workspace gates after changes. |
| Presentation/key safety and backend-free GUI | `shoop_egui` script manager/key translator tests and dependency scans | Covered for current fields | Extend UI tests for new per-rule MIDI diagnostics. |
| Browser preservation | wasm UI/worklet builds, hosted/self-contained Chrome output-only workflows, dependency scans | Covered | Re-run after API changes. |
| Regression/docs/QML | Workspace build/tests, `tst_LuaEngine.qml` pass, docs updated | Partially covered | The retained session-control QML test was attempted under offscreen/Xvfb and hung before testcase registration; either obtain passing relevant retained evidence or document a defensible narrower retained gate. Re-run final docs consistency scan. |

## Named files and deliverables

| Named artifact | Expected state | Current audit |
|---|---|---|
| `plans/EGUI_LUA_SCRIPTING_AND_MIDI_CONTROL_PLAN.md` | No checked item without complete evidence | Must be returned to **In progress** until the uncovered items above are closed. |
| `plans/EGUI_FEATURE_PARITY_MATRIX.md` | Lua rows match actual evidence | API/event/MIDI diagnostics/settings-startup rows currently overstate completion. |
| `plans/EGUI_REPLACEMENT_PROJECT.md` | Coarse status agrees with milestone audit | Must say in progress while audit findings remain. |
| `docs/egui_lua_compatibility_contract.md` | Exact retained contract and approved defect notes | Present; update if the full port discovers mismatches. |
| `docs/source/developers.scripting.rst` | Runtime/lifecycle/trust/settings/session/browser docs | Present. |
| `docs/source/usage.keyboard.rst` | Native manager and keyboard safety | Present. |
| `docs/source/usage.midicontrol.rst` | Native scripted MIDI matching/rate/diagnostics and generic-editor boundary | Present, but diagnostics wording must match the per-rule implementation. |
| `src/rust/shoopdaloop_egui/README.md` | Native scripting/resources and browser limitation | Present. |

## Final command and gate ledger

| Gate | Last observed evidence | Must re-run at completion |
|---|---|---|
| `cargo fmt --all --check` and `git diff --check` | Passed before prior completion claim | Yes |
| `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend` | Passed | Yes |
| Focused scripting/API/app/backend/engine/settings/GUI/runner tests | Passed | Yes |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 RUSTFLAGS="-D warnings" cargo test --workspace --features shoop_engine/app_backend` | Passed on final retry; unavailable virtual MIDI followed policy | Yes |
| wasm UI and release worklet builds | Passed with repository-available Trunk/lld | Yes |
| Hosted and self-contained browser workflows | Chrome output-only hosted and direct-file workflows passed | Yes |
| Native package/resource and fake-controller workflow | Native archive produced; embedded keyboard/APC workflows passed | Yes |
| Retained QML | `tst_LuaEngine.qml` passed; session-control-handler test hung before registration | Resolve or narrow with explicit evidence |
| Dependency/source scans | Passed after excluding the intentional generated Qt key-ABI comment | Yes |
| Clean committed tree | Was clean at prior claim | Yes |

## Audit conclusion

The prior completion status relied on proxy evidence for the full retained API table, granular MIDI diagnostics/rate spacing, event/timer edge cases, and startup path/ID persistence. The objective is therefore **not yet achieved**. Continue implementation and keep the milestone and project ledgers in progress until every row above is covered.
