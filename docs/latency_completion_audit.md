# Latency compensation completion audit

This audit maps the authoritative requirements in `latency_comp.md` to implementation and direct verification artifacts. The completion contract is: all 235 plan checkboxes are complete, all 26 immutable criteria retain direct evidence, every documented final gate passes in the selected Nix environment, and facility-dependent checks either pass or have a specific recorded facility limitation.

## Immutable acceptance criteria

| # | Requirement | Direct evidence |
|---|---|---|
| 1 | Monitoring remains immediate | `latency_characterization::current_monitoring_is_sample_identical_across_callback_sizes`; `session::dry_render_lookahead_does_not_retime_live_monitoring`; deterministic matrix cross-action monitoring row. |
| 2 | Deterministic capture alignment | `latency_characterization::record_then_play_matrix_matches_raw_and_logical_audio_midi_oracles` covers exact raw/logical audio and MIDI frames over the mandatory component and boundary values. |
| 3 | Deterministic wet alignment | The same record/play matrix covers live wet `I + P + H + O + T`; `deterministic_fixture_tracks_all_frame_domain_components` checks independently identifiable components. |
| 4 | Dry-through-wet PDC | `dry_through_wet_component_matrix_matches_audio_midi_processor_oracles`, `dry_through_wet_start_steady_wrap_stop_restart_and_parallel_loops_are_exact`, and `planned_render_matrix_dispatches_exactly_before_public_transition`. |
| 5 | No wet-rerecord double compensation | `dry_into_wet_component_and_boundary_matrix_writes_one_canonical_event`, `dry_midi_into_wet_audio_preserves_state_order_and_canonical_timing`, and `session::delayed_dry_into_wet_writes_canonical_take_without_double_compensation`. |
| 6 | Independent component control | `shoop_latency::every_component_toggle_and_mode_resolves_independently`; `latency_panel::policy_normalization_and_totals_cover_modes_ranges_cue_and_no_backend`; app optimistic reconciliation tests. |
| 7 | Conditional cue/output semantics | `shoop_latency::operation_recipes_enforce_component_and_cue_semantics`; `shoop_backend::selected_cue_output_contributes_only_to_cue_followed_recording`; record matrix cue/world rows. |
| 8 | Stable takes | `shoop_latency::take_snapshot_is_frozen_and_detects_later_revision_changes`; `audio_midi_loop::latency_recipes_latch_only_on_matching_operation_boundaries_and_mark_changes`; ordinary-play matrix changes current observations without retiming the take. |
| 9 | Truthful dynamic changes | Stable/variable grab tests, operation-latch test, backend rapid-transition stress test, persisted `changed`/variable-history session assertions, and UI warning tests. |
| 10 | Complete bounded capture windows | Audio/MIDI postroll and prerecord tests; `play_after_record_defers_until_compensated_postroll_is_ready`; insufficient grab/margin tests prove visible failure before mutation. |
| 11 | JACK awareness | `jack_app_backend::jack_latency_callback_publishes_connected_port_ranges`, retirement stress, route filtering, and two-buffer-size external send/return measurement; dedicated JACK2 run is recorded below. |
| 12 | Carla awareness | `carla_native::real_nonzero_rack_and_branched_patchbay_latency_match_impulse_paths`, real subprocess worker coverage, version/ABI checks, and `carla_latency_compatibility` unsupported-runtime fallback. |
| 13 | OxiSynth truthfulness | `oxisynth::event_application_exposes_64_frame_phase_latency`, all event-type/offset characterization, odd callback tests, and the declared `0..=63` range contract. |
| 14 | Ranges stay ranges | `shoop_latency::checked_observations_preserve_truthful_certainty`; bounded protocol round trips; backend/app/worklet/session/UI certainty tests. |
| 15 | Per-path correctness | `shoop_latency::path_aggregation_distinguishes_equivalent_ranged_unknown_and_ambiguous`; independent state-mirror revisions; Carla branched Patchbay and JACK ambiguity coverage. |
| 16 | Defined replacement and grab | Stable/variable audio/MIDI grab matrix, transactional insufficient-history failure, compatible replacement tests, and `session::incompatible_latency_replacement_requires_consolidation_before_mutation`. |
| 17 | Exact persistence | `shoop_session::latency_documents_round_trip_and_reject_inconsistent_metadata_transactionally`, deterministic exact archive tests, and same-rate replay timing oracle. |
| 18 | Deterministic resampling | `resampling_converts_every_sample_domain_and_preserves_midi_order` and `same_and_cross_rate_session_restore_replays_ordinary_and_dry_wet_timing`; rules are documented in `docs/session_format_v1.md`. |
| 19 | Explicit import/export | App logical/raw audio and MIDI export/import tests, exact media metadata tests, manual-offset tests, and format/user documentation. |
| 20 | Realtime safety | Atomic publication tests, audio/MIDI armed-record no-allocation tests, processor no-allocation tests, `tests/no_alloc.rs`, and full native/Wasm suites. Numeric updates do not rebuild topology (`session::processor_latency_updates_do_not_rebuild_graph_topology`). |
| 21 | Boundedness | Checked recipe overflow/capacity test; bounded diagnostics/plots; retained-history, protocol-capacity, browser recording, callback sub-block, and stress tests. |
| 22 | Backend honesty | Fake/dummy deterministic contract; real JACK ranges; CPAL/midir unknown/manual policy; Web Audio known/unknown property tests; Web MIDI next-quantum contract; unsupported Carla unknown/manual fallback. |
| 23 | Transactional safety | Session metadata rejection, replacement/consolidation, driver switch, processor restore, provider refresh, grab preflight, and backend session-I/O transactional tests. |
| 24 | Documentation and diagnostics | `docs/source/usage.latency_compensation.rst`, `docs/latency_diagnostics.md`, the format/provider docs, bounded backend snapshot diagnostics, latency panel warnings, and four inspected UI captures. |
| 25 | Full validation | Native, Wasm Node/Chromium, packaged Chromium/Firefox, JACK, Carla, formatting, warning, tracing, and policy gates listed below. |
| 26 | Automated loop-action coverage | `src/rust/shoop_engine/tests/latency_characterization.rs` directly asserts raw, logical, dispatch, and audible frames for ordinary play, record, grab, planned preplay, dry-through-wet, and dry-into-wet, with audio/MIDI, component modes, mandatory frame values, callback sizes, wrapping, transitions, and native/Wasm execution. |

