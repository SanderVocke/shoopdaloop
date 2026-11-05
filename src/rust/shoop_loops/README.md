# shoop_loops

Loop processing uses `runtime::LoopPlanner`, including sync/program settlement,
media ownership and grouped recording at quantum boundaries.

This library is unconditionally `no_std` with `alloc`; it has no `std` feature.
It works in ordinary `std` applications using their allocator and panic handling.
Bare-metal hosts must provide both and support pointer-width and 32-bit atomics.
Preparation still allocates; rendering retains its prepared-storage contract.


Portable loop timing and audio/MIDI execution on `rt_runtime`. A single render-owned
resolver supplies musical decisions; prepared playback/capture consumers execute
those decisions in the signal graph. The loop planner coordinates media ownership,
capture preflight, typed outcomes, settled state/export and explicit stopped cleanup.

The library supplies parts, not a runtime: `LoopPlanner::prepare` returns the planner,
its playback/capture nodes and a control-side `LoopClient`. The application installs
them in its own `GraphRuntime` next to its other processors and owns the resource
store, control endpoint and lifecycle. The planner refuses generic graph changes
that would drop or replace its nodes.

Start with the [runtime guide](docs/runtime.md). For generic graph/processors,
transport and resources, use the [runtime book](../rt_runtime/docs/README.md).
These libraries do not supply devices, UI, a production adapter, unrestricted live
processor construction or hard-deadline certification.

## Task guides

| Task | Guide |
|---|---|
| Prepare and install the parts, submit/cancel work, export and stop | [Runtime](docs/runtime.md) |
| Execute audio conversion, gain and discontinuity smoothing | [Audio](docs/audio.md) |
| Render/index/track/capture original packets | [MIDI](docs/midi.md) |
| Retain input history and publish private shared-media edits on fixed ticks | [Shared history](docs/shared-history.md) |
| Emit cleanup → projected restoration → original events | [MIDI restoration](docs/midi-restore.md) |
| Retain media and publish logical-owner batches | [Media ownership](docs/media.md) |
| Borrow flat/sparse compiled timing metadata | [Topology](docs/topology.md) |
| Ingest LTP1 with finite callback-shared work/recycle | [Topology staging](docs/topology-staging.md) |
| Inspect an unpublished control/clock cut | [Program boundary probes](docs/program-boundaries.md) |
| Own a program candidate until stopped/real zero | [Program changes](docs/program-changes.md) |
| Publish timing, routes, stopped generations or a running program/graph pair | [Activation paths](docs/topology-activation.md) |

## Offline examples

```sh
cargo run --locked -p shoop_loops --example coordinated_loop
cargo run --locked -p shoop_loops --example nested_timing
cargo run --locked -p shoop_loops --example configuration_cut
cargo run --locked -p shoop_loops --example program_graph_cut
```

Each example assembles its own runtime from the parts. Tests use
the dev-only `shoop_loops_test_support` crate instead, which is not part of the API.

Add `--release` for the optimized profile. `coordinated_loop` asserts stereo/MIDI
record → playback → FX → capture, wraps, seek, replacement, immutable export and
cleanup; it has no allocator guard of its own. The three focused examples guard
every callback and compare relevant callback partitions/zero boundaries:

- `nested_timing`: timing-only outer/inner/child control, direct Stop precedence and
  delivered countdown cancellation. Logical recording is not captured audio/MIDI.
- `configuration_cut`: already-stopped generation/media/timing/route publication,
  running/same-frame-Stop refusal and retained old versus newly scaled audio.
- `program_graph_cut`: pending until actual zero, then paired epochs and exact
  scalar route output with a continuing child. Its node pairs are timing-only.

Preparation and owning-return destruction stay off-render. Regression models and
historical native/Wasm/interpreter evidence have separate source/platform scopes;
these examples are not new host, benchmark or full acceptance evidence. The
[independent loop fixture](../../../tests/loops_host/README.md) adds a small
heap-guarded record/play/FX scenario with complete sample/MIDI comparisons through
direct, bounded wire and two-thread drivers, plus real non-isolated offline/live
AudioWorklets. Its primitive-command bridge retains separate admission/domain
outcomes across independent ordinary Wasm memories. Downstream Web Audio samples,
canonical semantic boundary/decision/event journals, fixed memory, failure canaries
and complete stopped recovery are checked in both profiles. A separate prepared
graph payload receiver has native and Node/Chromium binding coverage.
An opt-in native graph command bridge and public Node/Chromium bindings now
stage/activate/release and recycle owners through bounded results. Actual offline/live
graph worklets now transfer descriptors/commands/returns and check downstream output
against native replay using an explicit audible Ready checkpoint. That test-driver
pause is not continuous wall-clock live-edit/deadline evidence.

## Timing and ownership contracts

