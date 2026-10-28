# Milestone 1: integration notes and evidence

## Status and isolation

Milestone implementation is in progress under `PLAN_MILESTONE_1.md`. Stages 0–4
are verified against the initial pin: external composition, independent production
DSP, native service and complete public stereo/MIDI/media scenario. Native
debug/release, real Node tests and closed workspace gates pass. Shared metadata
additions were approved. Latency findings, further adversarial/measurement work,
final integration and full feature parity remain pending.
Draft PR: [#889](https://github.com/SanderVocke/shoopdaloop/pull/889).

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
| Capture preparation | Two banks/groups, two members per group, 32,768 staging bytes per group |
| MIDI indexing | Eight prepared versions, 32 checkpoints/version, 128 replay events |
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
  streams. Applications poll/dispose owning returns off-render. The service's
  render-side facade controller may move public owners into pre-reserved return
  storage, but must not destroy them or retain its private cancellation flags in
  the UI stream.
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
  the spike; Stage 2 records the production component's native/Node lifecycle and
  boundary tests. Full service-level lifecycle validation remains Stages 3–6.
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
crates' tests and commit history. The user subsequently approved the metadata and
future necessary edits, with explicit care for the concurrent finishing work.
Check shared files against both committed and working concurrent changes before
editing. Preserve protected loop/runtime ownership and use additive changes where
possible; this approval does not turn an unfinished upstream snapshot into a final
integration revision.

## Stage 2: independent production DSP

Implemented under `src/rust/shoop_example_dsp/` with only `rt_runtime` as a
production dependency and `#![forbid(unsafe_code)]`:

- Prepared generation-checked preset bank, bounded pre-touched delay storage,
  native typed client/render endpoints and component-instance-checked tickets.
- Per-channel live/playback coefficients, stereo source exchange, fixed-delay
  mode and exact-frame whole-preset activation with revision/reset semantics.
- Stable two-input MIDI fan-in, separate normal/recovery credits, truthful refusal
  details and preserved actual acknowledgements after a later processor failure.
- Atomic peak-since-publication-read mirrors, independent UI polling, and explicit
  NaN/infinity telemetry handling without changing the signal.

Fifteen guarded tests pass in native debug, native release **and actual Node**.
They check complete sample/absolute-packet oracles across callback partitions,
physical D = 3 delay and reset history, zero/callback-end frames, stale generations/
revisions, foreign component tickets, held-flag `Busy`, recovery under full normal
credit, disconnection, before/after-publication faults and capacity paths. MIDI
output `Full` is handled according to the existing runtime contract: continuing
audio, overflow diagnostics, ordinary-output suppression and an explicitly drained
separate recovery sink. The new fixture checks all 32 independent expected
all-sound-off/all-notes-off packets; no runtime/loop code or test was changed.

Observed Stage 2 gates:

| Gate | Result |
| --- | --- |
| `cargo test --locked -p shoop_example_dsp` with warnings denied | 15 passed |
| Same package release tests with warnings denied | 15 passed |
| Warning-denied `wasm32-unknown-unknown` library build | Passed |
| Existing Wasm runner, dev/Node, selected new package | 15 executed, zero failed |
| Warning-denied root workspace build | Passed |
| Root nextest suite, app backend feature, CI profile | 2,156 passed; four existing skips; 106 binaries |
| Formatting and test attribute policy | Passed |
| Closed tracing inventory | Passed: 212 modules; five new portable-library entries only |
| New-package Wasm dependency policy | Passed; production tree is `rt_runtime -> rtrb` |

The Nix shell currently provides Node 22.23.2, while the existing runner requires
22.22.2. Its first invocation refused before running any test. An already installed
Nix-store Node 22.22.2 was then selected **inside** the project development shell;
the unmodified runner validated that version and executed all 15 tests:

```sh
nix develop --command bash -c '
  export PATH="/nix/store/6s9qrnf23whlvphz6p5fkczfhpaj0zya-nodejs-22.22.2/bin:$PATH"
  export CARGO_BUILD_JOBS=2
  python3 scripts/run_wasm_tests.py --profile dev --runtime node --package shoop_example_dsp
'
```

That store path is local evidence, not a portable installation instruction. CI
uses its provisioned pinned tool. No version gate, runner or flake was modified.
Local logs: `target/milestone-1/dsp-{gates,node,workspace-gates,metadata-gates}.log`.
The earlier workspace-gates log ends in the then-missing five inventory rows;
metadata-gates records the subsequent approved registration and successful closed
check, without waiving that gate. Build output
now uses an isolated ignored directory on disk,
`/home/sander/dev/shoopdaloop/target/milestone-1-engine-service`, symlinked from this
worktree's `target`; the shared checkout's source and other build namespaces are
unchanged. This avoids exhausting `/tmp`'s RAM-backed filesystem.

The approved tracing rows are only for the new DSP's `control.rs`, `lib.rs`,
`meters.rs`, `processor.rs` and `types.rs`. Like the portable runtime/loop library,
this independent low-level component has no application tracing dependency; its
receipts and meters expose structured outcomes. Existing inventory rows and
checker logic remain unchanged. The existing Wasm dependency CI invocation gains
only `--package shoop_example_dsp`; portable test discovery already executed all
15 tests through package metadata. Before editing, both shared files and repository
guidance were unchanged from the dependency base in the concurrent checkout,
whose loop-host finishing commits had advanced to `8242de59`. None of that ongoing
work was imported or modified. Continue actual service/loop integration in Stage 3.

## Stage 3: native service and endpoints

The production service wrapper is implemented using only the pinned public
loop APIs. Its render owner keeps the loop controller, with bounded native
frontend queues, independently reserved owning/domain returns and a separate
render-owned disconnect-stop reservation. Public completion owners are moved to
its UI pipe while the controller's private prepared cancellation flag remains
local; holding a previous UI recovery result must not pin that local recovery
flag. A bounded request/inner-sequence table correlates outcomes; it does not
schedule musical time. The existing resolver/scheduler still owns execution.

The first new-wrapper tests correctly refused its underprovisioned preparation.
The earlier provisional 8,192-byte capture group cannot hold the requested 2,048
MIDI event headers: the public capture-member format needs `events * 12 + bytes`,
plus the stereo audio block. At maximum 64 frames this is
`2048 * 12 + 512 + 64 * 2 * 4 = 25,600` bytes per group. The service now prepares
32,768 bytes/group, retaining two members (interleaved stereo audio plus MIDI)
and two banks. This corrects only new service storage, not a dependency defect or
acceptance change. Runtime host MIDI ports remain 2,048 events/8,192 bytes; loop
role packet storage is separately bounded at 512 bytes and is not advertised as
unlimited MIDI throughput.

The full independent sample oracle also checked the actual transition outcomes and
caught a first-record `Capacity` refusal. Interval reservation preflights the full
2,048-event bound against the MIDI index's 128-event replay interval: one initial
checkpoint plus 16 possible additions requires at least 17 checkpoints. The
service now prepares 32/version rather than the provisional eight. This is a
second service-configuration correction; no sample/packet expectation or protected
implementation is changed to obtain a pass.

### Observed service verification

`src/rust/shoop_engine_service/tests/service.rs` contains nine guarded portable
cases. The initial graph processes frame zero without a warm-up callback. The
record/play/preset case checks every stereo sample and all 133 absolute MIDI
packets across whole, single-frame, irregular and zero-frame partitions. MIDI
expectations include the public restoration contract: canonical center pitch and
released CC64/69/66 on all 16 channels, plus the initially held note, at each play/
wrap entry. They do not discard restoration packets to compare only notes.

The route/prefix case stages a prepared graph, activates it at an interior cut,
and changes capture from post-DSP to live input without restarting the take.
A same-frame query delivered before activation observes graph epoch 2 and unchanged
topology epoch 0. It reads the retained stereo prefix through an owning buffer after
further recording and checks all raw interleaved bytes. Initial graph epoch 1 and
resolver epoch 0 are distinct: a draft test supplied topology epoch 1 and correctly
received `Timeline(InvalidState)` before that test precondition was corrected.
An additional rejection case checks wrong graph epoch returns `StaleEpoch` with
the candidate owner, not a successful activation.

Other cases verify normal-credit refusal, retained-ticket `Busy`, foreign route/
ticket identity, cancellation before forwarding, requested/delivered/actual late
loop-query frames, zero/end boundaries, invariant complete streams with state
queries and zero/one/64 meter polls, stalled owning/semantic recovery returns,
disconnect stop, and processor faults before/after actual DSP publication. Faults
retain real prior receipts, explicitly abort unfinished service work, do not replay
the callback, and recover all resource pages/versions/snapshots with zero media
references after stopped extraction. Zero calls
report zero work rather than recycling the preceding report's counters.

Observed commands in `nix develop`, with two build/test workers and warnings denied:

```sh
cargo test --locked -p shoop_engine_service
cargo test --locked --release -p shoop_engine_service
cargo build --locked -p shoop_engine_service --target wasm32-unknown-unknown
python3 scripts/run_wasm_tests.py --profile dev --runtime node --package shoop_engine_service
cargo fmt --all -- --check
cargo build --locked --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci --test-threads 2
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/check_wasm_test_dependencies.py \
  --package shoop_example_dsp --package shoop_engine_service
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
```

Native debug **9 passed**, native release **9 passed**, and actual pinned Node
**9 executed / zero failed**. The Wasm build passed separately. The local Node
selection is the same preinstalled pinned 22.22.2 tool described for Stage 2; no
runner/version check was changed. Workspace nextest passed **2,165 tests, four
existing skips, 108 binaries**. Closed tracing passed with **217 modules**;
formatting, warning-denied workspace build, test policy, both new packages' Wasm
dependency policy and Linux/Windows/macOS/Wasm dependency-boundary checks passed.
These are native tests and target/dependency checks, not Windows/macOS execution,
a new AudioWorklet application or device evidence. Logs:
`target/milestone-1/service-gates.log` and
`target/milestone-1/service-workspace-gates.log`.

Only five new service inventory rows and its new-package argument were added to
shared metadata. The concurrent checkout had advanced to `dbe6d623`, with workflow
finishing changes including the loop package in the same Wasm-check invocation.
Those changes were inspected but not imported piecemeal; this isolated patch adds
only the service argument alongside its earlier DSP argument, preserving existing
rows/checkers/gates. Final integration must retain both upstream and milestone
additions. All protected paths remain identical to the initial dependency pin.
The [service README](../../src/rust/shoop_engine_service/README.md) describes public
hosting, capacities and ownership. At Stage 3 only empty-loop preparation was
implemented; Stage 4 below adds the required startup-import/full-MIDI-prefix case.

## Stage 4: startup imports and complete public scenario

`prepare_with_content` takes owned planar stereo, ordered `InitialMidi` packets
and a strictly-pre-first-packet `MidiState`. Off-render loading uses public resource
uploads, interleaved little-endian audio, canonical MIDI records and indexed state.
Its finite resource-accounting loop runs before any engine callback/admission; it
is not a warm-up `Engine::process` or a way to renew rendering quotas. Snapshot
leases retain the imported versions; mutable startup resource handles are removed
before ownership transfers to the public loop facade. Two new startup tests prove
actual frame-zero stereo/MIDI, shape/packet-capacity refusals, multi-page loading
and recovery of all page/version/snapshot/MIDI-index/media/export/pin pools.

The [public example](../../src/rust/shoop_engine_service/examples/one_track.rs)
shares its public-API scenario with two portable integration tests. A separate
stopped-input case first proves that live note 60 observed while Stopped appears
in the recording's initial state: all eight stereo frames/133 MIDI packets, its
metadata and ninth-frame cleanup agree. That service is reclaimed before starting
the main imported-content case; it is not a preparation callback for that case.

The main scenario has these physical engine-frame cuts:

| Frame | Operation/evidence |
| --- | --- |
| 0 | Play imported four-frame revision zero and retain its export; first stereo output is `[-9,110]`, not silence. |
| 1 | Record a distinct revision with imported-playback cleanup and live MIDI state. |
| 2 | Retire initial residency; its export retains the original through later role replacement. |
| 4 | Publish post-DSP → live capture routing and a coherent query; retain a three-frame live prefix. |
| 5 | Typed preset activation reports actual frame 5/revision 2. |
| 5–9 | Read/reread unchanged stereo/MIDI prefix and all 16 initial-state channels while recording appends. Also read original imported media. |
| 9–10 | Release prefix, reject double release and stale cursor; capture continues. |
| 12 | Finish the eleven-frame take and immediately play its frame zero at an interior callback cut. Same-frame live CC precedes playback packets. |
| 23,25,33 | Wrap, seek to position 3, then wrap again; full packet restoration/order and samples agree. |
| 35 | Select the retained original after initial residency/old playback ownership were removed; a same-frame query sees the settled selection. |
| 40–42 | Stop with exact note cleanup, release exports/displaced graph, and extract/reclaim. |

Independent scalar tables/formulas and explicit full packet sequences check every
sample and all **487 absolute MIDI packets**. Expected resource bytes are manually
constructed, not copied from runtime journals/encoders. All **378 copied media
bytes** agree: prefix stereo/MIDI twice, original imports and completed take.
Initial-state expectations come from the explicit golden pre-record packets;
all channel values and held-note velocities are checked through the immutable
lease. Metadata is the channel/key projection, not an external synthesizer snapshot.
Peaks are `[980,110]`. Pool checks include 512 free pages, 32 versions/snapshots,
eight MIDI-index versions, eight media/pin slots, four exports, zero references and
zero reserved/retired storage after idempotent stopped cleanup.

Whole-per-phase, single-frame and irregular/zero partitions run with both valid
query declaration orders. Required control-side barriers learn graph/lease/cursor
IDs and preserve on-time declarations; “whole” does not falsely mean a 42-frame
callback that somehow observes results mid-callback. Every nonempty callback
still calls one engine. The prepared input table, adaptation, ingress, processing
and publication are heap/free guarded; UI journaling/owner disposal/printing stay
off-render. Normal completion remains distinct from admission.

Observed warning-denied commands in the Nix development environment:

```sh
cargo test --locked -p shoop_engine_service
cargo test --locked --release -p shoop_engine_service
cargo run --locked -p shoop_engine_service --example one_track
cargo run --locked --release -p shoop_engine_service --example one_track
cargo build --locked -p shoop_engine_service --target wasm32-unknown-unknown
python3 scripts/run_wasm_tests.py --profile dev --runtime node --package shoop_engine_service
```

Debug/release each passed **13 tests** and all six main executable runs, reporting
`frames=42 packets=487 copied_media_bytes=378`; callback counts were 11/42/30 for
whole/single/irregular phases. Actual pinned Node executed **13, zero failed**;
Wasm library build passed separately. In **both** native profiles, each
`-- --inject=sample`, `-- --inject=packet`, and `-- --inject=outcome` produced the
intended assertion panic/exit 101 after a successful build. These six negative
checks are failures by design, not passed musical scenarios.

The same workspace gates listed for Stage 3 passed again: warning-denied build,
formatting, test attributes, closed inventory (**217 modules**), new-package Wasm
dependency policy, and both four-target dependency boundaries. Nextest passed
**2,169 tests, four existing skips, 110 binaries**. Final-state logs:
`target/milestone-1/one-track-gates.log`,
`target/milestone-1/one-track-workspace-gates.log`, and
`target/milestone-1/one-track-inject-{debug,release}-{sample,packet,outcome}.log`.
No protected source, fixture, runner, plan or evidence path is changed. No new
shared-metadata edit was needed. These results do not cover the forthcoming latency
probe/measurements or final agreed-upstream integration.

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

Stages 0–4 are verified against the initial pin, including approved additive
metadata, 13 portable service tests, the complete public scenario and closed
workspace gates. Stages 5–7 must complete latency-probe findings, further adversarial
lifecycle, two-thread execution and comparable raw measurements. Final integration
still waits for the agreed completed loop revision and final-head CI/review.
No complete feature-parity, latency compensation or callback-deadline claim is made.
