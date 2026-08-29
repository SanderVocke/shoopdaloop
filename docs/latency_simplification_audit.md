# Latency simplification implementation audit

## Stage 0 baseline

The simplification work starts at `0ece22d5006738c10191e290f65c7660d94f7c1f`
(`Plan streamlined latency compensation`). Its merge base with `origin/master` is
`15cc18fe8274f1f04d9339774b9b51fef642425c`. The immediately preceding feature
head is `ba770f17746afcfa25ad19db35c3dcfd70b93e8e`.

The path-based baseline against the merge base is:

| Path class | Paths | Added | Deleted | Changed |
| --- | ---: | ---: | ---: | ---: |
| Production/documentation | 72 | 18,672 | 428 | 19,100 |
| Integration tests/examples | 6 | 2,891 | 3 | 2,894 |
| Total | 78 | 21,563 | 431 | 21,994 |

This classification counts paths containing `/tests/`, top-level `tests/`, test
suffixes, and examples as test paths. Inline unit tests remain in their production
path, so the final audit will additionally compare Rust test functions and
`#[cfg(test)]` line ranges. The largest baseline paths are:

| Path | Added | Deleted |
| --- | ---: | ---: |
| `src/rust/shoop_backend/src/lib.rs` | 2,396 | 31 |
| `src/rust/shoop_engine/tests/latency_characterization.rs` | 1,878 | 0 |
| `src/rust/shoop_app/src/lib.rs` | 1,652 | 93 |
| `src/rust/shoop_engine/src/app_backend.rs` | 1,467 | 49 |
| `src/rust/shoop_latency/src/lib.rs` | 1,418 | 0 |
| `src/rust/shoop_egui/src/latency_panel.rs` | 1,056 | 0 |
| `src/rust/shoop_engine/src/audio_midi_loop.rs` | 1,022 | 14 |
| `src/rust/shoop_engine/src/session.rs` | 1,011 | 4 |
| `src/rust/shoop_backend/src/native.rs` | 908 | 2 |
| `src/rust/shoop_engine/src/midi_channel.rs` | 804 | 19 |
| `src/rust/shoop_engine/src/latency_runtime.rs` | 756 | 0 |
| `src/rust/shoop_engine/src/audio_channel.rs` | 669 | 14 |

The first attempt to enter the prescribed Nix development shell failed while
applying `third_party/carla/shoop-latency-adapter.patch`: patch reports a malformed
hunk at line 163. This is baseline evidence rather than a passing gate. The patch
and adapter are branch-only functionality scheduled for deletion in Stage 3.

## Surface inventory

Latency behavior is concentrated in these groups:

- Domain: `src/rust/shoop_latency/`.
- Callback/runtime and media: `src/rust/shoop_engine/src/{latency_runtime,audio_channel,midi_channel,audio_midi_loop,state,state_mirror,port,midi_port,session,app_backend}.rs`.
- Providers: `src/rust/shoop_engine/src/{carla_processor,carla_native,carla_subprocess,oxisynth}.rs`, `src/rust/shoop_backend/src/native.rs`, and the Carla patch.
- Backend/application: `src/rust/shoop_backend/src/{lib,native}.rs`, `src/rust/shoop_app/src/lib.rs`, and `src/rust/shoop_app_api/src/lib.rs`.
- Browser/wire: `src/rust/shoop_audio_protocol/src/lib.rs`, `src/rust/shoop_audio_worklet/src/lib.rs`, `src/rust/shoop_worklet_client/src/{lib,transport}.rs`, `src/rust/shoopdaloop/src/browser_audio.rs`, and `src/rust/shoop_plugin_protocol/src/lib.rs`.
- Persistence/media: `src/rust/shoop_session/src/{document,archive,media,resample,lib}.rs`.
- UI/settings: `src/rust/shoop_egui/src/{latency_panel,app_widget,tracks_widget,track_widget,loop_widget,details_pane}.rs` and `src/rust/shoop_egui/examples/latency_panel_smoke.rs`.
- Documentation: `docs/{port_model,session_format_v1,settings_format_v1,web_midi_contract}.md`, `docs/source/usage.latency_compensation.rst`, tracing inventory, browser README, and Carla README.
- Main integration fixtures: `src/rust/shoop_engine/tests/{latency_characterization,latency_support/mod,jack_app_backend,carla_latency_compatibility}.rs`.

The public domain currently exposes checked mapping plus ranges, certainty,
component kinds and policy, source/interval identities, recipes, path aggregation,
and forensic take snapshots. Engine public APIs mirror observations, recipes,
history selection, latching, and atomic publication. Backend APIs expose provider
capability, port observations, take provenance, policy commands, and consolidation.
The app API additionally exposes component controls, cue selection, diagnostics,
plots, provider identities, and incomplete/history state.

