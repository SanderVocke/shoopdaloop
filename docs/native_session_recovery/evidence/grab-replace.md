# Application Grab and Replacing acceptance

This delivery completes the primitive application behavior specified by
[PLAN_FINISH_GRAB_REPLACE.md](../../../PLAN_FINISH_GRAB_REPLACE.md) and the
[behavior matrix](../grab-replace.md). The validation section below distinguishes
executed local acceptance from final CI. P17 remains the existing composite and
FX/dry-wet interaction dependency; running unrelated composites/processors is
preserved by reconciliation.

## Public application sequence

The packaged native `shoopdaloop --probe-input --session-transport socket` launches
an independent companion with the normal default 16-track/128-loop preparation.
It uses application intents and normal bundled keyboard/public Lua commands:

1. Negotiate Grab/Replacing on normal startup, create stereo and MIDI-only tracks
   and receive stopped input.
2. Grab two selected loops, exercise explicit-list Lua Grab and playback/Stop,
   then bundled Space with persisted default Grab.
3. Adopt an unsynced prefix into Recording with an owned automatic finish. Drop
   the UI and its Lua VM, reattach, and observe the original complete Playing take.
4. Use the normal “Replace input” action and one atomic Lua group to replace two
   sources through several wraps. Inject piano MIDI through monitored input; Stop
   produces complete media with an unchanged logical period.
5. Save, load with fresh IDs/stopped loops, save again and compare every PCM/MIDI
   payload exactly; replace the loaded take again and shut the companion down.

The shared-memory companion runs the complementary editing/file sequence while
retaining its controller, in keeping with the existing no-reattachment boundary.
The hosted Chromium `NEW_ENGINE=1` smoke adds the same input canary to the normal
workflow, including actual UI/Lua routes and a two-second main-thread stall while
the AudioWorklet completes the prefix. It compares exact file payloads, re-edits,
then continues the existing composite, timing, bus and hosted FX/synth workflows.
The harness requires no render memory growth, command overflow or xruns.

## Criterion map

Names are searchable in the linked source/test files. Each group runs through the
normal constructor unless explicitly described as a lower-layer resource proof.

| Contract | Production / independent proof |
| --- | --- |
| Selected gestures, explicit lists, solo and invalid-final-target rollback | `shoop_app/tests/support/input_cases.rs`: `public_grab_selection_exact_lists_solo_replacing_and_reload_share_owned_service_outcomes`; engine `input_batches_preflight_all_media_targets_and_solo_peers_at_one_production_cut` |
| Bundled keyboard/default Grab and Lua; MIDI-only and trigger tracks | `bundled_keyboard_and_lua_use_negotiated_grab_and_replacing_through_the_normal_runtime`, plus packaged native/browser input canaries |
| Target period/phase, apply-N, both post-modes and self-clock | Domain musical/window tests; engine `public_gesture_continues_the_prefix_and_finishes_without_controller_polling`, `public_gesture_uses_the_targets_pending_clock_delay_at_its_effective_cut` |
| Exact requests, receipts, cancellation and detached completion | Engine `public_input_protocol_grabs_replaces_and_retains_terminal_identity_on_normal_preparation`, `public_input_cancellation_survives_disconnect_and_late_refusal_preserves_solo_peers`; adapter `public_grab_and_replacing_have_one_owned_mutation_and_preserve_old_exports` |
| Complete stereo/packet/SysEx/state semantics through erasure/wrap | `continuous_mixed_writer_publishes_scheduled_cuts_and_keeps_retained_exports_exact`, `replacing_maps_each_channel_window_and_preserves_raw_tails_manual_period_and_monitoring`; runtime indexed MIDI independent packet/projection oracles |
| Long ordinary Recording → Replacing and growing prefix → fixed writer | `long_ordinary_recording_prepares_replacement_without_losing_input_during_building`, `prefix_recording_to_replacing_keeps_the_boundary_tick_and_active_exports`; no skipped input while private preparation builds |
| Warmup versus pressure/fault | `public_musical_grab_does_not_pad_density_eviction_as_recorded_history`, source-reset/partial-input cases; incomplete capture is projected through normal snapshots and UI |
| Cancellation/fault cannot be restarted by its automatic finish | `a_failed_or_cancelled_growing_gesture_cannot_be_restarted_by_its_owned_finish`; stop/reset variants wait past the original finish |
| Owner saturation cannot spend live capture/workspace | Allocator-guarded runtime `retained_cuts_cannot_spend_live_capture_credits_without_optional_maintenance`: two sources, packed and fragmented SysEx, immutable exports, finished writers and exact recovery |
| Lifecycle, full pools, zero optional budgets | Runtime required reclamation/tree/packet tests; engine saturated results, source removal/reuse, graph fault, cancellation and shutdown groups |
| Persistence and cache | Production service capture/load/re-edit, adapter immutable exports/revision invalidation and native/browser exact save/load/save |
| Target reconciliation | Role-aware dry/wet PCM/MIDI and recorded settings; shared snapshot API split; full processor graph tests; dependency and DSP-isolation gates |

