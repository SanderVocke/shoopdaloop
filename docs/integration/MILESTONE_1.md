# Milestone 1: integration notes and evidence

## Status and isolation

Milestone implementation is in progress under `PLAN_MILESTONE_1.md`. No service,
DSP component or feature-parity result is claimed by this initial baseline note.

- Worktree: `/tmp/shoopdaloop-milestone-1`.
- Branch: `feat/milestone-1-engine-service`.
- Initial dependency revision: `d038c208f46c98ac8d7561ddcf09ff7c8e7a9d94`.
- That revision was the checked-out and remote `feat/loop-execution-layer` head
  when isolated. The final completed upstream revision remains a delivery gate.
- The shared checkout remains on the loop executor's branch. Its dirty loop
  documentation and host-scenario files were not copied, edited or staged here.
- Only the untracked milestone plan was copied into the new worktree; the
  architecture, transport proposal and metaplan are already in the pinned commit.

The first attempt to create a sibling worktree under `/home/sander/dev` failed
because that location is read-only in the execution harness. Creating the worktree
under `/tmp` succeeded; this is not a dependency-test failure. Keep this worktree
until the branch is delivered, or explicitly relocate it through Git if needed.

## Baseline verification

Executed in the project Nix development environment, with `CARGO_BUILD_JOBS=2`
and `RUST_TEST_THREADS=2` to limit contention with the other executor:

```sh
RUSTFLAGS="-D warnings" cargo test --locked -p rt_runtime -p shoop_loop_engine
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
```

Results on the initial dependency revision:

- **341 passed, zero failed, zero ignored** across the two package test runs;
  doctest invocations also completed without failure.
- Runtime and loop production/build dependency checks passed for Linux, Windows,
  macOS and `wasm32-unknown-unknown`. These are dependency-boundary checks, not
  execution/build results for all those platforms.
- Toolchain: `rustc 1.97.1 (8bab26f4f 2026-07-14)` and
  `cargo 1.97.1 (c980f4866 2026-06-30)`.
- Local command output: `target/milestone-1/baseline.log`. This generated log is
  not committed and is not evidence for any later source revision.

## Inspected public composition seams

The loop facade's public `Engine`, `Controller`, prepared effect bindings,
resource/read commands, parameters, state/export operations and graph-staging
entry points are available on this baseline. The public `coordinated_loop`
example establishes an existing separate playback/mixer/capture graph to build
upon, not a new milestone result.

The typed-control limitation in the plan is confirmed:

- `GraphRuntime<P,L>` owns one planner type.
- Processor operations/results use `L::Operation` and `L::Result`.
- The loop facade fixes that planner to `LoopPlanner` and keeps its underlying
  runtime/controller private.
- The loop node wrapper delegates effect processing and parameters, but has no
  forwarding override for independently typed processor operations.

A service-level request enum does not by itself provide a dispatch path into the
owned effect. Do not edit those protected APIs to make a spike convenient.

One public mechanism worth testing is the existing generic ownership transport
for a component-native prepared control bridge. Before choosing it, the spike
must establish callback-wide admission snapshots, exact frame application within
loop-selected intervals, independent reliable completion retention and recovery.
It must use the supplied interval frame rather than create another musical clock,
must not renew quotas at every loop interval, and must not become a replacement
generic event scheduler. No choice has yet been approved as the Stage 1 solution.

## Existing references inspected

- `src/rust/shoop_backend/src/lib.rs`: current application-facing `Backend` trait,
  including topology, latency, MIDI, content access and session operations.
- `src/rust/shoop_loop_engine/examples/coordinated_loop.rs`: prepared startup,
  record/play/export and quiescent teardown.
- `src/rust/shoop_engine/tests/control.rs`:
  `a_graph_built_through_the_api_records_and_plays`.
- `src/rust/shoop_engine/src/audio_channel.rs`:
  `compensated_record_then_play_maps_postroll_across_callbacks_and_wrap` and
  `snapshot_stays_unsettled_through_latency_postroll_then_publishes_complete_take`.
- `src/rust/shoop_engine/src/midi_channel.rs`:
  `compensated_midi_record_then_play_maps_postroll_across_callbacks_and_wrap`.
- `src/rust/shoop_latency/src/lib.rs`: checked recording offsets, processor render
  advance and retained-window calculations. Reuse depends on the chosen contract;
  these utilities do not implement missing loop capture lifecycle behaviour.

## Remaining work

Stage 1 must define the slice's finite capacities and timing/ownership contracts,
complete the parity map, run a real external typed-control spike, and then either
select a conforming public composition path or pause for an approved prerequisite.
All musical scenarios, portable new tests, latency-probe results, performance
measurements, final upstream integration and CI/review delivery remain pending.
