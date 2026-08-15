# Browser smoke-test migration

## Result

The web CI matrix previously launched `browser_smoke.mjs` 13 times in the
debug job and 18 times in the release job (31 Chrome launches), plus one Firefox
launch in the release job. The retained policy launches exactly three packaged-application
smokes per workflow:

1. hosted Chromium, output-only physical AudioWorklet startup;
2. self-contained Chromium, output-only physical AudioWorklet startup; and
3. hosted Firefox, output-only physical AudioWorklet startup.

Each retained smoke proves a property which Node or a Chromium Worker cannot:
the packaged application loads, an actual `AudioWorkletNode` accepts application
commands, the browser render callback advances in 128-frame quanta, and shutdown
releases the browser process. The two Chromium forms preserve hosted and
single-file packaging coverage; Firefox preserves a second AudioWorklet
implementation. They intentionally do not repeat domain, Worker, settings,
Web MIDI, permission, restart, or stress assertions.

## Retired assertion map

| Retired browser workflow/assertion group | Deterministic replacement evidence |
| --- | --- |
| 360x200 and 900x600 session/UI self-tests | `shoop_egui::connection_dialog::tests::layout_paints_all_columns_at_small_and_common_sizes`, `shoop_egui::settings_dialog::tests::dialog_paints_category_tabs_at_minimum_and_common_sizes`, `shoopdaloop::tests::small_screens_use_a_larger_missing_setting_default`, and the shared `shoop_app` session/backend orchestration tests |
| hosted and direct-file physical audio domain round trips | Shared engine tests `session::tests::records_then_plays_back_end_to_end`, `audio_midi_loop_audio::audio_record`, and `audio_midi_loop_audio::audio_playback`; only genuine callback startup remains in smoke |
| hosted and self-contained Worker engine sessions | `shoop_wasm_runtime_tests::exact_production_worker_modules_process_and_isolate_instances`, whose four cases cover explicit processing, multi-instance isolation, transfer/restart cleanup, and terminal shutdown using the exact production Worker, host module, and raw Wasm |
| fixture control and application composition isolation | The same production Worker tests plus `shoop_worklet_client` transport/replay/isolation tests and shared `shoop_app` desired-state convergence tests |
| settings write, reload, unavailable storage, and self-contained settings | `shoopdaloop::settings::tests::{first_run_save_and_restart_publish_only_after_commit,rejected_source_requires_explicit_recovery_and_is_not_overwritten,failed_save_keeps_active_revision_and_prior_bytes,stale_draft_is_rejected_without_writing,unknown_values_survive_manager_save}` and `shoop_settings::settings::tests::*` |
| Web MIDI success, denial, open failure, routing, and self-contained Web MIDI | `shoopdaloop::browser_midi::tests::{lifecycle_and_hotplug_publish_revisioned_stable_endpoints,input_fans_out_to_control_subscribers_and_track_queue,track_and_control_limits_refuse_without_truncation,bounded_queues_count_drops,direction_validation_rejects_output_as_input_source}`, `shoop_audio_worklet::tests::web_midi_commands_route_record_monitor_and_playback`, and shared backend Web MIDI route/refusal tests |
| microphone permission denial and retry | Shared `shoop_app` active-I/O transition and preflight rejection tests; the retained smokes do not request microphone permission |
| repeated start, suspend/resume, shutdown/restart, and media ownership | Shared `shoop_app` active-I/O lifecycle tests, `shoop_worklet_client::tests::driver_restart_cancels_active_transfer_and_releases_staged_bytes`, and the terminal-shutdown case in `shoop_wasm_runtime_tests::exact_production_worker_modules_process_and_isolate_instances` |
| stress callback count, bounded overflow, and render diagnostics | Shared bounded queue/storage tests in `shoop_engine`, `shoop_backend::tests::saturated_web_midi_render_is_allocation_free_and_counts_refusal`, and Worker explicit-processing contracts; real callback progress remains in all three smokes |
| waveform, MIDI-detail, connection-dialog, Tiny Synth FX, and session-shape assertions | Shared `shoop_egui::{waveform,waveform_widget,midi_sequence_widget,details_pane,connection_dialog,tiny_synth_fx_editor}::tests::*`, `shoop_backend::tests::tiny_synth_fx_processes_audio_midi_controls_and_session_state`, and `shoop_session::tests::*` |

The replacement tests are emitted by `#[shoop_test]` and therefore retain the
same native test identity while also running in the Node and Chromium Wasm
suites. Their complete machine-readable membership is in
`target/wasm-tests/<profile>/inventory.json`; classification policy is in
`tests/wasm_test_classification.toml`.

## Timing and ownership

The pre-migration baseline in `docs/wasm_test_baseline.md` records approximately
6-7 minutes for the repeated Chrome work in each web job and 4 seconds for the
Firefox workflow itself (excluding browser setup). The post-migration workflow
source has two `browser_smoke.mjs` invocations and one
`browser_firefox_smoke.py` invocation. CI timing is recorded from PR #751 after
this policy lands; local retained-smoke measurements and the final CI values are
appended to the baseline rather than estimated.

The Wasm suite owns domain and production Worker behavior. Packaged-browser
smoke owns only physical AudioWorklet callback and packaging evidence. New
browser behavior must first be added to the shared or production-Worker Wasm
suite; a fourth smoke requires an explicit update to this document explaining
why neither lower layer can observe the property.
