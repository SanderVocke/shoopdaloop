# APC Mini click/hold global controls

## Goals and scope

Add reusable click/hold detection to `shoop_helpers.lua`, then use it for global toggle controls in the APC Mini MK1 and MK2 v3 scripts.

In scope:

- Shared click, hold-start, and hold-stop detection with a configurable timeout.
- MK1 SOLO and SYNC controls.
- MK2 v3 SOLO, SYNC (DRUM), and AUTO-MUTE (NOTE) controls.
- Updated leading Markdown documentation blocks in both controller scripts.

Out of scope: changing grid-pad actions, ordinary modifiers, controller mappings unrelated to these global controls, or the MK2 legacy/v2 scripts.

## Immutable acceptance criteria

- Releasing before the timeout emits exactly one click and no hold callbacks.
- Remaining pressed through the timeout emits exactly one hold-start; releasing afterward emits exactly one hold-stop and no click.
- Expired timers from earlier presses cannot affect a later press.
- A quick click permanently toggles the mapped global control.
- A hold toggles the control momentarily and restores its prior state when released.
- SOLO and SYNC follow these semantics on MK1 and MK2 v3; AUTO-MUTE follows them on MK2 v3.
- SHIFT is no longer used to make these global toggles permanent.
- MK2 v3 no longer requires firmware-reserved SHIFT + DRUM or SHIFT + NOTE gestures.
- The leading Markdown documentation in both scripts accurately describes click and hold behavior.

## Design rules and constraints

- Build the detector around `register_one_shot_timer_cb`; timers cannot be cancelled, so guard callbacks with pressed state and a generation token.
- Keep the timeout configurable and use a shared, documented controller-script value, initially 250 ms.
- Keep detection independent of the global control being changed; scripts supply click, hold-start, and hold-stop callbacks.
- Preserve current LED synchronization through existing global-event refresh paths.
- Do not change immediate behavior for grid pads or non-global modifier controls.

## Implementation stages

### 1. Shared helper

- [x] Define a concise click/hold detector API in `src/lua/lib/shoop_helpers.lua`.
- [x] Implement press/release state, generation guarding, timeout handling, and exactly-once callback dispatch.
- [x] Add focused Lua-runtime coverage for click, hold, stale timer, and repeated press/release behavior.
- [x] Verify the helper tests pass before integrating controller scripts.

### 2. MK1 integration

- [x] Replace the existing SOLO and SYNC press/release permanence state with detector instances.
- [x] Map click to permanent toggle and hold-start/hold-stop to momentary toggle/restore.
- [x] Remove SHIFT-based permanent-toggle behavior while preserving unrelated SHIFT actions.
- [x] Update the MK1 leading Markdown documentation block.
- [x] Verify Lua syntax and MK1 control-state tests or simulations.

### 3. MK2 v3 integration

- [x] Apply the detector to SOLO, SYNC (DRUM), and AUTO-MUTE (NOTE).
- [x] Remove SHIFT-based permanent-toggle behavior while preserving unrelated SHIFT actions.
- [x] Ensure plain DRUM and NOTE presses do not activate the controller's firmware-reserved shifted modes.
- [x] Update the MK2 v3 leading Markdown documentation block, including the relabeled control names.
- [x] Verify Lua syntax and MK2 v3 control-state tests or simulations.

### 4. End-to-end validation

- [x] Run formatting, Lua syntax, scripting API, and relevant controller/helper tests.
- [ ] Manually verify on MK1: click and hold behavior for SOLO and SYNC.
- [ ] Manually verify on MK2: click and hold behavior for SOLO, SYNC (DRUM), and AUTO-MUTE (NOTE), with no yellow/red firmware mode grid takeover.
- [x] Confirm both scripts' extracted Markdown documentation matches actual behavior.

Hardware validation status: the host exposes an APC Mini MK2 but no MK1. Completing the two manual checks requires physical button actuation and visual confirmation of the controller grid; automated MIDI/controller simulations cannot establish the absence of firmware color takeover.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
