# Scoped processing contract

This contract implements the acceptance criteria in [the plan](scoped-processing-plan.md).
The initial implementation has one root callback and one optional interleaved
planner region. Prepared callback consumers can surround one another in normal
signal order. Arbitrary recursive, parallel and multirate regions are deferred.

## Execution and timing

A processor declares its cadence during preparation. `Interval` is the compatible
default: the registered planner and scheduled actions may subdivide its region.
`Callback` names the root callback as its parent and requires exactly one call
over `[F, F+N)`. A zero-frame callback invokes neither cadence and advances no
clock. A callback consumer does not imply a fixed plugin quantum.

An interleaved planner retains the existing `plan -> process -> finish` lifecycle.
All its interval consumers form one atomic region in the parent dependency DAG.
Callback predecessors execute first, the region executes all its intervals, then
callback successors execute. Contracting the region must preserve an acyclic
graph. A path from the region through a callback processor back into the region
is refused, even if the uncontracted signal DAG is acyclic. Unrelated callback
nodes execute once in deterministic dependency order.

A callback planner instead prepares bounded immutable decision spans covering
the complete callback. Its consumers execute in ordinary signal order, applying
the spans to slices of that callback's audio and MIDI. This allows
`dry playback -> callback DSP -> wet capture` without a second clock or a delayed
block. Generic planners are never implicitly converted to this lifecycle.

One runtime frame authority, ingress envelope and resource budget cover the whole
callback. Internal planning and consumer hooks do not reset these allowances.
Full-block processor invocations and local media spans are separate counts.

## Loop ownership and media phases

The shared callback adapter owns one real loop `Resolver`. Each primitive LoopId
has one timing decision per interval and advances once. A logical dry/wet owner
may bind playback and capture phases to that same LoopId; these are media phases,
not independently advanced primitive loops. The first adapter exposes explicit
capture windows separately from the primitive playback mode. This is a low-level
phase API, not restoration of application dry/wet modes.

Playback reads retained imported snapshots. Source resource/revision identities
remain stable until the prepared adapter is torn down off-render. Spans contain
absolute frame ranges and settled source positions, including wraps and sync.
Playback and capture consult the same timing authority. Musical sync cycles
accepted by the resolver are distinct from unsatisfied signal dependencies.

Capture intent can be prepared before input exists, but publication of a capture
take waits until its input is ready. A generic before-consumer hook obtains the
actual routed MIDI, advances input state through the prefix preceding the capture
window, and prepares/reserves the coordinated resource group. In particular,
DSP-generated MIDI contributes to the initial checkpoint. Audio and MIDI writes
then use real render-resource access; an after-consumer hook commits only after
actual successful execution. There are no synthetic playback/capture observations
and no calls advancing the old interval planner without running its consumers.

Capture windows may start or end within a callback and continue across callbacks.
The initial profile is feed-forward timing: decisions cannot depend on media that
will only be produced later in the callback. Such work uses the interleaved API
inside an admitted region. It is never guessed or silently delayed.

## Actions and graph activation

Admission validates identities, generations, ports, payload limits and planner
signatures. A callback-relative restriction is checked at execution because the
host's future block sizes are not known when a command is submitted. A refusal
has an explicit terminal error at the requested delivery frame; it does not mutate
the target or round the frame. Host MIDI buffers retain ordinary sample offsets.

| Work | Interleaved interval target | Callback target / callback planner |
| --- | --- | --- |
| Parameter or typed processor operation at callback start | Execute before rendering | Execute before rendering |
| Parameter or typed processor operation inside callback | Execute at exact local cut | Refuse `InvalidFrame` |
| Scheduled MIDI at callback start | Inject at offset zero | Inject at offset zero |
| Scheduled MIDI inside callback | Inject at exact local cut | Refuse `InvalidFrame`; use host MIDI for sample-offset event streams |
| Planner operation at callback start | Existing semantics | Execute before callback plan publication |
| Planner operation inside callback | Existing local semantics | Refuse `InvalidFrame` |
| Generic graph commit/install/remove | Existing ingress boundary | Existing ingress boundary, validate cadence signature |
| Planner graph activation at callback start | Validated normal-lane permit | Validated normal-lane permit before spans are published |
| Planner activation inside callback | Allow only compatible interval-region edits | Refuse after callback plan publication |
| Recovery operation | Existing bounded safe family | Callback boundary only; no arbitrary structural edits |

