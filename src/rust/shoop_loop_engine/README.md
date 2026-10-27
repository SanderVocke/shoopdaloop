# shoop_loop_engine

Portable loop timing and audio/MIDI execution on `rt_runtime`. A single render-owned
resolver supplies musical decisions; prepared playback/capture consumers execute
those decisions in the signal graph. The runtime facade coordinates media ownership,
capture preflight, typed outcomes, settled state/export and explicit stopped cleanup.

Start with the [runtime guide](docs/runtime.md). For generic graph/processors,
transport and resources, use the [runtime book](../rt_runtime/docs/README.md).
These libraries do not supply devices, UI, a production adapter, unrestricted live
processor construction or hard-deadline certification.

## Task guides

| Task | Guide |
|---|---|
| Prepare/run a facade, submit/cancel work, export and stop | [Runtime](docs/runtime.md) |
| Execute audio conversion, gain and discontinuity smoothing | [Audio](docs/audio.md) |
| Render/index/track/capture original packets | [MIDI](docs/midi.md) |
| Emit cleanup → projected restoration → original events | [MIDI restoration](docs/midi-restore.md) |
| Retain media and publish logical-owner batches | [Media ownership](docs/media.md) |
| Borrow flat/sparse compiled timing metadata | [Topology](docs/topology.md) |
| Ingest LTP1 with finite callback-shared work/recycle | [Topology staging](docs/topology-staging.md) |
| Inspect an unpublished control/clock cut | [Program boundary probes](docs/program-boundaries.md) |
| Own a program candidate until stopped/real zero | [Program changes](docs/program-changes.md) |
| Publish timing, routes, stopped generations or a running program/graph pair | [Activation paths](docs/topology-activation.md) |

## Offline examples

```sh
cargo run --locked -p shoop_loop_engine --example coordinated_loop
cargo run --locked -p shoop_loop_engine --example nested_timing
cargo run --locked -p shoop_loop_engine --example configuration_cut
cargo run --locked -p shoop_loop_engine --example program_graph_cut
```

Add `--release` for the optimized profile. `coordinated_loop` asserts stereo/MIDI
record → playback → FX → capture, wraps, seek, replacement, immutable export and
cleanup; it has no allocator guard of its own. The three focused examples guard
every callback and compare relevant callback partitions/zero boundaries:

- `nested_timing`: timing-only outer/inner/child control, direct Stop precedence and
  delivered countdown cancellation. Logical recording is not captured audio/MIDI.
- `configuration_cut`: already-stopped generation/media/timing/route publication,
  running/same-frame-Stop refusal and retained old versus newly scaled audio.
- `program_graph_cut`: pending until actual zero, then paired epochs and exact
  scalar route output with a continuing child. Its roles are timing-only.

Preparation and owning-return destruction stay off-render. Regression models and
historical native/Wasm/interpreter evidence have separate source/platform scopes;
these examples are not new host, benchmark or full acceptance evidence. The
[independent native fixture](../../../tests/loop_engine_host/README.md) adds a small
heap-guarded offline record/play/FX scenario with complete sample/MIDI comparisons.
It is not yet a two-thread or AudioWorklet host; no loop benchmark is supplied.

## Timing and ownership contracts

The resolver API supports `submit`, retained `completion`/`take_completion`,
`cancel`, `preview`, `commit`, and `finish`. `preview` produces candidate
`decisions`; only `commit` publishes state, terminal outcomes, and semantic
events. Finish the selected interval before another submission or boundary.
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
identities, sync/trigger metadata and other programs. The facade's `ProgramActivate`
connects it to receiver banks, reliable runtime receipts and capture preflight;
ProgramGraphActivate pairs it with prepared routing. Fixed-role GraphActivate,
immediate TopologyGraphActivate and already-stopped ConfigurationActivate are
separate interfaces. All retain prepared physical shapes; logical generation edits
are not running physical-role replacement. `quiesce()` permanently
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

Composite cursors report iteration, passes, and the frame the iteration was
entered. `last_pulse` separately records the last actual clock edge. Nominal
units are duration/seek metadata, never a clock generator; an unsynchronized
program starts at its selected iteration and stalls until explicitly repositioned.
A stopped source can still relay clocks from its own source. Composite position
is not a synthetic uniformly advancing media playhead.

Capture preflight can supply `CaptureUnavailable { target, reason }` to refuse a
primitive start/restart without inventing an external request or start pulse.
An existing take's exhaustion is independent (`CaptureExhausted`); both may be
latched at one boundary. For atomic request-origin families use `Refused`, which
has precedence regardless of observation order. Target-only refusal does not
reject a parent composite or its other children.

The standalone resolver models logical capture positions and boundary failures;
it does not own captured samples. The runtime facade supplies the media ledger,
prepared group lifecycle, runtime receipts and independent recovery-lane admission.
Do not mix standalone resolver mutation with a running facade.

The resolver owns authoritative timing; separate render operations execute
supplied interval decisions without their own playback clock. Native and browser hosts use the same semantics;
transport, devices, UI, and AudioWorklet bindings stay outside this crate.

The implementation contract and progress are tracked in the repository's
`PLAN_LOOP_LAYER.md` and `docs/loop_layer/`. No full legacy backend compatibility,
unlimited recording, or hard-real-time deadline guarantee is promised.