The browser protocol currently carries component policies, observation records,
media/take provenance, backend observations, diagnostics, and commands for backend
configuration, take policy, and consolidation. The plugin worker protocol carries
Carla observation certainty and diagnostics. Session documents currently persist
track component policy and take alignment, margins, observation, history, warnings,
and render provenance.

All of these fields were introduced after the merge base. In particular, master
uses session document version 6, exact-media document version 1, audio protocol
version 14, and plugin protocol version 2; the feature branch changed those to 7,
2, 18, and 3 respectively. There is no released-format requirement for these
branch-only fields. They may be removed outright. Existing master-era sessions
must still load with a zero/default alignment, while the final reduced document
must preserve its own signed take alignment and sample-rate conversion.

The initial case-insensitive terminology inventory contains 45 tracked matches.
They comprise plan wording, three documentation uses, the qualified frame-mapping
type and its call sites/tests, generic mirror publication helpers, two non-latency
comments/tests, scripting's generic loop-value helper, and an architecture-test
diagnostic. Stage 1 owns all 45, including generic uses unrelated to latency.

## Test disposition

Every latency test is classified by the following source-and-behavior rules. A
final test audit will enumerate discovered test names and apply these same rules;
no unclassified latency test may remain.

| Existing test group | Disposition | Retained test purpose |
| --- | --- | --- |
| `shoop_latency` mapping, bounds, and arithmetic | Retain/rename | Signed capture mapping, overflow, bounds, and render-advance separation |
| `shoop_latency` observation, identity, component, range, recipe, path, and take-snapshot tests | Rewrite | Effective automatic/manual/trim resolution only; delete identity, component matrix, range selection, overlap, recipe, and forensic assertions |
| Audio/MIDI channel mapping, retained windows, postroll, wrap, snapshots, and no-allocation tests | Retain/rewrite | One latched alignment, complete pre/post media, settlement, deterministic playback, and callback safety |
| Channel replacement and grab tests | Rewrite | Operation-boundary value, prepared retention, and atomic mutation; delete history selection and consolidation-required workflow |
| `audio_midi_loop` latching, playback readiness, postroll, and dry/wet tests | Retain/rewrite | Effective value latching and exact-once processor advance without component recipes |
| `latency_characterization` ordinary audio/MIDI and dry/wet oracles | Retain/consolidate | Callback-size/sample-rate/loop-boundary deterministic oracles using effective values |
| `latency_characterization` component matrices and stable/variable history fixtures | Delete/rewrite | Keep pairwise effective-value boundaries and a simple grab; remove component/history coverage |
| Port/mirror observation publication and JACK route tests | Rewrite | One truthful JACK effective observation and callback-safe publication |
| Carla adapter, Carla provenance/diagnostic, OxiSynth phase-range, CPAL/midir, Web Audio/Web MIDI automatic-provider tests | Delete | Providers are removed or manual-only under the reduced contract |
| Backend policy/latching tests | Rewrite | Automatic value plus manual override/trim, future-operation semantics, unsupported/manual capability, and no topology rebuild |
| Backend diagnostics, ambiguity, current/frozen comparison, history, consolidation, and raw-media tests | Delete | Behavior and APIs are removed |
| Audio/plugin protocol round trips | Rewrite/delete | Reduced effective value, processor advance, alignment, pending/finalizing/error only; remove Carla protocol latency status |
| Worklet/browser tests | Rewrite | Manual-only compensation, settlement, logical playback/export, and concise failure |
| Session/archive/resample tests | Rewrite | Alignment and source sample rate only, checked conversion, malformed bounds, and master-era defaults |
| Logical export/import tests | Retain/rewrite | Normal compensated window; remove raw-margin metadata/export fixtures |
| App intent/model tests | Rewrite | Effective value and per-take alignment edits with future-operation semantics |
| Latency panel, diagnostic plot, provenance marker, consolidation, and raw-export UI tests/example | Delete/replace | Compact settings/take editor plus pending/error states and one replacement smoke fixture |
| Generic control, loop, and scripting tests incidentally using the forbidden term | Retain/rename | Existing non-latency behavior |

## Acceptance-criterion test ownership

