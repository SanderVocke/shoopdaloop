# Scalar latency completion audit

This audit maps the immutable acceptance criteria in `SIMPLIFY.md` to direct implementation and verification surfaces. Aggregate green suites are integration evidence, not substitutes for these mappings.

## Acceptance criteria

| # | Criterion | Direct evidence |
|---:|---|---|
| 1 | One scalar alignment and no region model | `ScalarFrameMapping`; audio/MIDI channel scalar tests; `forbidden_alignment_region_symbols_are_absent`; repository source/serialization search. |
| 2 | Immediate monitoring | `current_monitoring_is_sample_identical_across_callback_sizes`; `dry_render_lookahead_does_not_retime_live_monitoring`. |
| 3 | Deterministic capture alignment | `record_then_play_matrix_matches_raw_and_logical_audio_midi_oracles` and `ordinary_compensated_playback_matrix_matches_exact_audio_midi_oracle`. |
| 4 | Deterministic wet alignment | deterministic component matrix and `delayed_processor_emerges_on_dry_through_wet_transition_frame`. |
| 5 | Dry render-ahead | planned render matrix; dry-through-wet start/steady/wrap/stop/restart test; audio/MIDI render-wrap tests. |
| 6 | No wet double compensation | dry-into-wet component matrix, canonical channel-write tests, and delayed dry-into-wet session test. |
| 7 | Independent policy control | all component toggle/mode/range/trim tests, cue semantics, backend policy contract, and latency panel policy normalization. |
| 8 | Frozen takes | frozen snapshot revision test, operation-boundary latch test, backend provider-change diagnostic test, and application reconciliation test. |
| 9 | Complete bounded windows | audio/MIDI prerecord/postroll tests, play-after-record defer tests, insufficient-grab preflight, and callback no-allocation tests. |
| 10 | Scalar grab semantics | stable audio/MIDI and variable newest-revision characterization tests; bounded observation-history tests. |
| 11 | Scalar replacement semantics | compatible channel replacement tests, session preflight, and backend pre-mode rejection test. |
| 12 | Atomic consolidation | engine, app-backend, and native mixed audio/MIDI tests; application cache-invalidation regression. |
| 13 | Provider honesty | real JACK 9-test suite; Carla adapter/branched nonzero/compatibility/worker tests; OxiSynth phase tests; CPAL/Web fallback contract tests. |
| 14 | Persistence and resampling | scalar document validation/migration, same/cross-rate replay, collapsed-range certainty, archive transactionality. |
| 15 | Explicit I/O | logical/raw audio and MIDI tests, standard unknown-default import, manual offset including zero-duration media. |
| 16 | Realtime safety | fixed-capacity publications/history/JACK routes; bounded contention fallback; engine allocation/lock/tracing suites; topology-stable numeric update test. |
| 17 | Transactional safety | session I/O contracts, replacement/consolidation preflight, atomic loop command tests, malformed archive and driver-switch rollback tests. |
| 18 | Cross-target validation | native lower-layer/workspace suites, Node and Chromium shared Wasm suites, real JACK and Carla runs; packaged browser smoke gates are recorded in final validation. |

## Prompt-to-artifact checklist

- Stage 0 revisions, PR findings, baseline commands, helper/provider inventory: `latency_stage0_inventory.md` and the two CSV inventories.
- All 134 reference-added tests plus the three added regressions: `latency_reference_test_inventory.csv`; current source-name verifier reports no missing required names and no omitted piecewise test present.
- Shared scalar semantics and realtime audit: `latency_stage12_audit.md`.
- User behavior and troubleshooting: `source/usage.latency_compensation.rst` and `latency_diagnostics.md`.
- Persistence/settings/ports/Web MIDI/worklet/Carla contracts: `session_format_v1.md`, `settings_format_v1.md`, `port_model.md`, `web_midi_contract.md`, the application README, and `third_party/carla/README.md`.
- Provider measurements and facility limits: `latency_design_evidence.md`.
- Final commands and facility-dependent runs: recorded below when the final gate run is complete.

## Reference comparison

The ordinary scalar audio/MIDI frame oracles were transferred from the pre-piecewise reference implementation and still run at mandatory callback/loop boundary values. The deliberate differences from reference head `279308a6` are:

- a revision-spanning grab selects the newest fully available observation once for the complete channel and persists `variable` plus revision count; it creates no segments;
- replacement with a different resolved scalar is rejected before mode/content mutation and requires consolidation;
- export, playback, persistence, resampling, and consolidation use one monotonic scalar mapping, so region precedence, shadowing, nonmonotonic ordering, and interval-capacity behavior do not exist.

## Final validation record

Pending the complete Stage 13 gate run. Facility-dependent results must name the actual server/runtime/browser used and must not be inferred from portable tests.