## Production limits and measured costs

Defaults retain the existing 256 MiB ordinary media budget. The separate thirty-
second history reservation contains 256 MiB audio rings, 128 MiB + 32 KiB shared
user/workspace audio and 16 MiB MIDI payload. Each of sixteen sources has a
2,048-event/512-KiB MIDI quota; physical page fragmentation may evict earlier.
Input admits at most 256 complete events and 2,048 packet bytes per quantum.
Eight writers share bounded storage. The service admits one input transaction
at a time; an independent overlapping transaction receives an explicit Busy.

Global record/category/page admission reserves live history, staging and tree
workspace before retained cuts. Deferred hidden data remains charged until it
retires; finishing writers relinquish future input credits. Required maintenance
survives zero optional transfer budgets. No capacity was silently raised to pass
these tests. Larger shapes exceeding the portable arena refuse before allocation.

The repeatable release examples include whole-host preparation, ordinary loop
metadata, buffers and (for `prepared_memory`) optional processor banks. The FX
measurements compile with `--features builtin-fx,oxisynth`; the callback benchmark
uses direct-track normal engine preparation without hosted processors.

| Prepared shape (16 sources / 128 loops / Q32) | Processor budget | Processor bytes | Complete peak bytes | Preparation ms |
| --- | --- | --- | --- | --- |
| 2 ch / host max 512 | 256 MiB | 198,959,872 | 3,458,138,693 | 1718 |
| 2 ch / host max 128 | 256 MiB | 198,959,872 | 3,128,407,109 | 1650 |
| 8 ch / host max 128 | 512 MiB | 346,440,448 | 4,080,206,525 | 2464 |

All listed prepared callbacks pass the heap guard and teardown recovers all pages.
The eight-channel processor shape explicitly uses a 512-MiB processor budget;
it is refused at the unchanged 256-MiB default. These native allocator measurements
of the 128-frame shape are sizing evidence; the Chromium canary supplies actual
WebAssembly/AudioWorklet execution evidence.

The callback benchmark routes one dense host MIDI stream to sixteen independently
captured sources and keeps two replacement writers active. It retains every sample
of the 32 stopped-input and 128 replacement callbacks; tables include both phases.
The 256-frame cases inject eight events per source **per callback**. The extreme
case injects 256 events per source every Q32 (about 6.1 million source events/s).
Deadlines at 48 kHz are 5.333 ms and 0.667 ms respectively.

| Shape | p50 ms | p99 ms | Max ms | Misses | Max accounted validation steps |
| --- | --- | --- | --- | --- | --- |
| 2 ch / 256 frames / 8 events | 2.633 | 3.800 | 3.869 | 0/160 | 157,518 |
| 8 ch / 256 frames / 8 events | 3.635 | 5.013 | 5.184 | 0/160 | 157,902 |
| 2 ch / 32 frames / 256 events | 17.027 | 46.369 | 47.398 | 160/160 | 4,808,984 |

The table is the corrected-source measurement round at `e03e1bc8` on target
`273853aa`. All four measurement rounds remain in the raw JSON, with source pins.
Earlier loaded rounds include representative deadline misses and an extreme-density
maximum of 175.125 ms; all are retained in full. These runs overlapped other local
tests/builds and packaged application acceptance. Every miss and tail is retained in
[raw measurements](grab-replace-measurements.json); they are not discarded as
outliers. Extreme density is finite and allocator-safe but exceeds the available
wall-clock budget in this environment. This is not physical-device deadline
certification. Functional packet/sample/state, configured work/storage bounds,
no output recovery/overflow and exact credit recovery remain required and pass
independently. The real Chromium workflow separately asserts zero xruns and
render-time memory growth under its admitted workload.

Reproduction from the repository development environment:

```sh
cargo run --release -p shoop_rt_engine --example input_capacity_benchmark -- 2 128 8 256
cargo run --release -p shoop_rt_engine --example input_capacity_benchmark -- 8 128 8 256
cargo run --release -p shoop_rt_engine --example input_capacity_benchmark -- 2 128 256 32
cargo run --release -p shoop_host_processors --features builtin-fx,oxisynth --example prepared_memory -- 2 512 256
cargo run --release -p shoop_host_processors --features builtin-fx,oxisynth --example prepared_memory -- 8 128 512
shoopdaloop --probe-input --session-transport socket
shoopdaloop --probe-input --session-transport shared_memory
```