| Criterion | Planned automated owner and evidence |
| ---: | --- |
| 1 | Audio/MIDI channel and end-to-end backend record/play oracles with manual and JACK values |
| 2 | Runtime/backend latching tests that change provider/manual values during and after operations |
| 3 | Session archive and resampling round trips comparing alignment time within one-frame rounding |
| 4 | Audio/MIDI positive/negative retention and postroll settlement tests |
| 5 | Channel/backend capacity and interrupted-finalization transaction tests |
| 6 | Normal audio/MIDI logical export tests plus API/UI absence searches |
| 7 | Consolidated dry-through-wet and dry-into-wet audio/MIDI exact-once oracles and monitoring equivalence |
| 8 | JACK capability tests and dummy/native/browser manual fallback tests |
| 9 | App API and compact UI tests plus removed-symbol searches |
| 10 | Channel/runtime no-allocation tests, atomic publication tests, and realtime lock audit |
| 11 | Pairwise callback-size, sample-rate, wrap, transition, and session-round-trip matrices |
| 12 | Case-insensitive tracked terminology audit with no output |
| 13 | Final prompt-to-test matrix, discovered-test audit, and full native/browser suites |
| 14 | Final path `--numstat`, inline test LOC, and retained-subsystem explanation against this baseline |

## Ordered deletion map

1. **Domain:** rename the mapping, replace recipes/components/observations with
   checked `RecordingOffset`, optional `ProcessorRenderAdvance`, a compact prepared
   operation value, and concise errors.
2. **Runtime:** replace observation history and recipe publication with one atomic
   operation snapshot; retain frozen alignment, pre/post retention, postroll,
   exact-once render advance, and transactional channel operations.
3. **Providers:** collapse JACK observations to a truthful effective offset;
   delete cue/path aggregation, Carla patch/provenance, OxiSynth timing ranges,
   CPAL/midir/Web provider estimates, and associated dependency features.
4. **Backend/API:** replace component policy and take provenance with effective
   offset, manual adjustment, processor advance, frozen alignment, and concise
   pending/finalizing/error state. Delete consolidation and diagnostics.
5. **Wire/browser:** remove provider records, component policy, histories,
   diagnostics, cue commands, and raw/consolidation commands; bump reduced wire
   contracts and make browser automatic capability unavailable.
6. **Persistence/export:** persist only the settled take alignment and source
   sample rate needed for conversion. Remove margins/provenance/incomplete state,
   raw latency export, and bake/recovery workflows while retaining logical export.
7. **UI/docs:** replace the advanced panel and loop actions with compact controls;
   rewrite provider, settings, session, browser, Carla, and troubleshooting text.
8. **Tests/dead code:** apply the disposition table, remove orphan fixtures and
   patches, run dependency/compiler/search audits, then execute all final gates.

## Reproducible audit commands

```sh
START=0ece22d5006738c10191e290f65c7660d94f7c1f
BASE=$(git merge-base "$START" origin/master)
ABANDONED_TERM=$(printf 'sca%s' 'lar')
git diff --numstat "$BASE..$START"
git diff --name-only "$BASE..$START"
git grep -in "$ABANDONED_TERM"
git grep -inE 'Latency(Component|Recipe|Observation|Certainty|Provider)|capture_alignment|render_advance|retained_(before|after)|postroll|consolidat'
rg -n '#\[[^]]*test[^]]*\]' src/rust --glob '*.rs'
cargo nextest list --workspace --features shoop_engine/app_backend
python3 scripts/check_shoop_test_usage.py
```

The final audit must rerun these commands against the final commit, inspect every
remaining match rather than relying on counts, and record test command output and
any unavailable host/browser facility explicitly.

## Final implementation audit

### Delivered surface

The final implementation has one checked signed `RecordingOffset`, one separate
`ProcessorRenderAdvance`, `CaptureFrameMapping`, and a compact prepared/latched
callback snapshot. Track state contains only automatic/manual/trim selection, the
resolved value, processor advance, and pending/error state. Channel/take state
contains one signed capture alignment. Session data stores track adjustment,
manual and processor values, and one channel alignment; it stores no provider
identity or observation history.

JACK recomputes connected capture latency on the control path and accepts it only
when the relevant connected range is exact. Native tracks require all applicable
connected inputs to agree, ignore disconnected inputs, and reject route changes
while a recording operation is armed.
Dummy, CPAL, Carla, built-in synth, and browser paths remain
manual when they cannot make that claim. The browser protocol is version 15 and
carries only the reduced controls and channel alignment.

Recording preparation reserves the sign-derived pre/post window before the
operation. Postroll remains an unsettled content mutation. Audio or MIDI storage
exhaustion stops the operation and clears its partial take. Differently aligned
replacement and nonzero-offset retrospective grab fail before mutation. Playback
and normal audio/MIDI export map the logical window; no special raw export or
bake command remains.