## Staged-plan checklist

The 14 implementation stages map to these concrete surfaces. Their individual implementation and verification bullets remain enumerated in `latency_comp.md`; this table prevents a checked stage from being treated as evidence without the corresponding artifacts.

| Stage | Required outcome | Primary implementation and verification surface |
|---|---|---|
| 0 | Characterized baseline and deterministic fixtures | `docs/latency_design_evidence.md`; `tests/latency_characterization.rs`; delayed processor fixture; JACK/Carla/OxiSynth characterization tests. |
| 1 | Shared checked policy domain | `src/rust/shoop_latency/src/lib.rs` and its 11 native/Wasm tests. |
| 2 | Processor/port contracts and atomic publication | `latency_runtime.rs`, processor/port/state-mirror modules, publication and topology-stability tests. |
| 3 | Raw capture, frozen alignment, retention/finalization | Audio/MIDI channel implementations and postroll, prerecord, safe-prefix, ordering, and no-allocation tests. |
| 4 | Dry render-ahead and dry/wet modes | Audio/MIDI loop and session ordering, planned/immediate transition, callback/wrap, and canonical-write tests. |
| 5 | Grab/replacement semantics | Port history, audio/MIDI grab matrix, transactional replacement, consolidation, and provenance tests. |
| 6 | Backend/application policy integration | Backend trait/native/fake/Web Audio implementations, app intents/state, policy contracts and reconciliation tests. |
| 7 | JACK observation/propagation | JACK callback route slots and retirement in `app_backend.rs`; dedicated real-JACK tests and documented physical procedure. |
| 8 | Carla provider | Versioned adapter patch/runtime lock, in-process/subprocess protocol, graph-path aggregation, real Carla tests. |
| 9 | OxiSynth provider | Characterized `0..=63` phase range and exhaustive event/callback tests in `oxisynth.rs`. |
| 10 | CPAL/browser/protocol capabilities | Backend capability APIs, protocol/worklet/client transport, browser latency publication/restart tests, Web MIDI contract. |
| 11 | Persistence/resampling/I/O | Session v7 document/archive/media/resample modules and direct round-trip, corruption, replay, import/export, clone tests. |
| 12 | Settings/UI | Settings registry/migration, latency panel/details markers, app API state/intents, unit tests and inspected captures. |
| 13 | Diagnostics/hardening | Bounded counters/plots, tracing inventory, stress/capacity/overflow tests, `docs/latency_diagnostics.md`. |
| 14 | Documentation and end-to-end validation | User/provider/format docs, this audit, `docs/latency_validation_runs.md`, and all final gates below. |