Compatible internal activation preserves every callback node, its generation,
its order and every edge touching a callback node (including host edges). Internal
region edges and region-to-host routing can change at their precise local cut.
Changes to parent interfaces or grouping are refused before publication. The
check applies to every intermediate activation, preventing stale parent-buffer
use. Pending injected MIDI remains an additional compatibility constraint.

Staged graph owners retain existing scope/generation/epoch checks. A successful
activation stores the displaced owner in its reserved stage; no plan is freed
during rendering. Callback spans cannot outlive their callback and cannot survive
a topology or source generation change. The callback adapter initially admits
only its exact prepared consumer bindings; incompatible graphs are refused.

## Buffering, bounds and failures

Audio uses prepared planar callback storage. Interval outputs remain available to
callback successors. MIDI has separate interval scratch and callback accumulation;
region boundaries translate offsets exactly once. Capacity applies to the complete
callback event stream. Overflow invokes the existing reserved reset path and is
reported rather than truncating successfully admitted traffic silently.

The runtime's prepared node/edge/port/frame and MIDI limits bound all scope scratch.
The shared loop plan reserves at most `max_frames` spans per prepared owner: even
one-frame boundaries fit without a diagnostic or subdivision quota. Preparation
uses checked products and refuses sizes exceeding the allocator-independent
signed byte bound. The adapter may expose a smaller explicit span allowance for
testing/prepared-memory tradeoffs in a future profile. This implementation requires
the full `max_frames` allowance during preparation: insufficient plan storage is
refused before the runtime starts. This avoids a partial timing advance or a
render-time storage failure on dense valid boundaries. No partial plan is
published. Input storage and capture reservations have independent capacities.

| Failure point | Required result |
| --- | --- |
| Invalid graph, grouping, identity or block contract | Refuse preparation/activation; retain active owners |
| Inadequate plan storage | Refuse before timing/plan publication |
| Capture prepare/reserve failure | Do not publish that recording transition; retain previous snapshots and report refusal |
| Consumer fault after timing publication | Advance outer time once; cancel uncommitted capture, report failure; no replay or fabricated success |
| Missing/delayed consumer completion | Do not commit or complete successful capture; retain/cancel reserved ownership safely |
| Audio or MIDI member fault | Do not publish a partial coordinated capture group |
| MIDI output exhaustion | Reserved recovery remains available and diagnostics report overflow |
| Receipt/diagnostic backpressure | Existing bounded refusal; never discard a musical boundary |

Capture completion identifies actual effective window frames and real snapshots.
An accepted request is not a successful capture. Previously exported snapshots
retain their leases across refusal, replacement and cleanup. Final reclamation,
including graph and processor owners, occurs off-render.

## Compatibility and proof

Processors default to interval cadence and planners default to interleaved mode;
existing applications preserve their behavior. Opt-in callback processors cannot
consume transient interleaved decisions or observations. Replacement requires the
same prepared cadence and planning signature. New process-block fields are borrowed
views only; the few in-tree explicit constructors must be updated.

Two runnable canaries use a real GraphRuntime, prerecorded loop media, nonzero
audio and MIDI, independent absolute-frame oracles and a deterministic full-block
gain/MIDI processor. One tests a subdivided region feeding a bus-shaped successor;
the other tests shared playback/capture phases around DSP, including wraps and an
internal capture boundary. Tests cover irregular callbacks, large frame origins,
multiple channels/owners, refusal and post-publication fault paths, MIDI recovery,
activation compatibility, dense boundaries and allocator guards.

The portable crates remain `no_std` with preparation-only `alloc`. Cortex-A32,
Cortex-A64, Cortex-M, WebAssembly and native validation remain required. Production
bus controls, persistence and recovery stay covered; bus FX and Carla remain
subsequent integration work.
