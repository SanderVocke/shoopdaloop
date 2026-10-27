# Coordinated runtime facade

`runtime::{Engine, Controller, Stopped}` joins the resolver, media ledger,
prepared playback/input consumers and the generic signal graph. It does not use
the old engine, devices, UI or browser APIs. The resolver remains the only
musical clock; the generic runtime owns absolute frames.

Run the standalone public example from the workspace:

```sh
cargo run --locked -p shoop_loop_engine --example coordinated_loop
cargo run --locked -p shoop_loop_engine --release --example coordinated_loop
```

It records stereo and MIDI, finishes at an interior frame, plays immediately,
wraps, seeks, records a distinct revision, selects retained original content,
stops, reads an immutable export including initial held-note state, releases it
and recovers the resource pools. Playback → mixing/gain → capture are distinct
signal nodes, even when playback and capture belong to the same logical loop.
Its callbacks have no allocator guard of their own; guarded integration tests provide separate allocation evidence. It is not a device or AudioWorklet demonstration. The [configuration example](../examples/configuration_cut.rs) separately exercises guarded stopped-owner/media/route publication and retained exports.

## Preparation and ownership

`Engine::<Effect>::prepare` consumes:

- A `PreparedConfig`, compiled `PreparedTopology` and `ResourceStore`.
- `Settings { planner, normal_requests, media_slots, export_slots }`.
- One `Binding` per primitive, imported `media::Input` values, prepared effects
  and a complete `GraphDescription`.

Preparation allocates/pre-touches storage and validates all inputs off-render.
It returns `(Controller<Effect>, Engine<Effect>)` at frame zero, graph epoch one,
without a hidden warm-up callback. Failed preparation consumes/destroys supplied
owners off-render; it does not return a live partial engine.

Each primitive requires distinct playback and capture node identities in the
signal graph. Both receive its immutable decision. `Binding.channels` is the
number of audio ports on each role; `midi` adds one MIDI port. Playback has outputs,
capture has inputs. Capture tracks connected MIDI even while not recording.
Zero-channel/no-MIDI primitives are valid timing-only loops. Composites do not
have media bindings. Effects implement `Processor<LoopPlanner>` and must declare
no planner access; they receive ordinary signal ports and may implement
`set_parameter`. Node identity, port and signal-cycle validation still applies.
The opt-in [publication paths](topology-activation.md) separately support timing/program changes, fixed-role live routing, stopped-owner configuration and paired running-program/graph changes. They operate over prepared physical templates, not unrestricted processor construction or port resizing. Configuration staging also permits unassigned templates whose pairs are absent from the initial graph. Prepare timing/source/role banks off-render, and opt into generic graph staging separately before the first nonempty callback.

Provision two capture groups per media-bearing primitive, with separate resource
identities for their audio and MIDI members. The facade creates these resources
and binds the groups; the supplied store must have enough free capacity. Two
banks allow a repeated Record to reserve a replacement before freezing the old
take. Group staging must fit maximum audio interval bytes plus the conservative
MIDI reservation. MIDI members reserve at least the runtime's 32-event/96-byte
recovery form, independently of smaller ordinary-input bounds. MIDI indexing
must be enabled; index rows must also fit the prepared per-interval bound.
Insufficient future take/append capacity is a boundary refusal, not a reason to
publish half a take.

`normal_requests` must be nonzero and smaller than the resolver's
`pending_requests`: one additional domain slot is reserved for recovery. It
bounds retained domain requests/queries, separately from generic command,
scheduled-event and unread-result storage. Pending media pins have their own
`normal_requests` slots. External exports use a separate `export_slots` pool;
zero export slots is allowed. Metadata slots must cover resident imports,
initial content, retained playback/latest takes, prepared captures, active-prefix
exports and content retained by pending pins/exports. These are finite storage
bounds, not musical transition quotas.

Imports transfer existing snapshot leases into the ledger. Preparation validates
owner, unique logical revision per owner, channels, common duration, exact scoped
snapshot/index metadata and indexed MIDI. It scans imported MIDI packet headers
off-render to check `packet_bytes`; no content-history scan occurs during a seek
or freeze. `packet_bytes` must also cover the configured ordinary capture byte
bound. Do not release transferred leases independently. The underlying imported resource identities remain caller-owned; stopped cleanup releases leases but does not remove those resources.

Revision zero supplies initial content and must match the topology's initial
length. Missing initial content is synthesized only for an empty media loop or
a timing-only loop. All imported revisions reserve the resolver's revision
watermark, including imports not selected initially: later recording cannot
reuse their identities. Logical revisions differ from generic resource revisions,
which advance as capture intervals commit.