## Named documentation and compatibility surfaces

The required user and contract documentation exists in:

- `docs/source/usage.latency_compensation.rst`, linked from `docs/source/usage.rst`;
- `docs/source/usage.loopcontrols.rst`;
- `docs/session_format_v1.md`, `docs/settings_format_v1.md`, and `docs/port_model.md`;
- `docs/web_midi_contract.md`, `src/rust/shoopdaloop/README.md`, and `docs/latency_diagnostics.md`;
- `third_party/carla/README.md`, `runtime-lock.json`, and `shoop-latency-adapter.patch`;
- `docs/latency_design_evidence.md` and `docs/latency_validation_runs.md`.

Session v6 migration, exact-media compatibility, protocol version rejection, Carla runtime version fallback, and malformed/overflow rejection all have direct tests rather than documentation-only claims.

## Final validation record

Selected environment: repository `nix develop`, UTC 2026-08-23.

- `cargo fmt --all -- --check`: passed.
- `RUSTFLAGS="-D warnings" cargo build --workspace`: passed.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`: passed, 1616 passed and 2 declared skips.
- `python3 scripts/check_shoop_test_usage.py`: passed.
- `python3 scripts/check_tracing_coverage.py --require-closed`: passed.
- `cargo build --target wasm32-unknown-unknown -p shoopdaloop --no-default-features`: passed. (`native-fx` is intentionally a native default.)
- `cargo build --target wasm32-unknown-unknown -p shoop_audio_worklet`: passed.
- Complete shared Wasm suite in pinned Node 22.23.2: 17 packages, 1336 tests, zero failures.
- Complete shared Wasm suite in pinned Chromium/ChromeDriver 147.0.7727.137: 17 packages, 1336 tests, zero failures.
- Packaged hosted and self-contained Chromium output-only AudioWorklet smokes: passed with genuine 128-frame callback progress.
- Packaged Firefox AudioWorklet smoke: passed with 36 callbacks/4608 frames, 128-frame quantum, zero overflows, and clean teardown. Firefox 150.0.1 emitted a geckodriver 0.36.0 recommendation warning but the smoke itself passed.
- Browser harness parser and three-invocation smoke-budget checks: passed.
- Dedicated JACK2 dummy-server run without the missing-backend allowance: 9/9 passed. No physical device endpoints were present, so cable/converter loopback remains the documented facility skip rather than a claimed physical result.
- Pinned real Carla: zero/nonzero Rack, branched Patchbay, Patchbay16, and subprocess worker scenarios passed; queried ranges matched impulse frames.
- Manual-equivalent deterministic matrix: passed at 44.1/48 kHz and 64/127 callback sizes. UI usability captures for Direct, External, Carla, and Built-in Synth were inspected as recorded in `docs/latency_validation_runs.md`.
- `git diff --check`: passed; final diff was reviewed for generated artifacts, accidental fixture churn, and unrelated formatting.

The aggregate suites are not used as substitutes for the mappings above: each immutable criterion and each staged requirement has an implementation/test/documentation surface, while the final commands verify cross-feature and cross-target integration.
