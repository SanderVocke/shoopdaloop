# Built-in Lua Akai and keyboard audit plan

## Goal

Produce a static, end-to-end audit of the bundled controller scripts:

- `src/lua/builtins/akai_apc_mini_mk1.lua`
- `src/lua/builtins/keyboard.lua`

The completed audit must show, for every advertised binding and every significant body-only behavior, whether its Lua API is present and correctly used, whether the current application path is expected to work through the engine or other relevant boundary, which existing Rust tests prove that exact situation, and which Rust tests were added to close gaps. The result should separate well-tested behavior, plausible but insufficiently tested behavior, and suspicious or missing behavior so that later fixes can be prioritized.

This is an audit-and-test task, not a behavior-fix task. Correctness conclusions must come from reading the current code path, not from manually operating the application or controller.

## Scope

### In scope

- Every documentation-comment behavior at the top of both scripts, including every key, button, fader, press/release behavior, modifier, and advertised combination.
- Significant additional behavior found in the script bodies, identified as **body-only** in the inventory below.
- The Lua helper and coordinate implementations used by the scripts.
- Lua API registration, constants, argument conversion, selector handling, operation generation, callback dispatch, and MIDI handling.
- Application handling of generated operations and events, including model state, backend calls, state publication, and the engine boundary where applicable.
- Existing Rust tests at all layers, with an explicit assessment of whether they cover the API in the script's actual situation rather than only testing a lower-level primitive.
- New or extended Rust tests for every row whose situational coverage is insufficient. A newly added test may remain failing if it exposes a real gap; its failure and reason must be recorded.

### Out of scope

- Fixing Lua scripts, helper libraries, application behavior, engine behavior, or documentation discovered to be wrong.
- Changing advertised semantics or silently narrowing an audit row to match the implementation.
- Hardware testing, manually running the application, or treating observed runtime behavior as a substitute for a code-path audit.
- Broad refactors unrelated to focused Rust audit tests.

## Relevant code and likely evidence surfaces

Start with these surfaces and follow symbols further whenever the path continues:

- Lua scripts and libraries: `src/lua/builtins/`, `src/lua/lib/shoop_helpers.lua`, `src/lua/lib/shoop_coords.lua`, `src/lua/lib/shoop_midi.lua`, and `src/lua/lib/shoop_control.lua`.
- Lua host/API: `src/rust/shoop_scripting/src/control.rs`, `src/rust/shoop_scripting/src/lib.rs`, `src/rust/shoop_scripting/src/key_constants.rs`, and `src/rust/shoop_scripting/src/midi.rs`.
- Input and public contracts: `src/rust/shoop_egui/src/key_input.rs` and `src/rust/shoop_app_api/src/lib.rs`.
- Application-to-backend behavior and primary integration tests: `src/rust/shoop_app/src/lib.rs`.
- Engine implementation and tests: `src/rust/shoop_engine/src/` and `src/rust/shoop_engine/tests/`.

Existing broad integration tests such as `production_keyboard_script_handles_navigation_modes_numbers_targets_and_releases`, `production_keyboard_plays_manual_recording_on_next_sync_cycle`, `unchanged_apc_script_drives_authoritative_state_and_bounded_led_output`, and `unchanged_apc_script_uses_native_virtual_midi_when_available` are search anchors, not automatic proof for every row. Count them only where their setup and assertions actually prove the listed behavior and relevant boundary.

## Immutable acceptance criteria

1. Every inventory row below is resolved, and a final line-by-line comparison against both script headers and bodies finds no omitted advertised binding, combination, press/release behavior, or significant body-only behavior.
2. Every row names the exact Lua/helper calls and has an API audit that establishes whether each function and constant exists, accepts the supplied argument shape, selects the intended loops/tracks, and produces the intended operation or callback behavior.
3. Every row contains a static path audit from input/callback through Lua and the host bridge into authoritative application code and on to the backend/engine boundary. For MIDI output, LEDs, selection, or other behavior with a different terminal boundary, that boundary and why no engine call is expected are stated explicitly.
4. Every row lists exact existing Rust evidence as `path::test_name` plus the relevant assertion or scenario. Generic API tests are labeled partial and are not treated as situational coverage.
5. Every row has one final verdict from the taxonomy below, with a concise evidence-based reason.
6. Every row deemed insufficiently tested has one or more focused Rust tests added or extended, and the exact `path::test_name` and pass/fail status are recorded. Failing tests are retained and described rather than weakened or mislabeled.
7. No production behavior or Lua documentation is changed as part of this audit without explicit user approval.
8. The completed document contains no `PENDING` cells, unresolved shorthand, unnamed tests, or unsupported claims.