`Controller::media()` lists prepared checked identities, not ownership tokens.
An import has a resident reference until `Operation::Retire`; playback/latest,
exports and pending selections retain their own references. Retiring residency
does not invalidate such retained references. A borrowed `MediaId` alone does not
keep anything alive. IDs are instance-scoped and generation checked.

## Commands, receipts and cancellation

`Controller::submit(lane, at, operation)` uses preallocated generic submission flags, unlike allocating `rt_runtime::executor::Control::submit`. Preparing flags does not prepare payloads or make arbitrary owning destruction safe. Build payload Boxes off-render and dispose returned payloads/retired owners there. Full admission and Busy retained ticket/completion flags are separate finite limits; unread terminal results have another limit.

Submission returns a ticket or returns the unchanged owning command on refusal.
Drain both channels:

1. `try_completion()` returns the owning transport command and admission result.
   Scheduled admission is **not** musical success.
2. `try_execution()` returns the reliable typed terminal result. Inspect both the
   generic execution result and `operation_result`: `Reply::Failed` or a failed
   `Reply::Transition` is not success merely because delivery succeeded.

`Operation::Transition` accepts resolver operations except raw `Replace` and `Program` tags. `at` is supplied separately: Timing::At uses that effective boundary; AfterTicks { source, skip } waits for actual source pulses at/after it, skips that many and applies on the next. An idle valid source neither times out nor invents pulses.

| Transition | Musical effect |
|---|---|
| Play { position, retrigger } | Start at a position; already-playing non-retriggering Play is idempotent. |
| Seek { position, retrigger } | Reposition an already-playing primitive; retrigger also resets its cycle count/supplies a start edge. |
| Record { monitor_previous } | Start a new take; optional monitoring reads retained previous content, not the mutable new prefix. Repeated Record preflights the replacement before ending the old take. |
| Stop | Stop playback/freeze the completed capture prefix at the settled boundary; connected input tracking continues. |
| PlayIteration / SeekIteration | Start/reposition a composite snapshot without replaying earlier iterations. |

Play requires nonempty content and a position below its duration; Seek also requires Playing mode. Ordinary refusal is a domain outcome, not callback replay permission.

`Operation::Select` pins checked media and derives replacement length/revision
from its immutable metadata. A pending selection retains its pin even if its
resident reference is retired. Winning decisions select that media; losing,
failed or cancelled requests do not change playback. Pins are released after the
terminal interval finishes. Selecting while recording follows the resolver's
replacement refusal; it does not freeze the current take implicitly.

Four frame identities remain distinct: requested (caller intent), effective (scheduler-admitted frame), delivered (actual operation delivery, optional on failure/cancellation) and terminal (`Execution::actual_frame`). Late delivery retains the original requested frame; `late_by()` measures delivery lateness, not Pending wait. For example requested/effective/delivered/terminal can be 1/10/10/20: delivery is nine frames late, not nineteen. Runtime sequence identities supply ordering.
Resolver completions are forwarded after boundary commit, then recycled only
after finish, when immutable decisions are no longer in use.

`Operation::Cancel { sequence }` distinguishes a queued runtime operation from a
delivered pending resolver request. The cancelled operation receives its own
terminal result; cancellation has a separate acknowledgement. Ticket cancellation is observed before the command's original dequeue, not at arbitrary times before scheduled delivery. Cancelling an already dequeued ticket cannot cancel its typed work; use Operation::Cancel for queued or delivered work. If recovery parks a normal ingress head, its original cancellation observation/progress is preserved rather than sampled again.

The exact recovery-safe set is `Cancel`, `TopologyCancel`, `RoleCancel`, `RoleSourceRelease`, and `Transition { timing: Timing::At, transition: Transition::Stop, .. }`. Submit recovery at/before the current frame, never as a future request. State/export/release, source validation and every activation use normal admission. Recovery must terminate at its delivery frame before any positive interval, not just by callback end. The independent generic recovery
slot and extra domain slot permit recovery with normal requests/results saturated.
Recovery is not an unlimited queue: its own reserved result must be recycled before
reuse. Stalled results prevent further admission, not admitted automatic wraps.
Controller disconnection alone does not invent a musical Stop.

`Operation::Gain` changes the primitive's finite (including negative) gain.
`Controller::parameter` sends ordinary parameters to effect nodes. Neither creates
a new musical clock. Smoothing follows the resolver's discontinuity flag, not
journal events, host cuts, or a gain change by itself.

## Nested control and identity routing

