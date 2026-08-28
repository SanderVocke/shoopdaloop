# Scalar latency Stage 0 inventory

This document records the clean baseline and the transfer decisions for the scalar latency implementation. It is an inventory, not evidence that latency compensation is implemented.

## Revisions and branch provenance

| Item | Revision / evidence |
|---|---|
| Updated implementation base | `15cc18fe8274f1f04d9339774b9b51fef642425c` (`origin/master` after fetch) |
| Implementation branch | `feature/scalar-latency-compensation` |
| Scalar plan commit on the implementation branch | `2456a99d` |
| Reference branch commit | `279308a6345858e85859c51c1da1532a5c227f19` |
| Reference PR | [#797, Implement end-to-end latency compensation](https://github.com/SanderVocke/shoopdaloop/pull/797) |
| Reference merge base used for inventory | `a28058146eee5f40f757ad512e54050bfa0f29f0` |

The implementation branch was created directly from the fetched `origin/master`, then the plan-only commit was replayed. `git log a2805814..279308a6` contains 47 reference commits. None is an ancestor of the implementation branch, and no production commit from the reference branch was cherry-picked.

## Baseline verification

Selected environment: repository `nix develop` on NixOS.

| Surface | Command | Result |
|---|---|---|
| Complete native workspace | `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci` | Passed: 1,526 run, 1,526 passed, 2 declared skips. The first attempt found a stale Tracy CMake source path in `target`; `cargo clean -p tracy-client-sys` removed only that generated state, and the clean rerun passed. |
| Complete portable Node Wasm suite | `python3 scripts/run_wasm_tests.py --runtime node --profile dev` | Passed: 16 packages, 1,259 tests, zero failures. The flake supplied Node 22.23.2 while the harness requires 22.22.2, so the run used the checksum-verified official Node 22.22.2 binary without changing repository sources. |
| Forbidden-region baseline search | `rg -i 'AlignmentRegion|alignment_regions|piecewise.*alignment|latency.*region'` excluding the plan and inventories | No matches. |

Local raw logs are retained under ignored `artifacts/latency-stage0/`; Wasm JSON/JUnit reports are under ignored `target/wasm-tests/dev/reports/node/`.

## Test accounting

`latency_reference_test_inventory.csv` compares all Rust tests at the reference merge base and reference head by path and function name. It accounts for all 134 tests introduced by the reference branch:

- 124 transfer with their behavioral intent intact;
- 7 rewrite with scalar fixtures;
- 3 omit because they test the forbidden piecewise model;
- 3 new regressions for cache invalidation, empty imports, and repository/API absence of region data.

The three omitted tests are exactly:

- `piecewise_alignment_regions_select_the_newest_matching_raw_mapping`;
- `piecewise_alignment_regions_select_the_newest_matching_midi_mapping`;
- `piecewise_state_restore_ignores_raw_earlier_logical_future_events`.

The seven scalar rewrites cover logical/raw audio export, logical/raw MIDI export, standard import, mixed consolidation, latency document validation, same/cross-rate replay, and collapsed-range resampling. No transferred test may be ignored, weakened, or changed to accept invented zero observations.

## Helper and fixture inventory

The following dedicated reference harness symbols did not exist at the merge base. All are transferred in Stage 1, with region-oriented assumptions excluded.

| Reference artifact | Symbols / behavior | Scalar disposition |
|---|---|---|
| `src/rust/shoop_engine/tests/latency_support/mod.rs` | `DeterministicTimingConfig`, `checked_signed_total`, `IdentifiedAudioEvent`, `IdentifiedMidiEvent`, `DeterministicDelayedSource`, `TimingObservations`, `DeterministicDelayedProcessor`, `DeterministicActionHarness`, `identified_audio_sample`, `pump_callbacks` | Transfer. Keep independent frame-domain components and checked arithmetic. |
| `latency_characterization.rs` callback helpers | `runtime_render_recipe`, `process_audio_callback`, `process_midi_callback`, `process_midi_synth_callback` | Transfer against scalar API scaffolding. |
| `latency_characterization.rs` action oracles | `dry_through_wet_audio_oracle`, `dry_through_wet_midi_oracle`, `dry_midi_into_wet_audio_oracle`, `dry_into_wet_audio_oracle`, `render_audio_oracle`, `render_midi_oracle`, `record_and_render_audio_oracle`, `record_and_render_midi_oracle` | Transfer; use one raw/logical mapping. |
| `latency_characterization.rs` grab helpers | `latency_grab_fixture`, `midi_latency_grab_fixture` | Rewrite variable grabs to select one newest fully available observation for the complete channel. |
| `latency_characterization.rs` utility helpers | `playback_mode`, `repeated_raw_frames` | Transfer. |
| `carla_processor.rs` deterministic processor fixture | Delayed audio and MIDI over arbitrary callback partitions, exact/ranged/dynamic observation publication | Transfer before provider production code. |
| `latency_panel_smoke.rs` plus four reference screenshots | Direct, External, Carla, and Built-in Synth fixtures | Transfer the fixture after scalar UI state exists; screenshots are validation evidence, not source fixtures to copy as implementation. |
| `third_party/carla/shoop-latency-adapter.patch` | Versioned Carla Rack/Patchbay path-query adapter | Revalidate against the pinned runtime, then port only scalar path observations. |

Inline helpers in the 134 introduced tests remain coupled to their owning rows in the test CSV and receive the same disposition as those tests. Production conversion helpers are not copied wholesale; they are reimplemented only when a stage needs them and must share scalar semantics.

## Provider inventory

| Provider surface in the reference | Evidence to retain | Scalar disposition |
|---|---|---|
| JACK: `app_backend.rs`, `jack_app_backend.rs` | Connected-port ranges, route filtering, callback publication, retirement stress, external send/return callback-period measurement at 64 and 128 frames | Port fixed-capacity callback-safe route snapshots. Keep physical-device latency distinct and unknown without evidence. |
| Carla: `carla_native.rs`, `carla_processor.rs`, `carla_subprocess.rs`, `carla_latency_compatibility.rs`, worker protocol, runtime patch and README | Rack sum, Patchbay path range, Patchbay16, dynamic revisions, subprocess publication, unsupported/version-mismatch fallback | Revalidate the pinned runtime and adapter. Never import region data. Unpatched or mismatched runtimes remain usable with unknown/manual latency. |
| OxiSynth: `oxisynth.rs` | Characterized event-phase range `0..=63`; note/controller offsets and odd callbacks | Port the range on native and Wasm. Do not count SoundFont onset, attack, reverb, or chorus as transport latency. |
| CPAL/midir: `cpal_mock.rs`, driver/backend capability paths | No defensible automatic host value in the reference APIs | Keep unknown/manual unless current APIs provide measured semantics. |
| Dummy/test | Deterministic exact observations and revision changes | Port for contract and end-to-end tests. |
| Web Audio/Web MIDI: audio protocol, worklet, client, browser integration | Supported output properties, unavailable values, coarse Web MIDI timing, restart behavior | Port bounded scalar records; unknown values must not become zero. |

## Protocol and persistence accounting

`latency_reference_field_inventory.csv` inventories every field and enum variant introduced in the reference audio-worklet protocol, Carla worker protocol, session document, and exact-media document, plus the affected version and capacity constants. It has 155 rows. Ten region fields are explicitly omitted; all remaining latency shapes are candidates for scalar porting with checked conversion and bounded payload tests.

Important version decisions are deferred until the scalar shapes settle:

- audio worklet protocol reference change: 14 to 18;
- Carla worker protocol reference change: 2 to 3;
- exact-media document reference change: 1 to 2;
- session document reference change: 6 to 7;
- audio command/event maximum remains 64 KiB but must be reverified with worst-case scalar payloads.

The scalar implementation must not contain `WireAlignmentRegion`, `AlignmentRegionDocument`, an `alignment_regions` field, or an equivalent interval map.

## PR #797 review findings

All 30 review threads at the reference head were inspected. Twenty-eight were resolved on the reference branch and two remained open. The scalar implementation uses the findings as follows.

| # | Finding | Scalar action |
|---:|---|---|
| 1 | Do not bound legacy media offsets as latency | Preserve the full media-layout offset domain; bound only scalar alignment. |
| 2 | Keep resampled range certainty valid | Normalize a collapsed ranged observation truthfully and test archive validation. |
| 3 | Finalize postroll on the smoothed playback path | Share cleanup across smoothed and bypassed finalization. |
| 4 | Preserve take latency in browser media-detail transfers | Carry the complete scalar take snapshot and reject inconsistent chunks. |
| 5 | Bake MIDI channels when consolidating a mixed loop | Consolidate all audio and MIDI channels atomically. |
| 6 | Enforce alignment-region capacity | Omit: no region capacity or region data exists. |
| 7 | Preserve nonempty alignment regions while downsampling | Omit the region behavior; retain the collapsed observation-range regression from finding 2. |
| 8 | Route production grabs through latency-aware adoption | Wire both production backends and audio/MIDI policy. |
| 9 | Preflight compensated replacement before changing mode | Reject differing scalar alignment before mutation. |
| 10 | Restore piecewise regions into the engine | Omit: scalar invariant forbids regions. |
| 11 | Preserve latched observations in frozen-take status | Fall back to the latched recipe for ordinary recordings in every backend. |
| 12 | Reject forward region revisions | Omit region rule; validate scalar observation/provenance relationships transactionally. |
| 13 | Honor piecewise alignment in logical exports | Rewrite export around one shared scalar mapper. |
| 14 | Use piecewise alignment during engine playback | Omit segmented mapping; ordinary playback must use the frozen scalar. |
| 15 | Use active region for MIDI playback validity | Omit region selection; directly test scalar boundary validity. |
| 16 | Exclude shadowed MIDI events from logical exports | Omit shadowing; preserve scalar boundary state and equal-frame order. |
| 17 | Sort remapped MIDI before replacing channel contents | A scalar shift is monotonic, but consolidation still directly verifies ordering and equal-frame stability. |
| 18 | Apply forward precedence in native MIDI consolidation | Omit precedence; native and fake paths share scalar consolidation helpers. |
| 19 | Wait for current MIDI snapshots before baking | Wait for exact settled audio and MIDI snapshots before atomic consolidation. |
| 20 | Preflight every channel before queuing consolidation | Prepare and validate all channels, then commit all or none. |
| 21 | Synchronize take-alignment editor with backend state | Refresh drafts from authoritative state except during active edits. |
| 22 | Preserve unspecified offsets when restoring latency | `None` preserves existing media geometry. |
| 23 | Restore latency on the channel named by each update | Resolve by explicit channel identity, never positional zip order. |
| 24 | Exclude shadowed events from playback state restoration | Omit shadowing; preserve correct scalar boundary-state restoration. |
| 25 | Update all native take channels atomically | Submit one loop-wide prepared command. |
| 26 | Sort logical MIDI events before standard export | Scalar mapping preserves order, but stable ordering remains an explicit export assertion. |
| 27 | Rebuild skipped MIDI state on logical timeline | Omit nonmonotonic regions; scalar MIDI state is derived on the monotonic logical timeline. |
| 28 | Order preroll MIDI before deriving consolidated state | Scalar mapping is monotonic; still derive boundary state in stable logical order. |
| 29 | Invalidate media caches after consolidating latency | Add the unresolved cache-invalidation regression and implementation. |
| 30 | Skip alignment regions for empty imports | Add the unresolved empty-import regression; a scalar offset is valid for empty media and creates no interval metadata. |

The last two findings are the two unresolved review threads at `279308a6` and are mandatory additions in this plan.

## Uncompensated behavior baseline

Current-master tests and reference characterization establish the starting behavior that must not regress accidentally.

| Behavior | Baseline evidence / Stage 1 transfer |
|---|---|
| Immediate audio monitoring | Existing session routing tests; transfer `current_monitoring_is_sample_identical_across_callback_sizes`. |
| Immediate MIDI monitoring and cleanup | Existing MIDI passthrough, state, and JACK fanout tests. |
| Direct/dry/wet record and ordinary playback | Existing `audio_record`, `audio_playback`, `midi_record`, `midi_playback`, processor mode matrix, and wrap tests. |
| Play after record | Existing composite record-pass boundary tests. |
| Planned preplay | Existing `audio_preplay`, `midi_preplay`, and MIDI state edge tests. |
| Dry-through-wet | Existing routing matrix; transfer `current_dry_through_wet_dispatches_without_render_ahead`, which records the current processor lateness before compensated expectations replace it. |
| Dry-into-wet | Existing routing matrix; transfer `current_dry_into_wet_records_the_uncompensated_delayed_return`. |
| Prerecord | Existing audio/MIDI prerecord and MIDI state tests; `start_offset` remains geometry. |
| Grab | Existing ringbuffer adoption and transactional no-allocation tests; transfer callback-partitioned raw-history characterization. |
| Low-level replacement | Existing audio/MIDI replacement, wrap, and no-allocation tests. |

The baseline intentionally records current dry/wet lateness. Those assertions are characterization, not final desired behavior; later compensated tests replace them with exact target-frame oracles.

## Stage 0 gate conclusion

The implementation branch has a clean updated base, passing native and portable baselines, an exhaustive introduced-test inventory, explicit helper/fixture/provider decisions, field-level protocol and persistence accounting, and all PR findings recorded. No reference production commit was wholesale transferred, and current production/API sources contain no region model.
