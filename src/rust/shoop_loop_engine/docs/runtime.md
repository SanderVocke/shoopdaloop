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
The example is not a device or AudioWorklet demonstration.

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
No unrestricted live graph replacement is exposed. Transactional live
loop/graph/program replacement is subsequent integration work.

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
bound. Do not release transferred leases independently.

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

`Controller::submit(lane, at, operation)` uses the prepared generic transport.
Submission returns a ticket or returns the unchanged owning command on refusal.
Drain both channels:

1. `try_completion()` returns the owning transport command and admission result.
   Scheduled admission is **not** musical success.
2. `try_execution()` returns the reliable typed terminal result. Inspect both the
   generic execution result and `operation_result`: `Reply::Failed` or a failed
   `Reply::Transition` is not success merely because delivery succeeded.

`Operation::Transition` accepts resolver operations except raw `Replace`.
`Operation::Select` pins checked media and derives replacement length/revision
from its immutable metadata. A pending selection retains its pin even if its
resident reference is retired. Winning decisions select that media; losing,
failed or cancelled requests do not change playback. Pins are released after the
terminal interval finishes. Selecting while recording follows the resolver's
replacement refusal; it does not freeze the current take implicitly.

Requested, delivered and terminal frames remain distinct. Late delivery retains
the original requested frame; a countdown may finish much later without being
classified as transport lateness. Runtime sequence identities supply ordering.
Resolver completions are forwarded after boundary commit, then recycled only
after finish, when immutable decisions are no longer in use.

`Operation::Cancel { sequence }` distinguishes a queued runtime operation from a
delivered pending resolver request. The cancelled operation receives its own
terminal result; cancellation has a separate acknowledgement. Ticket cancellation
is the generic transport's pre-delivery mechanism, not a substitute for cancelling
a delivered musical request.

Only cancellation and immediate `Transition::Stop` are recovery-safe. Submit them
on `Lane::Recovery` at/before the current frame. The independent generic recovery
slot and extra domain slot permit recovery with normal requests/results saturated.
Recovery is not an unlimited queue: its own reserved result must be recycled before
reuse. Stalled results prevent further admission, not admitted automatic wraps.
Controller disconnection alone does not invent a musical Stop.

`Operation::Gain` changes the primitive's finite (including negative) gain.
`Controller::parameter` sends ordinary parameters to effect nodes. Neither creates
a new musical clock. Smoothing follows the resolver's discontinuity flag, not
journal events, host cuts, or a gain change by itself.

## Boundary transaction and render contract

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
combines frame, resolver state, playback/latest identities, gain, capture failure
and playback MIDI-overflow diagnostics. Borrowed media IDs in a state reply do
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

`Stopped::cleanup` explicitly cancels reserved intervals/prepared takes, freezes
remaining active prefixes for disposal, releases every ledger ownership category,
releases groups and removes internally created resources. It is idempotent and
also recovers a partially staged faulted interval. Drive generic reclamation to
return retired pages. Imported resource identities remain caller-owned: remove
them separately after cleanup if desired. Destroy the remaining owners off-render.

The guarded integration tests compare independent complete samples, MIDI packets,
semantic journals, initial states and exported content across whole/single/irregular
partitions and declaration orders, including journal saturation, stalled results,
refusal/recovery and explicit full pool recovery. Their package-Wasm execution is
not an actual loop AudioWorklet. Inter-loop/composite activation/replacement,
independent loop hosts, performance and final CI/review acceptance remain separate
plan milestones.