Run [nested_timing](../examples/nested_timing.rs) in debug/release for a small public-API clock/outer/inner/child example. It uses timing-only roles, not audio or MIDI capture: one-frame source pulses drive nesting; seek starts recording at 3, stop freezes a two-frame logical take at 5, seek replays at 6, and direct child Stop defeats inherited Record at 8 without a delayed start at 9. A delivered countdown is cancelled at 10. Whole/single/irregular calls, journal capacity zero, a zero cut and repeated cleanup are checked under heap guards. The larger mixed-media reference remains in `tests/interloop.rs` and its nested support scenario; sufficient-capacity and exhaustion evidence are distinct.

For the separate sufficient-capacity mixed-media scenario, see [`nested_lifecycle.rs`](../tests/support/nested_lifecycle.rs): D plays at 2, records at 3, freezes at 5 and replays at 6 through the same mix. Recording output is 1; replay output is 1.5. Its retained revision-one prefix/final export still has the initial held key 63/velocity 70 and original note-off at offset zero after revision two exists. Read all initial-state channels/reset marker and bytes from that export, not live input state. The 256-page sufficient-capacity scenario does not replace the older 137-page exhaustion evidence.

Composite entries may run nested composites. Regular programs repeat; explicit-mode programs make one pass. Nominal duration describes compilation/seek positions, not a clock generator: pure-sync pulses advance iteration, and a stopped source can still relay an upstream pulse. Unsynchronized programs can remain pending indefinitely without blocking unrelated music.

PlayIteration/SeekIteration choose an authoritative iteration without replaying preceding events. Non-retriggering seek repositions continuing playback; newly started playback/recording has its normal start edge. Direct child controls retain precedence over inherited program actions. A direct Stop defeating a nested Record does not queue that losing Record for the next frame. Use immutable resolver decisions for smoothing, restoration and capture; journal loss cannot change these rules.

LoopId identifies a logical slot/generation, not a host endpoint. Standalone RequestId is validated by its owning resolver's slot/generation state, not a global routing registry. Route each to the correct resolver/engine; reconstructing parts adds no authority. Facade cancellation instead uses the originating runtime Sequence. Do not mix standalone resolver mutation with a running facade or treat a physical NodeId as a musical LoopId. Independent Wasm instances require serialized plain data and local endpoint routing: a Rust pointer, Vec, Box or trait-object address is not a transport protocol.

## Boundary transaction and render contract

