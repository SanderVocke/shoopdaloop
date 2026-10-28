# Milestone 1: integration notes and evidence

## Status and isolation

Milestone implementation is in progress under `PLAN_MILESTONE_1.md`. The Stage 1
external composition spike is verified; production DSP/service implementation and
feature-parity acceptance remain pending. Draft PR: [#889](https://github.com/SanderVocke/shoopdaloop/pull/889).

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

## Slice contract after the Stage 1 feasibility review

### Preparation and caller boundary

Prepare the loop topology, role bindings, resource pools, imports, graph, DSP preset
bank, optional graph-stage storage and return channels off-render. The first
callback produces the declared graph's audio; no silent preparation callback is
allowed. Native client operations carry Rust values, not encoded bytes. Expose
loop operations, DSP preset selection/recovery, prepared route staging/activation,
state/export/cursor/read/release and explicit shutdown; do not expose arbitrary
mutable component internals.

The first service configuration has the following prepared limits. Construction
validates the whole configuration and reports it in the executable; these are not
promises of sufficient capacity for arbitrary MIDI or application workloads.

| Storage/work class | Initial service limit |
| --- | --- |
| Signal graph | 48 kHz; 64 frames/callback; 4 node slots; 24 ports; 16 edges |
| Host channels | Stereo input/output; one MIDI input/output |
| MIDI storage | 2,048 events and 8,192 bytes per port; 512-byte individual packets |
| Resource pool | 32 slots; 4,096 pages of 256 bytes; no callback growth |
| Capture preparation | Two banks/groups, two members per group, 8,192 staging bytes per group |
| MIDI indexing | Eight prepared versions, eight checkpoints/version, 128 replay events |
| Runtime ingress/scheduling | 16 normal command slots plus its existing recovery slot; 32 scheduled events |
| Runtime callback budgets | 16 commands, 32 events, 128 validation steps, 1,024 optional transfer bytes, 16 publications |
| Loop domain | One loop slot; eight normal requests plus recovery; eight media slots; four exports |
| Native service loop delivery | 16 normal slots; one recovery slot; terminal/owning credits reserved separately before admission |
| DSP component | Four prepared preset slots; one normal and one recovery operation; two dequeues/completions per callback |
| Prepared routes | One candidate slot; fixed playback/effect/capture identities |
| Telemetry | Atomic per-channel peak mirrors, not a reliable request queue |

Service owning-return and loop-execution storage must cover every corresponding
admitted credit, plus one render-owned emergency-stop outcome on disconnect.
Ordinary requests cannot consume that emergency reservation. Reject preparation
rather than weaken these relationships. Runtime command, domain result, DSP,
resource-transfer and optional export capacities remain separate. The minimal
spike uses only two presets and one unused optional telemetry slot; it does not
claim to implement the entire service configuration above.

A retained ticket/result may keep a prepared cancellation flag busy even after
its normal credit returns; that is a truthful `Busy`, not a reason to allocate.

The native API will expose preparation returning a client, one render owner and
precompiled fixed-role route candidates. The client offers typed loop submission,
component-defined preset selection, route staging/activation, queries, export/
cursor/read/release and recovery, with separate owning-completion and domain-
execution polling. The render owner retains the real loop `Controller` so client
disconnect can request loop recovery without requiring a live UI producer. Compile
route plans during preparation before splitting these endpoints; do not borrow a
live render-owned catalog from another thread. A stopped owner performs cleanup
and bounded reclamation only after callbacks cease. Concrete wrapper names may be
refined during implementation without changing these timing/ownership contracts.

### Timing, outcomes and component ownership

- Queue admission reserves a reliable return. It is not semantic execution.
  Preserve the loop facade's separate owning completion and domain execution
  streams. Poll and destroy owners only off-render.
- A DSP `SelectPreparedPreset` names a generation-checked prepared preset, a
  requested absolute engine frame and the expected component revision. Its
  component-defined terminal result must distinguish applied frame/from-to
  revision, malformed/stale/late refusal, cancellation and processing failure.
- Late DSP delivery refuses without applying; it does not silently clamp. An
  operation at callback end belongs to the following nonempty callback. A
  zero-frame callback applies no sample-bound mutation. Transport tickets retain
  their documented best-effort-before-dequeue cancellation; explicit recovery
  must also cancel already-delivered future component work.
- Loop commands settle according to the existing resolver before rendering their
  interval. A preset at F changes DSP output beginning at sample F within the
  supplied interval, even if the resolver did not need an interval cut at F.
  Same-frame loop queries describe loop state, not an atomic loop-plus-DSP cut.
- Chosen narrow bridge: a component-owned render control handle snapshots its
  existing typed transport once before the host's single `Engine::process` call. It
  publishes one copy-only prepared directive to its own processor through a fixed
  atomic mailbox; only that processor changes its private DSP state. Afterwards
  the handle reads an acknowledgement and publishes the reliable typed result.
  No shared mutable access to the loop or another processor, extra engine call,
  signal-buffer sharing or general-purpose event scheduler is introduced.
- The directive contains only frame/preset/revision metadata. Preset banks and
  scratch remain processor-owned. The native client has no mailbox access.
  Release/acquire publication and single render-thread ownership are exercised by
  the spike; the involved atomics compile for Wasm. The complete production
  component still needs its Stage 2 portability, lifecycle and boundary tests.
- One normal component operation can remain pending; recovery has an independent
  slot and priority. A stalled result consumer must not prevent an admitted future
  activation, autonomous loop wraps or recovery. No stream of future generic
  actions or second musical clock is accepted by this component.

### Scenario and valid partitions

Use distinct left/right values and stable equal-time MIDI markers. Import an
initial immutable revision off-render. While stopped, observe a held input note;
then record a different revision, retaining that initial state. Export a prefix
before recording finishes, obtain audio/MIDI cursors and initial MIDI metadata,
read it while more input is appended, and reread the same export unchanged.

Prepare a fixed-role graph candidate without interrupting the initial audio, then
activate it at a future declared frame while capture continues. The candidate
changes the capture source from post-DSP to live input while leaving the same
playback, DSP and capture node identities present. Finish recording and begin
playback at an interior frame, wrap, seek, select the retained original import and
stop with the required note cleanup. Independently construct every expected sample,
absolute packet, revision and cut; a journal/hash-only comparison is insufficient.

All compared callback partitions must deliver commands and prepared candidates
before their requested frames and must provide the off-render polling barriers
needed to learn returned export/stage IDs. Within those phases compare whole,
single-frame and irregular subdivisions, with commands declared before processing.
Do not compare a late, interactive command in one partition with an on-time command
in another and call the discrepancy musical partition dependence.

### Queries, meters, exports and teardown

Loop queries return settled pre-render `Cut` values with actual graph/topology
epochs. Peak meters use nonnegative finite sample magnitudes encoded in atomics:
render publishes a channel maximum, control reads/resets that channel atomically.
A racing peak belongs to this or the next read, never both; separate channels do
not form one coherent transaction. No reliable meter request is submitted.

An export owns an immutable prefix and its initial MIDI metadata. The caller keeps
its lease alive through every outstanding owning-buffer read, observes EOF/cursor
progress, then releases it. Optional retention/copy refusal is reported without
stopping capture. Stop/cancel is available independently of ordinary admission.
Continue callbacks long enough to publish cleanup and drain MIDI recovery before
quiescent extraction; only then dispose/reclaim off-render. On disconnection, use
the declared recovery path and retain undeliverable owners until stopped teardown.
If processing faults after publishing some state, preserve that actual state and
its receipts; never retry the failed callback or pretend it never happened.

### Fixed-delay experiment declared before implementation

Use D = 3 frames, equal-channel impulses/ramps and timestamped MIDI markers. For
an isolated input x, the physical wet signal is y[t] = x[t-D] with zero prepared
history for t < D. Choose a logical recording interval [4,12), length eight. The
uncompensated raw capture in that interval must remain separately visible; the
proposed aligned wet window is [7,15), requiring physical tail through frame 15.
Use a probe graph with dry input on one capture channel and delayed wet output on
the other so both physical streams remain observable in one loop graph.

The negative-offset case uses recording offset -2: its proposed dry window is
[2,10), and with D = 3 the proposed wet window is [5,13). This requires dry
prehistory before logical start 4 and one wet tail frame after logical finish 12.
Compare current-engine positive/postroll and negative/history
references only for matched initial conditions. The probe may report unsupported
logical windows, continuation-after-finish or window-selection APIs; it must not
trim, shift timestamps or substitute an oracle output for physical captured data.
Production compensation is not part of this milestone.

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

### Composition decision and external proof

Choose the component-native prepared control bridge for this bounded slice. Its
host handle uses the existing public `rt_runtime::transport` once per callback;
its DSP alone applies one prepared configuration at the supplied interval frame.
The client cannot touch the copy-only directive/acknowledgement mailbox or the
processor's private state. The in-flight operation stays in the existing transport
rather than a new event queue or generic scheduler. No protected prerequisite is
needed for this narrow path.

Alternatives inspected:

- Direct independently typed `Processor::operation` dispatch is unavailable through
  the protected loop node wrapper. A wrapper enum alone cannot fix dispatch.
- Scalar parameters work for parameters, but are not a meaningful typed prepared-
  configuration transaction with the required result and recovery semantics.
- Replacing the planner or constructing a separate runtime would duplicate the
  authoritative resolver/ownership facade; nested engines or extra callback calls
  would also violate the required timing/budget boundary.

The reproducible standalone workspace is
[`experiments/milestone_1_composition`](../../experiments/milestone_1_composition/README.md).
Its DSP crate imports only `rt_runtime`. Its host uses public loop APIs and the
repository's shared test macro and heap instrumentation, not private fixtures.
The alternative preset changes prepared live/playback mixing coefficients and
stereo routing together, not a float-encoded selector.

Observed results with warnings denied:

- Five tests pass in native debug **and** release. The interior activation test
  covers five host partitions, including zero-frame calls and callback-end F.
- `SelectPreparedPreset` applies at frame 5, inside the resolver's [4,7) playback
  interval, reports the actual frame and revision 1 -> 2, and changes full stereo
  sample expectations beginning at that frame.
- A stalled normal result prevents another normal admission, but not later loop
  wraps. A recovery request cancels a dequeued future operation with normal credit
  full. Pre-dequeue cancellation also has a typed terminal result.
- Invalid preset generations/slots, stale revisions and late requests refuse
  without changing output. Component disconnect retains undeliverable owners until
  off-render disposal. Capture pools/media references recover after quiescence.
- Every host begin/render/finish path in these tests is guarded against allocations
  **and freeing**, and the fixture checks processor disposal is not on render.
- The independent DSP library builds for `wasm32-unknown-unknown`. This is not a
  Node execution result; production portable test registration remains Stage 2.
- Root formatting, test-attribute and tracing checks pass. The warning-denied
  root workspace build passes. Complete nextest validation of the unchanged root
  workspace passes: **2,141 passed, four skipped**, across 104 binaries. This does
  not count the standalone spike as automatically discovered workspace tests.

Commands (inside the repository Nix environment):

```sh
export CARGO_TARGET_DIR="$PWD/target"
RUSTFLAGS="-D warnings" cargo test --locked --manifest-path experiments/milestone_1_composition/Cargo.toml
RUSTFLAGS="-D warnings" cargo test --locked --release --manifest-path experiments/milestone_1_composition/Cargo.toml
RUSTFLAGS="-D warnings" cargo build --locked --manifest-path experiments/milestone_1_composition/Cargo.toml -p shoop_m1_spike_dsp --target wasm32-unknown-unknown
```

The existing root suite was run with:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend --profile ci --test-threads 2
```

Its invocation additionally used `RUSTFLAGS="-D warnings"` and
`CARGO_BUILD_JOBS=2`. The four skips are the existing profile/opt-in exclusions;
no failing test was disabled for this milestone.

Local logs: `target/milestone-1/spike-gates.log`, `workspace-build.log` and
`workspace-nextest.log`. The
initial spike fixture underprovisioned capture groups (one rather than the public
facade's two prepared banks). Only that new fixture was corrected; no dependency
code or expectation was changed to obtain these passes.

### Settled implementation scope

Use the proposed names `shoop_example_dsp` and `shoop_engine_service`. Promote the
successful narrow design, not a copy of the resolver or the private loop node.
The allowed implementation changes are new crate trees, their required root
lockfile entries, these integration documents, `ARCHITECTURE.md` and the milestone
plan. The standalone spike can be removed after its proof is preserved in the new
crates' tests and commit history. No runtime, loop, shared test runner or CI edit is
approved implicitly; coordinate any shared discovery change separately.

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

Stage 1's bounded composition gate is satisfied. Stages 2–7 must implement the
production DSP/service, complete stereo/MIDI/media scenarios, portable Node tests,
latency-probe findings, adversarial lifecycle and measurements. Final integration
still waits for the agreed completed loop revision and final-head CI/review.
No complete feature-parity, latency compensation or callback-deadline claim is made.
