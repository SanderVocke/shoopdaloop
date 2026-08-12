# Implementation plan

## Goals and scope

Add an **auto-mute other track inputs** global policy that:

- Appears as an on/off control in the top global-controls bar.
- When enabled, causes input monitoring to be exclusive when an unmute operation respects the policy.
- Is exposed through Lua getters/setters.
- Can explicitly be respected or bypassed by Lua input-mute setters and helpers.
- Is always respected by the bundled keyboard and Akai APC Mini controls.
- Persists with sessions and defaults off for existing behavior and old sessions.

Update the keyboard script to use **I** to toggle input mute for tracks containing selected loops. Retain the Akai script’s existing **PAN + grid button** binding, making it explicitly respect the new policy.

## Immutable acceptance criteria

1. The top bar has a visible toggle for the policy, with a tooltip explaining that enabling track input monitoring mutes other track inputs.
2. The policy defaults to **off** and enabling it does not retroactively alter current monitoring states.
3. A GUI track-input unmute always respects the policy:
   - Policy off: other tracks remain unchanged.
   - Policy on: all non-target tracks, including the sync track, become input-muted.
   - Muting a track never changes other tracks.
4. A multi-track unmute operation treats its selector as one target group: selected tracks become unmuted and tracks outside the selector become muted.
5. Lua exposes:
   - `get_auto_mute_other_track_inputs()`
   - `set_auto_mute_other_track_inputs(active)`
   - `track_set_input_muted(selector, muted, respect_auto_mute)`
   - `shoop_helpers.track_toggle_input_muted(selector, respect_auto_mute)`
6. Lua calls with `respect_auto_mute = false` never alter tracks outside their selector. Calls with it set to `true` apply the policy only when unmuting and the global policy is enabled.
7. Legacy two-argument `track_set_input_muted(selector, muted)` and one-argument helper calls remain valid and behave as `respect_auto_mute = false`.
8. The Lua API minor version advances from `1.0` to `1.1`; bundled scripts announce `1.1`.
9. The keyboard **I** binding:
   - Collects the unique tracks containing selected loops.
   - Does nothing if no loops are selected.
   - Toggles the selected track group as a unit: if all are unmuted, mute them; otherwise unmute them.
   - Always passes `respect_auto_mute = true`.
10. Akai **PAN + grid button** toggles input mute for the track represented by that grid column, including the sync-track button, and always respects the policy.
11. The new global value round-trips through sessions; sessions missing the field load it as `false`.
12. Core/application and Lua-interface tests cover the policy. No new interactive GUI tests or tests executing `keyboard.lua` or `akai_apc_mini_mk1.lua` are added or run.

## Design rules and constraints

- Keep policy enforcement in the application model, not in egui or Lua, so every respecting caller receives identical behavior.
- Carry the caller’s `respect_auto_mute` decision through the intent/control-operation boundary.
- Apply a multi-track request as one application operation rather than repeatedly applying singular exclusivity.
- Preserve synchronous Lua shadow-state behavior: getters later in the same callback must observe the resulting target and non-target mute states.
- Publish global-control callbacks when the policy changes.
- Preserve old Lua call forms with non-respecting behavior.
- Do not change engine-level recording semantics: input mute continues to disable monitoring without discarding recording input.
- Avoid unrelated UI redesign, Lua bindings, or session-format changes.
- Do not add or run bundled-script behavior/syntax tests or interactive egui tests.

## Stage 1 — State, actions, and persistence

- [x] Add the boolean to `GlobalControlState` in `src/rust/shoop_app_api/src/lib.rs`, defaulting to `false`.
- [x] Add its `GlobalControlAction` setter variant and stable action kind.
- [x] Add the field to `GlobalControlsDocument` in `src/rust/shoop_session/src/document.rs` with missing-field deserialization defaulting to `false`.
- [x] Wire capture, load, snapshots, fixtures, and struct initializers through `src/rust/shoop_app/src/lib.rs` and `src/rust/shoop_session`.
- [x] Add session tests for exact round-trip and loading a document without the new field.
- [x] Verify with targeted `shoop_app_api` and `shoop_session` tests.
- [x] Commit the completed state/persistence milestone.

