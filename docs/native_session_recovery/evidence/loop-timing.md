# P12 loop timing acceptance evidence

Status: in progress; P12 is OPEN. Implementation branch: `feat/p12-loop-timing`,
base `df4683527dbde0ec7483c3e522fc3873e39a81c8`, PR target
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

## Recorded validation

- Before behavioral changes: 18 targeted reference/backend/editor tests passed,
  covering channel timing, alignment-adjusted details, armed-recording length
  refusal and the complete details-pane test selection.
- Initial window foundation: warning-denying workspace `cargo check` passed.
- Initial window foundation: all 17 `shoop_loops --test runtime` cases passed,
  including three new heap-guarded integration scenarios: absolute-position/raw
  retention and independent channel windows; atomic stale/duplicate/armed-recording
  refusals; and audible stopped pre-play with uninterrupted MIDI pickup notes.
- Warning-denying workspace build and runtime/loop/engine dependency checks for
  all four supported dependency-audit targets passed; the test-attribute policy
  also passed.
- RT transaction milestone: all 88 engine/adapter regression tests passed.
  `timing_edits::window_edits_keep_raw_recordings_atomic_results_and_recoverable_state_at_every_quantum`
  passes at Q=1/16/32/64/128 with guarded engine calls, actual recorded stereo
  output/raw bulk comparisons, zero-length retention, resync and recording edits.
  RT codec tests include full-window/result round-trips and malformed batch bounds.
  The warning-denying workspace build and test-attribute policy pass.
- Production capability is still disabled. Library evidence alone does not close
  P12 or establish production persistence, recovery or browser support.

Final acceptance must add all nine plan criteria, final source/check identities,
Q matrix, native/browser output artifacts, persistence/recovery results and review.

## Retained-media and deployment checkpoint

Raw uploads carry a bounded private-source window header; raw durations need not
be quantum aligned. Logical periods still must be aligned. Imported channel gains
apply during rendering and raw export samples remain exact. The adapter retains
validation proofs across window-only edits of the same immutable buffers.

- `shoop_rt_backend::persistence::timing_windows_preserve_raw_audio_midi_proofs_and_preplay_through_replacement`
  covers independent audio/MIDI timing, channel gain, nonzero pre-play, zero length,
  re-extension, raw-buffer sharing, save/load/save and clear.
- `shoop_rt_engine::timing_edits::retained_raw_lead_in_starts_inside_quantum_and_carries_midi_pickups`
  verifies actual output at Q=1/16/32/64/128, frame-precise raw media, different
  channel lead-in onsets, no gap/duplicate pickup at start and delayed output cleanup.
- `shoop_loops::runtime::timing_scenarios::cancelled_superseded_and_invalidated_pickups_release_notes_without_starting`
  checks cancellation, replaced starts and source Stop against samples and packets.
- Production service tests check typed/encoded channel batches, recording admission,
  retained results, stale-attachment refusal and no render waits. Lua length getters
  and change events observe confirmed edits without restarting the script.
- `scripts/run_loops_hosts.py --native-only --profile debug/release`: both pass.
- Bare-metal `thumbv7em-none-eabihf` build: pass.
- Selected pinned Miri cases for raw windows, pickup continuity and atomic armed
  refusal: pass, seed 0; reports retained locally for final source/hash inventory.
- Packaged socket/shared-memory canaries: live timing and retained raw reads pass;
  socket reattachment retains admitted recording metadata.

The capability is enabled for integration acceptance. Full-suite, final browser
acceptance and final-head CI are still required before P12 can close.