## Design and evidence rules

- Preserve the advertised behavior as the expected contract even when the body appears to disagree; record the disagreement as suspicious.
- Audit the production bundled script itself. A test of a rewritten Lua snippet is only lower-level evidence unless the behavior cannot reasonably be exercised through the bundled script.
- Distinguish four evidence levels when useful: Lua API conversion/operation tests, application tests with a fake backend, application tests with the dummy/real engine backend, and native host/device integration. State which level each test reaches.
- A test name alone is not evidence. Record what input it supplies and what state, operation, backend effect, MIDI output, or published result it asserts.
- Validate coordinate conventions (`-1` sync track versus zero-based regular tracks), selector cardinality, ordering, modifier precedence, press/release state, and empty-selection behavior explicitly.
- For transition operations, inspect synchronization, delay, alignment, repeat-sync, apply-N-cycles, and global-control handling all the way to the backend call; do not infer correctness merely from a changed application snapshot.
- For track controls, inspect fader conversion, dB conversion, balance range, stereo assumptions, muting, input monitoring, and `respect_auto_mute` behavior.
- For MIDI behavior, inspect host endpoint direction, regex matching, reconnect behavior, callback timing, rate limiting, message validation/classification, note/CC mapping, and output queue behavior.
- Do not use manual app or hardware execution as audit evidence. Rust tests may be run while authoring tests; record their exact result, including intentional failures that expose defects.
- If one Rust test covers several rows, cite it in each row and identify the distinct assertion for each. If one row needs several layers of proof, cite all relevant tests.
- Newly discovered significant body behavior must be appended with a stable `AK-*` or `KB-*` ID and marked **Body-only**. Do not remove an existing row because implementation evidence changes.

## Verdict taxonomy

- **PASS — expected and sufficiently tested**: the complete static path is sound and contextual Rust coverage reaches the relevant boundary.
- **WORKS-BUT-UNTESTED**: the complete static path appears sound, but existing contextual coverage is absent or partial. This verdict requires a new/extended Rust test in the final column; after adding a passing adequate test, normally promote the row to `PASS`.
- **SUSPICIOUS**: an API is missing/misused, advertised semantics disagree with the body, the application/backend path is incomplete, or another concrete defect risk exists. Add a focused regression/characterization test; it may remain failing.
- **NOT-APPLICABLE**: only for a clearly justified non-engine boundary or a behavior that cannot require a test. This does not waive the requirement to audit and test script-local or MIDI behavior where Rust can exercise it.

## How to fill the audit table

The first four columns are the complete initial inventory and must remain stable except for evidence-backed clarification or newly appended body-only rows. Replace every `PENDING` cell as follows:

1. **API audit** — list every exact API/helper/constant used by the row and state `OK`, `MISMATCH`, or `MISSING` for existence, argument shape, coordinate/selector handling, return shape, and generated `ControlOperation` or callback effect. Include symbol paths, not just module names.
2. **Static app → boundary path** — write the concrete call chain from keyboard/MIDI event or callback through Lua, `shoop_scripting`, application operation handling, and backend/engine method. Name the final observable boundary. Mark breaks or semantic mismatches at the exact symbol.
3. **Existing Rust evidence** — cite `path::test_name` and summarize the scenario/assertion that covers this row. Write `None found (searched: …)` if no contextual test exists. Label lower-level-only evidence `Partial`.
4. **Verdict** — use exactly one taxonomy value and give a short reason tied to the preceding columns.
5. **New/extended Rust test(s)** — for insufficient coverage, cite every added `path::test_name`, what gap it closes, and `PASS` or `FAIL` with the failure reason. If existing coverage is sufficient, write `Not needed — <specific existing evidence>`. Do not write only `N/A`.

Audit one row at a time, but re-check shared helpers and operations whenever later evidence changes an earlier conclusion. Keep the table updated immediately after each investigation or test addition.

## Master audit table