**Dependency:** This stage precedes policy enforcement, UI, and Lua exposure.

## Stage 2 — Core monitoring policy

- [x] Extend the input-monitoring application action path to carry whether the operation respects auto-mute.
- [x] Centralize handling of input-monitoring changes in `src/rust/shoop_app/src/lib.rs`.
- [x] When enabling monitoring with respect enabled and the policy active, mute every non-target track before enabling the complete target set.
- [x] Keep disabling, bypassed requests, and policy-off requests scoped to their selected tracks.
- [x] Update existing internal call sites explicitly; callers that intentionally monitor several tracks must bypass the policy.
- [x] Add application-model tests covering policy off/on, bypass, muting, sync inclusion, multi-target behavior, and unchanged state when merely enabling the policy.
- [x] Verify with only the targeted non-GUI core tests.
- [x] Commit the completed core-behavior milestone.

## Stage 3 — Lua interface

- [x] Extend `ControlSnapshot` and global change detection with the policy value.
- [x] Add Lua getter/setter functions and corresponding `ControlOperation` support.
- [x] Extend `SetTrackInputMuted` with `respect_auto_mute`.
- [x] Preserve legacy setter arity while accepting and validating the new boolean argument.
- [x] Make Lua shadow-state updates mirror core target-group auto-muting.
- [x] Update `shoop_helpers.track_toggle_input_muted` to accept a selector and optional respect flag, using consistent group-toggle semantics.
- [x] Advance `LUA_API_VERSION` to `1.1`.
- [x] Add focused scripting-interface tests for getter/setter operations, legacy calls, respecting/bypassing behavior, multi-target shadow state, and argument validation.
- [x] Add an application-level Lua snippet test proving that a respecting operation reaches the core policy correctly; do not use either bundled script.
- [x] Commit the completed Lua-interface milestone.

## Stage 4 — Top-bar control

- [ ] Add the new toggle to `src/rust/shoop_egui/src/global_controls.rs` beside the existing synchronization/solo controls.
- [ ] Give it a concise active/inactive visual state and explanatory hover text.
- [ ] Emit the new global action when clicked.
- [ ] Make ordinary track input-monitoring buttons issue respecting requests.
- [ ] Update exhaustive action mappings and compile-time fixtures.
- [ ] Verify through compilation and core action tests only; do not add or run interactive egui tests.
- [ ] Commit the completed UI-wiring milestone.

## Stage 5 — Bundled scripts and documentation

- [ ] Update `keyboard.lua` documentation and handling for the **I** binding, including unique selected-track collection and respecting group toggle.
- [ ] Update the Akai documentation comments to clarify **PAN + grid**, column-to-track behavior, sync handling, and auto-mute-policy respect.
- [ ] Pass `true` explicitly from the Akai input-mute binding.
- [ ] Update both bundled scripts to announce API `1.1`.
- [ ] Update `docs/lua_compatibility_contract.md`, generated helper documentation inputs, and version references such as `src/rust/shoopdaloop/README.md`.
- [ ] Review script changes statically only; do not execute or syntax-test the bundled scripts.
- [ ] Commit the completed script/documentation milestone.

## Final end-to-end validation

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run targeted session, application-core, and Lua-interface tests that do not execute bundled scripts or interact with egui.
- [ ] Confirm tests demonstrate the complete path: Lua global setting → respecting input-unmute operation → target enabled and all non-target inputs muted.
- [ ] Confirm legacy sessions and legacy Lua setter arities retain policy-off behavior.
- [ ] Record that bundled-script execution and interactive GUI validation were intentionally deferred to the user.
- [ ] Commit any final validation-only corrections as a meaningful milestone.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
