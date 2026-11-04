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
- Production capability is still disabled. Library evidence alone does not close
  P12 or establish production persistence, recovery or browser support.

Final acceptance must add all nine plan criteria, final source/check identities,
Q matrix, native/browser output artifacts, persistence/recovery results and review.