| ID | Contract source and binding/event | Functionality that must be proved | Candidate Lua/helper path to verify | API audit | Static app → boundary path | Existing Rust evidence | Verdict | New/extended Rust test(s) |
|---|---|---|---|---|---|---|---|---|
| AK-01 | Advertised mapping: grid notes `0..63`; CLIP STOP `82`; SOLO `83`; REC ARM/RECORD `84`; MUTE/GRAB `85`; SELECT `86`; blank 1/SYNC `87`; blank 2/sync loop `88`; STOP ALL CLIPS `89`; SHIFT `98`; VOLUME `68`; PAN `69`; SEND/DRY `70`; DEVICE/SET N CYCLES `71`; faders CC `48..56` | Physical controls and renamed labels route to the intended logical button, grid coordinate, regular track, or sync track. | `note_to_loop_coords`; `cc_to_fader_track`; button constants | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-02 | Advertised startup behavior: controller appears/disappears | Automatically discover, open, connect, disconnect, and reconnect matching APC MIDI input/output endpoints. | `auto_open_device_specific_midi_control_input`; `auto_open_device_specific_midi_control_output` with `.*APC MINI MIDI.*` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-03 | Advertised startup plus body detail: output connection/reconnection | Install `port.send`, schedule a delayed reset, and emit reset LEDs through the configured positive output rate limit without an unbounded burst. | output open/connected callbacks; `register_one_shot_timer_cb(1000, reset)`; `port.send`; `reset` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-04 | Advertised: grid buttons indicate loop state | Loop events produce off for empty/stopped, yellow for nonempty/stopped, green for both playing modes, and red for both recording modes; cached colors suppress duplicates and reset can resend all regular/sync-loop colors. | `register_loop_event_cb`; `push_loop_color`; `push_all_loop_colors`; loop event fields and `LoopMode_*`; `port.send` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-05 | Advertised global controls plus body feedback | Global events and reset refresh SOLO and SYNC LEDs according to current global state, including the script's intentional SYNC LED polarity. | `register_global_event_cb`; `recheck_global_controls`; `get_solo`; `get_sync_active`; `port.send` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-06 | Body-only: press/release feedback | SHIFT, SELECT, RECORD, GRAB, CLIP STOP, DRY, SET N CYCLES, VOLUME, and PAN LEDs turn on while held and off on release without corrupting held state. | `handle_noteOn`; `handle_noteOff`; `set_led_by_note`; state variables | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-07 | Advertised: grid button with no action modifier | Perform the complete default loop action, including transition cancellation, empty-loop record/grab policy, recording→playing, stopped nonempty→playing, and other states→stopped. | `shoop_helpers.default_loop_action(coords, false)`; mode/length/next-mode getters; `get_default_recording_action`; `loop_trigger`; `loop_trigger_grab` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-08 | Advertised: DRY + loop/grid button | Use the dry variant of the default action so transitions to playback become Playing Dry Through Wet while all other default-action branches remain correct. | `shoop_helpers.default_loop_action(coords, true)` and its getter/trigger/grab chain | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-09 | Advertised: CLIP STOP + loop/grid button | Stop the clicked loop. | `loop_trigger(coords, LoopMode_Stopped)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-10 | Advertised: SHIFT + CLIP STOP + loop/grid button | Clear the clicked loop. | `loop_clear(coords)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-11 | Advertised: RECORD (REC ARM) + loop/grid button | Record the clicked loop. | `loop_trigger(coords, LoopMode_Recording)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-12 | Advertised: RECORD + DRY + loop/grid button | Re-record dry into wet on the clicked loop. | `loop_trigger(coords, LoopMode_RecordingDryIntoWet)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-13 | Advertised: GRAB (MUTE) + loop/grid button | Retroactively grab into the clicked loop from the running buffer. | `loop_trigger_grab(coords)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-14 | Advertised: SHIFT + GRAB + loop/grid button | Toggle the default empty-loop recording action between `record` and `grab`. | `get_default_recording_action`; `set_default_recording_action` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-15 | Advertised: SELECT + loop/grid button | Toggle selection of the clicked loop. | `loop_toggle_selected(coords)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-16 | Advertised: SHIFT + SELECT + loop/grid button | Toggle targeting of the clicked loop while respecting the application's single-target semantics. | `loop_toggle_targeted(coords)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-17 | Advertised mapping plus body detail: blank 2/sync-loop button, alone and with loop modifiers | Route note `88` to `{-1, 0}` and apply all applicable loop-button actions to the sync loop. Explicitly audit body-only consequences such as SET N CYCLES + sync-loop yielding zero and composition attempts involving the sync loop. | `note_to_loop_coords`; `handle_loop_pressed` with sync coordinates; all reached helper/control calls | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-18 | Advertised: hold SOLO, then release | Toggle solo on press and toggle it back on release for momentary behavior. | `shoop_helpers.toggle_solo`; `get_solo`; `set_solo`; SOLO held/permanent state | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-19 | Advertised: SHIFT + SOLO | Toggle solo permanently: release SOLO must not restore the previous toggle. | SOLO note handlers; `toggle_solo`; permanent-state tracking | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-20 | Advertised: hold SYNC, then release | Toggle synchronization active on press and toggle it back on release for momentary behavior. | `shoop_helpers.toggle_sync_active`; `get_sync_active`; `set_sync_active`; SYNC held/permanent state | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-21 | Advertised: SHIFT + SYNC | Toggle synchronization active permanently: release SYNC must not restore the previous toggle. | SYNC note handlers; `toggle_sync_active`; permanent-state tracking | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-22 | Advertised: STOP ALL CLIPS | Stop every loop. | `loop_get_all`; `loop_trigger(..., LoopMode_Stopped)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-23 | Advertised: SELECT + STOP ALL CLIPS | Deselect every loop. | `loop_select({}, true)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-24 | Advertised: SHIFT + STOP ALL CLIPS | Clear every loop. | `loop_clear_all()` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-25 | Advertised: hold DEVICE/SET N CYCLES + grid button | Set apply-N-cycles to row-major values `1..63`, with the bottom-right grid button setting `0`. | grid coordinate conversion; `(x + y * 8 + 1) % 64`; `set_apply_n_cycles` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-26 | Advertised: fader without VOLUME or PAN | Moving a fader has no effect unless one of the two mode buttons is held. | `handle_cc` state guards | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-27 | Advertised: hold VOLUME + move regular-track fader | Map MIDI `0..127` to gain-fader `0.0..1.0` and set output gain for the intended regular track. | `cc_to_fader_track`; `track_set_gain_fader(track, value / 127.0)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-28 | Advertised: hold PAN + move regular-track fader | Map MIDI `0..127` to balance approximately `-1.0..1.0`, set the intended track, and verify the advertised stereo-only expectation. | `cc_to_fader_track`; `track_set_balance(track, value / 63.5 - 1.0)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-29 | Advertised: rightmost fader reserved for sync-loop track | CC `56` controls sync track `-1` for both VOLUME/gain and PAN/balance modes. | `cc_to_fader_track(56)`; `track_set_gain_fader(-1, ...)`; `track_set_balance(-1, ...)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-30 | Advertised: VOLUME + loop button | Toggle output mute on the track containing the clicked loop, including the sync loop when its loop button is used. | `shoop_helpers.track_toggle_muted(coords[1])`; `track_get_muted`; `track_set_muted` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-31 | Advertised: PAN + regular grid button | Toggle input mute once for the track represented by the grid column; unmuting must honor auto-mute-other-inputs. | `shoop_helpers.track_toggle_input_muted(coords[1], true)`; `track_get_input_muted`; `track_set_input_muted` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-32 | Advertised: PAN + sync-loop button | Toggle sync-track input mute and honor auto-mute-other-inputs when unmuting. | sync coordinate `-1`; `track_toggle_input_muted(-1, true)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-33 | Advertised composition: hold SHIFT + DRY, click target | Enter composition mode and choose the first clicked loop as target without clearing an existing composition. | modifier state; `STATE_composition_active`; target state; no operation on first target click | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-34 | Advertised composition: subsequent source loops pressed serially | Append each subsequently clicked source as a new serial section at the end of the target composition. | `loop_compose_add_to_end(target, source, false)`; press/release parallel counter | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-35 | Advertised composition: multiple source loops held together | Put additionally pressed source loops in parallel with the current composition section while preserving source order and target contents. | `loop_compose_add_to_end(target, source, true)`; parallel counter | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-36 | Advertised composition: SHIFT + DRY held throughout | Source releases update parallel state safely, and releasing DRY exits composition and clears the target so later loop presses are normal actions. | `handle_loop_released`; DRY note-off state reset | PENDING | PENDING | PENDING | PENDING | PENDING |
| AK-37 | Body-only: SHIFT + DEVICE/SET N CYCLES | Perform the controller LED reset/debug action instead of entering N-cycles mode. | DEVICE note-on branch; `reset`; global/loop LED resend | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-01 | Body-only infrastructure: keyboard callback registration and event lifecycle | The bundled script receives translated non-repeat press/release events outside text entry, including synthetic releases on focus loss, with matching key/modifier constants. | `register_keyboard_event_cb(handle_keyboard)`; `KeyEventType_*`; `Key_*`; `KeyModifier_*` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-02 | Advertised arrows with empty selection; body-only fallback | Select `{0, 0}` when available; if no regular track exists, fall back to sync loop `{-1, 0}` rather than leaving selection empty. | `loop_get_which_selected`; `loop_select`; follow-up selection query | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-03 | Advertised: unmodified arrow key with selected loop(s) | Move the entire selected group one cell in the direction only when every destination exists and remains in bounds. | `shoop_helpers.move_selection`; `shoop_coords.move`; `loop_count`; `loop_select` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-04 | Advertised: CTRL + arrow | Expand selection in the requested direction, including edge/out-of-bounds selector behavior. | modifier mask; `shoop_helpers.expand_selection`; `shoop_coords.move`; `loop_select` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-05 | Body-only: ALT + arrow | Shrink selection by removing the extreme row/column from the indicated direction. | modifier mask; `shoop_helpers.shrink_selection`; `shoop_coords.extreme`; `loop_select` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-06 | Advertised: ESCAPE | Clear all loop selection. | `loop_select({}, true)` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-07 | Advertised: SPACE | Perform the complete default action on selected loops: cancel queued transitions, obey record-versus-grab policy for empty loops, transition recording→playing, stopped nonempty→playing, and other states→stopped. | `loop_get_which_selected`; `shoop_helpers.default_loop_action`; its getter/trigger/grab chain | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-08 | Advertised: R | With selection, trigger Recording; without selection, replace selection with all currently Recording loops. | `handle_loop_action(LoopMode_Recording)`; `loop_get_which_selected`; `loop_trigger`; `loop_get_by_mode`; `loop_select` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-09 | Advertised: P | With selection, trigger Playing; without selection, replace selection with all currently Playing loops. | `handle_loop_action(LoopMode_Playing)` and shared query/trigger/select calls | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-10 | Advertised: S | With selection, trigger Stopped; without selection, stop all loops rather than selecting stopped loops. | `handle_loop_action(LoopMode_Stopped)`; `loop_get_all`; `loop_trigger` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-11 | Advertised: L | With selection, trigger Playing Dry Through Wet; without selection, select all loops currently in that mode. | `handle_loop_action(LoopMode_PlayingDryThroughWet)` and shared calls | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-12 | Advertised: M | With selection, trigger Recording Dry Into Wet; without selection, select all loops currently in that mode. | `handle_loop_action(LoopMode_RecordingDryIntoWet)` and shared calls | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-13 | Advertised: I | Deduplicate tracks containing selected loops, toggle their input mutes as one group, and honor auto-mute-other-inputs when unmuting. | `toggle_selected_track_inputs`; `shoop_helpers.track_toggle_input_muted(tracks, true)`; track get/set input-muted APIs | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-14 | Advertised: N, “Record next” | Choose the first empty loop in the one selected track, or recording track(s) when the selection rule does not yield one track; stop recording loops and record the chosen loop(s). | `shoop_helpers.record_into_first_empty(false)`; selected/recording/track/mode/length queries; `loop_trigger` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-15 | Advertised: G, “Grab” | Grab running-buffer data into selected loops retroactively, with empty-selection behavior explicitly characterized. | `loop_get_which_selected`; `loop_trigger_grab` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-16 | Advertised: O, “Overdub” | Choose first empty loop(s) by the same track rule as N, transition current recording loops to Playing, and record the chosen loop(s). | `shoop_helpers.record_into_first_empty(true)` and its query/trigger chain | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-17 | Advertised: T | Choose one selected loop under the API's defined ordering, toggle it as the sole target, and untarget it if it is already targeted. | `loop_get_which_selected`; `loop_toggle_targeted` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-18 | Advertised: U | Untarget every loop. | `loop_untarget_all()` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-19 | Advertised: W | Record selected loop(s) in synchronization with the currently targeted loop, including empty-selection/no-target behavior. | `loop_get_which_selected`; `loop_record_with_targeted` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-20 | Advertised: C | Clear selected loops and do nothing outside that selector. | `loop_get_which_selected`; `loop_clear` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-21 | Advertised: `.` press and release | On press, immediately record empty stopped loops or play nonempty stopped loops without sync; on release/focus loss, immediately stop all sampler-started loops and restore repeat-sync. | `shoop_helpers.start_sampler`; `stop_sampler`; mode/length getters; `loop_set_repeat_sync`; `loop_transition` with both `Loop_Dont*` constants | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-22 | Advertised: one `0`–`9` key | Set apply-N-cycles to that digit, including `0` disabling bounded actions. | `as_number_key`; pressed-number state; `set_apply_n_cycles` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-23 | Advertised combination: hold multiple `0`–`9` keys, e.g. `1` then `2` | Build an ordered decimal number such as `12`, avoid duplicate held instances, and remove released digits from held state without changing the already applied value until another press. | `handle_number_pressed`; `handle_number_released`; `update_n_cycles`; `set_apply_n_cycles` | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-24 | Advertised cross-cutting modifier: hold CTRL | Momentarily invert global synchronization active while CTRL is held and restore it on release, while remaining compatible with CTRL + arrow expansion. The script body has no obvious standalone modifier handler, so trace host behavior before judging. | No direct call visible in `keyboard.lua`; inspect key translation/dispatch and any host global-sync handling | PENDING | PENDING | PENDING | PENDING | PENDING |
| KB-25 | Advertised cross-cutting transition rule | Every applicable loop-transitioning binding is immediate or sync-scheduled according to global synchronization active state, with apply-N-cycles and targeted/sampler exceptions handled as documented. | `default_loop_action`; `record_into_first_empty`; `loop_trigger`; `loop_trigger_grab`; `loop_record_with_targeted`; `loop_transition`; global snapshot fields | PENDING | PENDING | PENDING | PENDING | PENDING |

## Ordered execution and verification

### Inventory and shared-path pass

First, compare the header comments and every event-handler branch against the master table. Append only genuinely missing significant body behaviors. Build a shared call map for helpers and APIs so repeated rows use consistent coordinate, selector, and operation semantics. Verify this pass by recording the compared header sections and body handlers in an audit note beneath the table; do not mark any behavioral verdict yet unless its full path has been read.

### Akai static audit pass

Resolve `AK-01` through `AK-37` in order. Trace MIDI endpoint ownership and direction before button behavior, then message routing/state precedence, Lua calls, application operations, backend effects, and LED output. Revisit all rows affected by shared state or modifier precedence. Verify the pass by ensuring every Akai row has complete API/path evidence and no unsupported inference from the broad APC test.

### Keyboard static audit pass

Resolve `KB-01` through `KB-25` in order. Start at egui key translation and release behavior, then trace the bundled callback, helpers, API conversion, application operation handling, and backend scheduling. Treat the advertised CTRL momentary-sync behavior as a required contract even if no implementation is found. Verify the pass by ensuring every keyboard row has complete API/path evidence and by rechecking no-selection and modifier cases separately.

### Existing-test evidence pass

Search Rust unit and integration tests by script constant, callback/API symbol, `ControlOperation`, application handler, backend method, and expected mode/state. Read setup and assertions before citing a test. For each row, record full, partial, or absent situational coverage. Verify that no row is called sufficiently tested solely because a lower-level API exists or a large integration test happens to load the script.

### Gap-test pass

For every row that lacks sufficient contextual evidence, add or extend focused Rust tests at the lowest layer that can still prove the script's real situation. Prefer loading `KEYBOARD_SCRIPT` or `AKAI_APC_MINI_MK1_SCRIPT`, injecting synthetic key/MIDI events, and asserting authoritative application plus backend/engine results. Use fake MIDI for deterministic controller input/output and a dummy engine backend where timing or engine transition semantics must be proved. Keep tests narrow enough that table rows can cite distinct assertions. Run targeted tests while iterating and record `PASS` or the exact expected `FAIL`; do not repair production behavior in this task.

### Final end-to-end validation

Perform a final header/body-to-table comparison, re-read every shared helper/API path whose conclusion changed, and ensure all acceptance criteria hold. Then record the exact outcomes of:

```sh
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1
```

Also run the focused test filters used during development and record them near the affected table rows. A failing newly added regression test is allowed, but the final summary must identify its row, command, failure, suspected broken layer, and why the test was retained. No `PENDING` table cell may remain.

## Final audit summary to add during execution

After the table is complete, add a concise summary containing:

- totals and IDs for `PASS`, `WORKS-BUT-UNTESTED`, `SUSPICIOUS`, and `NOT-APPLICABLE`;
- all confirmed header/body mismatches and body-only behaviors;
- all missing or misused APIs and all broken/incomplete app-to-engine paths;
- newly added passing and failing Rust tests;
- a prioritized follow-up list for script, API, application, engine, and documentation fixes, without implementing those fixes.

## Execution contract

- Keep this document updated as work progresses and replace table cells immediately when evidence is established.
- Commit each completed script audit, test group, or other meaningful milestone.
- Investigation and test steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- If a path cannot be resolved after following its symbols and searching tests, stop that row with the evidence gathered, attempted paths, exact blocker, and next input needed; do not guess.