## Native stack and integration corrections

The default-pool JACK canary exposed a 512-KiB callback-stack overflow during
musical Grab. A debugger backtrace identified the large general-operation
closure retained above the MIDI query. Input transactions and shared operations
now dispatch before that frame, gesture resolution continues without recursive
entry, and MIDI grab returns its existing media value instead of rebuilding it.
The complete default 16-track/128-loop native JACK workflow then passes, including
selected Grab, detached automatic completion, multiple Replacing writers, exact
file reload/re-edit and graceful shutdown. No JACK stack size or MIDI capacity is
increased. The strict isolated JACK CI script now executes that packaged canary.

Whole-period MIDI output reservations also exceeded the portable allocation bound
for large Q1 configurations. Dummy/JACK preflight a smaller render block when
needed while preserving their physical callback period and control lead; Dummy
renders every sub-block. Existing shared-memory/socket recovery coverage passes
with those pools. Host integration tests use one bounded nextest group to avoid
concurrent default-pool preparation and shared JACK graph mutation.

The retained legacy latency fixture now activates its source before connecting
and publishes its physical latency through JACK's recomputation callback. Its
original exact 37-frame assertion passes. Initial failures are not presented as
successful runs: the full 3,094-test run was followed by focused correction
checks, and the final validation pins below record the resulting gates.

The shutdown gate also exposed a late-callback race: reporting Disconnected after
joining JACK did not prove the reset had crossed the graph. Shutdown now waits
for recovery entry, retains reset packets across the bounded drain cycles, and
reports success only after that barrier. All eleven strict driver cases and the
real Master-preview path pass. The Q1 direct recovery/preview fixture explicitly
disables unused optional DSP banks, whose default reservation exceeded its test
configuration; the normal default FX-enabled canaries and DSP bank gates remain.

Independent loop/runtime host fixtures also pass. Their observation codecs now
encode Replacing and input transitions without changing existing primitive replay
bytes; the journal preserves Adopt length, revision, position and post-mode.

## Validation pins and delivery

Implementation reconciliation commit: `e5b87190`, target `78d39677`, source
`d0451596`. Complete Phase A implementation: `b53ed797`. Phase B refresh adds
target `273853aa` (profiling budgets and connection drag identity); the combined
application schema uses version31 because both branches independently used30.
Input RT protocol23, host setup8 and worklet configuration7 are preserved. Local acceptance passes: packaged default socket and shared-memory input canaries;
strict real JACK driver/service/process/application/Master-preview/default input
acceptance; rebuilt Chromium workflow with zero xruns, overflows and render memory
growth; 791 focused contract tests; 60 independent loop-fixture tests and exact offline/two-thread native runtime
replay against the retained real-worklet journal; warning-
denied workspace build, formatting, attributes, tracing, parity and dependency/DSP
isolation. The full 3,094-test run exposed the legacy latency and reset issues
above; both were corrected and passed their focused/strict reruns. Final source/
base CI is the remaining Phase B gate. No pending check is claimed green. Ready-for-review
[PR #940](https://github.com/SanderVocke/shoopdaloop/pull/940) remains unmerged.

The first final-source [Build/Test round](https://github.com/SanderVocke/shoopdaloop/actions/runs/42085353425)
found four independent correction causes: idle live-admission scans exceeded the
existing optional scan bound; the new native canary embedded a bundled Lua script;
the raw JS worklet fixture still encoded the old configuration/protocol; and the
JACK coverage launcher ignored Cargo's target directory. The correction preserves
all original functional and work-budget assertions, uses the external bundled
catalog, updates the fixture layout, and resolves Cargo's actual build output.
Local release measurement covers all twenty Q/partition combinations; 39 focused
shared-engine cases, the raw worklet/profiling contract and five trace-transport
cases pass. The fresh binary also passes the archive's compiled-script rejection
check. Final CI is required for the corrected source before closing P04/P05.

The FreeRTOS firmware adapter retains its existing direct record/play scope;
this native OS/browser application delivery does not add firmware input editing.
It explicitly avoids desktop input-history reservations and negotiates those
capabilities as unavailable. Its existing 32/128-MiB recording, 16/64-MiB media
and 320/768-MiB arena contracts remain unchanged. Preparation now omits processor-
only MIDI banks where processors and external dry/wet are unavailable; all
admitted MIDI restoration and normal input/output capacities remain intact.
Native allocation tests retain room for three simultaneous media owners. Actual
QEMU recovery peaks at 355,649,560 bytes and ZedBoard boot/restart at 248,796,872;
both return Rust heap usage to zero. Fresh final-source firmware CI is required.