The resolver API supports `submit`, retained `completion`/`take_completion`,
`cancel`, `preview_tick`, `commit`, and `finish`. Prepare the resolver with its
immutable `Quantum`; `preview_tick` produces candidate
`decisions`; only `commit` publishes state, terminal outcomes, and semantic
events. `finish()` completes exactly that whole quantum before another submission or boundary.
A failed or invalidated preview cannot be committed or expose valid decisions.
`Decision::playback_discontinuity` is an authoritative, transactional media restart
flag, independent of optional journal storage. Same-position seeks and explicit
replacements can set it without changing visible state; final non-playing states
clear it. Use this flag rather than diagnostic events to drive renderer smoothing
and MIDI cleanup/restoration.

Pending requests and unread terminal records share fixed reserved slots.
Automatic wraps and sync pulses do not consume those slots. Journal capacity
is optional: overflow increments a saturating diagnostic without changing
musical execution. Clock edges coalesce per loop/frame; a prepared sync
component terminates by visiting each seeded member once, not by a wave quota.

`replace_topology` checks the expected epoch and generation high-water marks,
revalidates pending requests, and returns the displaced owner. Removed loops
must already be stopped. Changed running programs are safely refused by that
immediate replacement API. The standalone [`stage_program`](docs/program-changes.md)
path retains one prepared program candidate, selects a zero/stopped cut, preflights
without publication, and swaps owners only on commit. It preserves logical
identities, sync/trigger metadata and other programs. The planner's `ProgramActivate`
connects it to receiver banks, reliable runtime receipts and capture preflight;
ProgramGraphActivate pairs it with prepared routing. Fixed node-pair GraphActivate,
immediate TopologyGraphActivate and already-stopped ConfigurationActivate are
separate interfaces. All retain prepared physical shapes; logical generation edits
are not running physical node-pair replacement. `quiesce()` permanently
ends resolver execution so faulted/stopped teardown can collect owners without
finishing an unrendered interval. Dispose returned/rejected owners off-render.

[`probe_program_boundary`](docs/program-boundaries.md) inspects a composite's
unpublished control/clock cut before its old child actions are emitted. It reuses
normal arbitration and cursor advancement without publishing state/receipts or
creating a committable interval. Its zero-entry predicate distinguishes an actual
entry from a cursor stalled at zero. This is not candidate activation authority;
the separate owning transaction must recheck and publish its own candidate.

Composites execute sequential/parallel/delayed/repeated prepared entries and
finite nesting. Regular programs repeat; explicit-mode programs execute one
pass. `PlayIteration` selects a starting iteration and `SeekIteration` selects
an authoritative snapshot without replaying preceding events. A non-retriggering
seek repositions already-playing descendants without inventing new clock edges;
newly activated playback/recording still has its normal start edge.

Composite cursors report iteration, completed passes, entry frame and repeat-wait
state. `nominal_unit_frames` is the prepared frame duration of each iteration;
source changes never retime a running arrangement. Position advances on rendered
frames, including without a source. `last_pulse` records the composite's own start
or repeating pass edge, not every internal iteration or incoming source pulse.
`Resolver::set_repeat_sync` / `Operation::RepeatSync` configure repeat alignment
independently of the source used by delayed commands. Aligned primitives and
regular composites wait only at their own end while the source plays; its next
edge restarts them. An inactive source releases the wait. Explicit programs stop
after one pass. See the [composite contract](../../../docs/native_session_recovery/composites.md).

Capture preflight can supply `CaptureUnavailable { target, reason }` to refuse a
primitive start/restart without inventing an external request or start pulse.
An existing take's exhaustion is independent (`CaptureExhausted`); both may be
latched at one boundary. For atomic request-origin families use `Refused`, which
has precedence regardless of observation order. Target-only refusal does not
reject a parent composite or its other children.

The standalone resolver models logical capture positions and boundary failures;
it does not own captured samples. The loop planner supplies the media ledger,
prepared group lifecycle, runtime receipts and independent recovery-lane admission.
Do not mix standalone resolver mutation with a running planner.

The resolver owns authoritative timing; separate render operations execute
supplied interval decisions without their own playback clock. Native and browser hosts use the same semantics;
transport, devices, UI, and AudioWorklet bindings stay outside this crate.

## Adversarial regression models

```sh
cargo test --locked -p shoop_loops --test adversarial_timeline
cargo test --locked -p rt_runtime --test adversarial_delivery --test adversarial_capture
```

Add `--release` for the optimized profile. The timeline target compares seeded
programs and sync components with independent scalar/event expectations while
request credits stay full and optional journals overflow. The runtime targets
check typed payload/receipt ownership and retained capture/index content across
partitions, refusals, stale/reused identities and exhausted maintenance credits.
Actual mutations are heap-guarded; allocating references and destruction stay
off-render. These are regression models, not fresh worklet, deadline or interpreter
certification. The [validation record](../../../VALIDATION.md#loop-evidence)
identifies actual native/Wasm results and outstanding source-scoped interpreter
coverage.

The implementation contract and remaining work are tracked in
[PLAN.md](../../../PLAN.md#portable-loop-execution) and
[ARCHITECTURE.md](../../../ARCHITECTURE.md#loop-semantics).
[Earlier plans and evidence narratives](../../../docs/history/README.md) are
historical. No full legacy backend compatibility, unlimited recording, or
hard-real-time deadline guarantee is promised.