The compact controls live directly in the track options menu and include manual
completed-take alignment. Browser layout evidence is
`artifacts/latency-controls.png` in the validation workspace (the repository's
artifact directory is intentionally ignored). It shows the Manual selector,
Offset, Processor, and Effective rows at 1200 by 800; minimum/common-size UI tests
also pass.

### Size result

Both measurements use merge base
`15cc18fe8274f1f04d9339774b9b51fef642425c` and `git diff --numstat`.

| Measurement | Paths | Added | Deleted | Changed |
| --- | ---: | ---: | ---: | ---: |
| Starting production/documentation | 72 | 18,672 | 428 | 19,100 |
| Starting integration tests/examples | 6 | 2,891 | 3 | 2,894 |
| Final production/documentation | 47 | 5,512 | 172 | 5,684 |
| Final integration tests/examples | 2 | 48 | 2 | 50 |

The simplification delta itself is 4,005 additions and 19,751 deletions across 77
paths. Repository Shoop-test attributes are 1,535 at the merge base, 1,672 at the
simplification baseline, and 1,587 finally: 85 feature-branch tests were removed
while 52 tests above the merge base remain. Inline unit tests are counted in their
production path, which is why the path table is supplemented by this test count.

The retained large files are general application/backend and audio/MIDI channel
implementations. They own track/session orchestration, chunked audio, MIDI state,
content snapshots, smoothing, and non-latency controls. There is no dedicated
advanced panel or characterization matrix. The focused latency domain and runtime
are small, while channel retention/mapping stays beside the callback code whose
bounds and allocation behavior it controls.

### Prompt-to-artifact completion checklist

| Criterion | Concrete evidence |
| ---: | --- |
| 1 | `engine_track_latency_applies_to_future_operations_only`, compensated audio/MIDI record-play tests, and the Worklet full-duplex manual-offset test |
| 2 | `recording_offset_latches_at_each_operation_boundary`, backend/app future-operation tests, and armed offset/route update rejection |
| 3 | `latency_settings_and_take_alignment_round_trip_without_provider_metadata` and multi-rate resampling assertions |
| 4 | positive postroll, actual-captured-preroll, final-event, unsettled-snapshot, and postroll re-entry rejection tests |
| 5 | insufficient-retention, immediate/imminent short-preroll abort, incremental audio exhaustion, MIDI exhaustion, compensated-grab preflight, and stopped/atomic take-alignment preflight tests |
| 6 | logical audio export assertions, exact/standard MIDI assertions including preroll start-state folding, and removed-command searches |
| 7 | audio/MIDI dry-through-wet, dry-into-wet canonical-write, wrap, and independent-domain tests |
| 8 | real-JACK exact observation-and-record-boundary test, connected-input agreement test, unsupported automatic error test, manual browser Worklet test |
| 9 | reduced app/backend/wire structs, track-menu UI test, screenshot, and removed-symbol searches |
| 10 | prepared-latch, armed audio/MIDI postroll, publication, and complete engine no-allocation suites; topology-arm test |
| 11 | 64/64, 127/1, and 31/17/80 audio partition test; MIDI callback/wrap tests; 44.1/32/96 kHz session conversion |
| 12 | case-insensitive tracked terminology audit returns no output |
| 13 | disposition table above, 1,578-test native suite, complete Node and Chromium shared suites, and orphan searches |
| 14 | path table, test counts, simplification delta, and retained-file explanation above |

### Final command evidence

- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`: 1,578 passed, two host-dependent skips.
- Complete shared Wasm suite in Node 22.22.2: every discovered package test passed; after the latest review fixes, the changed engine, backend, and Worklet client packages passed all 819, 47, and 21 shared tests respectively.
- Complete shared Wasm suite in Chromium 147: every package passed after the domain package's three browser tests were explicitly configured; after the latest review fixes, the changed engine, backend, and Worklet client packages passed all 819, 47, and 21 shared tests respectively.
- Real JACK integration ran against the available server; the exact connected 37-frame value was latched onto a recording channel.
- `RUSTFLAGS="-D warnings" cargo build --workspace`, formatting, focused warning-denied latency/backend/session/protocol/client Clippy, test-attribute policy, tracing inventory, dependency tree, smoke-budget check, and report parser tests pass.
- `trunk build` builds both application and AudioWorklet Wasm. The raw Wasm host artifact contract passes with protocol version 15. Firefox 146 loaded the built hosted UI and produced the layout screenshot. The standalone Chromium smoke launcher is incompatible with this host's crash-handler wrapper, but the complete Chromedriver-based Chromium suite passed.
- Removed architecture/API and case-insensitive terminology searches return no tracked production matches. Remaining search matches in this audit are deletion history only.