Configuration source banks validate indexed MIDI packet headers incrementally; audio sources are immediately Ready. Successful RoleAppend consumes its source IDs, while rejection transfers none. With `preserve_input`, an assigned physical capture row retains its MIDI projection. Otherwise the new projection resets before first reservation/consumer execution and the reset flag clears after finish, not every subinterval. Initial state remains strictly before F. See [source/role preparation](topology-activation.md#3-prepare-a-logical-owner-batch-when-needed) for ownership and currently free metadata requirements.

At a nonempty boundary F:

1. Deliver finite admitted operations and preview the resolver up to the generic
   runtime ceiling.
2. Reserve metadata/take/first-interval capacity for new captures and append
   capacity for continuing ones. Initial MIDI state is the input projection
   strictly before F; events at F remain original events.
3. On capacity/identity exhaustion, latch a new request-family refusal,
   autonomous-start refusal or existing-take exhaustion, cancel provisional
   intervals, and re-preview. Still-useful prepared takes remain owned. Each
   retry adds a new failure; there is no arbitrary retry/subdivision count.
4. Freeze ended takes, activate prepared replacements, select pinned media and
   commit once. New immutable indexes are available before sample/event F.
5. Complete terminal requests and post-settlement queries, publish decisions,
   and execute the actual signal DAG.
6. Validate observation producer identities/roles, commit every staged group,
   then advance the resolver to the interval end. Do not settle the next boundary.

Zero-frame calls do nothing. A transition at the callback end belongs to the next
nonempty callback. Playback/capture operations neither allocate nor retain host
borrows. Large MIDI projections and consumer scratch live in prepared storage.
An optional `BoundaryObserver` receives final decisions and the bounded semantic
journal before rendering; it must obey the same no-allocation/destruction,
nonblocking/no-I/O contract as processors. Journal loss is diagnostic only.

Conservative MIDI reservation can refuse earlier than the bytes actually produced.
Failure positions may differ with interval partitioning under insufficient storage;
this is tested separately from sufficient-resource partition equivalence.
Ordinary exhaustion freezes the common completed prefix and changes decisions
before crossing F. Failed new/repeated starts preserve prior mode/content/take
revision and do not publish a start pulse. Optional export failure does not stop
recording. Unexpected staging/producer/identity violations are interface faults,
not normal exhaustion and not rollback. A propagated error may follow outer-clock
advancement; do not replay that callback. Quiesce and explicitly reclaim owners.

The MIDI renderer continues logical packet/state progress after output overflow,
reports it, and returns `Full` for the generic reserved reset protocol. Unrelated
audio still runs. `Cut::midi_overflows` counts playback-renderer overflow intervals;
it is not a count of every downstream effect or host-output fault. The host must
also use `Engine::drain_midi_recovery` when its output recovery remains pending.
No lossless delivery under insufficient event/byte storage is promised.

## Coherent state and immutable exports

`Operation::State` and `Operation::Export` are deferred to **post-settlement,
pre-render F**, even if their admission preceded a transition due at F. `Cut`
combines frame, graph/topology epochs, resolver state, playback/latest identities, gain, capture failure and playback MIDI-overflow diagnostics. It is a pre-render cut; the public Cut has no AfterInterval field. Successful same-frame paired publication reports the new epochs, not stale context-entry identities. Borrowed media IDs in a state reply do
not acquire ownership. `capture_failure` retains the last preflight refusal until
a successful new activation; incomplete state belongs to the actual captured take.

Export selections are:

- `Playback`: the currently selected content, even when stopped.
- `Latest`: the latest completed capture, which can differ from monitored playback.
- `Recording`: a coherent prefix of the current active take.

A successful export includes a checked owning `ExportId`, its cut and immutable
media view. Active-prefix audio/MIDI snapshots are acquired at the same boundary,
without intervening rendering. If any member fails, temporary leases and reserved
metadata are released and no partial export is published. Optional mutable-tail
copies consume the callback's background transfer allowance; a `Budget` refusal
is not a capture failure. Snapshot cuts and capture metadata remain those of the
retained content; the export request cut does not relabel when old content was
recorded or frozen.

Use `Operation::Cursor { export, midi }`, then `Controller::read(cursor, bytes)`
for bounded reads into an owning boxed return buffer. Inspect the resource reply's
written/eof fields and updated returned request cursor. Keep the export until all
reads complete. Audio bytes are interleaved little-endian f32. MIDI bytes retain
the indexed resource's original frame/length/packet records, not synthetic
restoration packets unless those packets actually passed through the recording
input graph.

MIDI exports also need initial projection metadata. Request
`Operation::MidiInitial { export, channel }` for channels 0–15; each reply contains
one fixed `ChannelState` plus the System Reset marker. These reads address the
same immutable export, not live input state. `MidiState::from_channels` reassembles
the sixteen copied channels and marker without replaying or losing metadata.
The projection's device/parameter-history limitations still apply. Finish with
`Operation::Release { export }`; stale/double releases are errors.

## Shutdown and evidence scope

Before stopping callbacks, execute a musical Stop and drain MIDI recovery/device
cleanup as appropriate. `Engine::into_stopped` is quiescent/off-render: it destroys
processors/runtime owners and preserves planner plus resource store. It does not
send MIDI or settle pending requests after the device stops.

`Stopped::cleanup` permanently quiesces the resolver, cancelling nonterminal
requests without advancing time or undoing published state. It recovers pending
or displaced program owners even if rendering faulted after publication and before
interval finish. This is not resumable execution or an implicit musical Stop.
It explicitly cancels reserved intervals/prepared takes, freezes
remaining active prefixes for disposal, releases every ledger ownership category,
releases groups and removes internally created resources. It is idempotent and
also recovers a partially staged faulted interval. Drive generic reclamation to
return retired pages. Imported resource identities remain caller-owned: remove
them separately after cleanup if desired. Destroy the remaining owners off-render.

The guarded integration tests compare independent complete samples, MIDI packets,
semantic journals, initial states and exported content across whole/single/irregular
partitions and declaration orders, including journal saturation, stalled results,
refusal/recovery and explicit full pool recovery. Their package-Wasm execution is
not an actual loop AudioWorklet. Distinct [publication paths](topology-activation.md) have guarded regression coverage including prepared source/role banks, running zero cuts, stale/reused graph ownership and non-rollback cleanup. Historical reports retain their original source/platform scope. The [independent loop fixture](../../../../tests/loop_engine_host/README.md) exercises a small record/play/FX scenario through direct, bounded-wire and two-thread drivers and actual non-isolated offline/live AudioWorklets. Its primitive-command bridge keeps admission, runtime delivery and domain outcomes distinct across independent ordinary Wasm memories. The browser checks actual downstream samples, complete MIDI/receipts, fixed memory, real processor/heap failure witnesses and successful stopped recovery. Live commands are staged before acknowledged engine-clock start; this is not yet broad dynamic live-ingress coverage. Graph/program/resource payload staging, exports and broader musical scenarios remain host work, and no loop benchmark is supplied. Native replay, package-Wasm tests and the generic NoPlanner fixture remain distinct evidence, not substitutes for actual loop-worklet/newer performance campaigns or hard-deadline certification.
