# Composite execution and ownership

Status: implementation in progress under [P03](feature-parity.md) and
[the implementation plan](../../PLAN_P03_COMPOSITES.md). This document specifies
the accepted contract; it does not advertise current production support.

## Frames and synchronization

One render-owned loop resolver advances primitives and composites on the engine's
immutable quantum. A prepared composite describes sparse child actions on a
frame timeline. Its stored unit is the actual frame duration of one program
iteration. Source pulses, client elapsed time and child positions do not advance
that timeline.

The unit is captured when the arrangement is prepared. Cycle-authored positions
use that stored basis; changing the sync source's length or phase never implicitly
retimes an accepted arrangement. Gridless serial Lua composition uses the maximum
child duration per parallel section and sums sequential sections. Its compiled
unit must represent these spans exactly on the engine's quantum grid. Explicit
timing edits are admitted, versioned changes to the arrangement.

A start applies its complete child snapshot at the effective engine frame.
Every subsequent iteration begins after its prepared unit has elapsed. Stop,
seek and retrigger are authoritative controls. All decisions are speculative until
capture/graph preflight succeeds and the resolver commits. A retry cannot advance
the cursor or emit an action twice. No callback allocation or application timer
is involved.

Synchronization has two separate uses:

- A scheduled command may await a configured source boundary. Skipping n edges
  executes on edge n+1. A stopped source leaves that command pending.
- Repeat alignment may hold a playing loop at the end of its own pass. Earlier
  source edges never truncate the pass. At its end, an aligned loop waits only
  while its source is actively playing and no source boundary occurs. Without
  alignment or an active source, it repeats independently. If the source stops
  during a wait, the next settled quantum releases the wait.

Primitives and regular composites share that repeat rule. Configured source,
command timing and repeat intent are distinct. An explicit-mode composite ends
after one pass; an empty composite starts as a successful stopped operation.
Recording continues in frames until an admitted transition ends it.

At pass end, completed authored child occurrences end even if the parent waits.
The parent remains Playing with its playhead at the end and explicit wait state.
Completed-pass accounting happens once; entering iteration zero happens only when
the next pass really starts. A coincident end/source boundary preserves permitted
same-mode continuations; a gap ends them and a later restart emits a new snapshot.
Internal unit boundaries are not external synchronization pulses. A composite's
own start/retrigger and repeating pass boundaries supply its own musical edges.

## Controls, edits and accepted work

Explicit child commands outrank competing inherited or explicit program actions
at the same frame. An independently controlled child never advances its parent.
Later authored actions may affect that child through normal arbitration. Nested
starts and seeks establish descendant snapshots in the same quantum.

Prepared edits carry active/pending versions. Safe changes strictly beyond the
current and next unit may activate before wrap if they preserve current ownership
and state. Other running changes wait for an actual pass restart, or use an
explicit structural stop/reconfiguration. Waiting at the end is not permission
to emit a new plan's first section. Cancellation/refusal retains the working plan
and reliably returns the candidate owner; activation returns the displaced owner.

The session service owns authored data, timing basis, repeat and monitoring intent,
accepted operations and retained results. The adapter owns identity mappings;
the resolver owns actual execution. A disconnected UI or lost Lua VM does not
cancel accepted playback, schedules, edits or automatic monitoring. Future VM
commands are not accepted work, and reconnect does not restart scripts.

Persistence records the accepted timing basis and confirmed plan, including after
the source has changed length. Pending edits cannot be mixed into active capture.
Load remaps child identities and starts stopped. Legacy migration derives missing
timing from the saved session, not the destination's running sync source.

## Prepared capacity

The initial production budget is 256 program entries, 1024 compiled segments and
512 timing/action relations across the engine. Composite count uses the negotiated
logical loop slots; composites consume no playback/capture nodes or host ports.
These budgets do not reduce existing primitive slots or recording media capacity.
Engine preparation exposes them as configurable, finite negotiated limits.

The production engine carries these through protocol v8, native setup v7 and
worklet configuration v4. The relation capacity is at least the prepared logical
loop count, so adding composite budgets never removes existing primitive capacity.
For N loop slots, E entries and S segments, the prepared LTP image holds at most
`1 + 4*N + E + S` records of 64 bytes. Payload pools cover that image and append
it in bounded batches when it exceeds the command window. The active, next and
private replacement models each prepare S owned-span slots; the ingress stage
prepares S plain spans. Their byte sizes use the target ABI's `size_of` values.
Runtime candidate/retirement storage remains checked by the loop library's limits.

Account separately for active metadata, staging receivers, candidate/displaced
owners, pending requests and reliable results. Use the loop library's checked
storage calculations rather than estimating only raw entry bytes. Existing
single-owner program staging may serialize edit publication; saturation refuses
new work before acceptance and never stops admitted autonomous playback.

## Production preparation and publication

The pure `shoop_loop_plan` compiler is shared by loop preparation and adapters.
It contains no runtime or application dependency. The dependency audit checks that
boundary separately; the render runtime continues to forbid every Shoop crate.
Sparse spans, rather than authored documents, cross the RT protocol. The endpoint
checks counts, quantum/sample rate, identities, modes, ordering and version fences;
the prepared LTP receiver independently validates the complete timing/action DAG.

Program staging IDs increase over the endpoint lifetime. Full resync publishes
their floor, including cancelled stages. An edit names the exact current loop
generation and active version; rejected/cancelled candidates do not replace it.
An empty timing slot has version zero. Conversion also fences the old primitive
identity and reserves a fresh generation. At the render-owned activation cut,
`ConvertEmpty` rechecks that it is still empty and stopped before changing any
owners. A recording admitted during preparation survives a refused conversion.

Composite slots have no active capture/playback/fader bindings. Their optional
strip owner controls lifecycle only; standalone composites need no strip. Private
replacement prepares their plans alongside primitive media and publishes all new
identities stopped. Repeat intent is installed with replacement activation.

Loop telemetry reads one checked atomic publication without spinning. It includes
the settled topology epoch so the endpoint cannot combine a newly active frame
state with an older document. Composite telemetry carries its frame, mode,
position, length, cursor/pass/wait state and confirmed active/pending versions.
Resync carries the complete active spans, source, ownership and repeat policy.

## Reference disposition

| Behavior | Disposition |
| --- | --- |
| Stable event IDs, serial/parallel layout, move/copy/delete, nesting and child control | Preserve and test through production UI/Lua. |
| Deterministic arbitration, contiguous playback and owning publication | Preserve under frame-driven execution. |
| Source ticks advancing every internal iteration or stalling a running program | Superseded by frame progression. |
| Application elapsed-time fallback, one section per pump and discarded overshoot | Superseded by render-owned frame progression. |
| Safe future-only live edits and deferred wrap edits | Preserve using the prepared frame timeline and real restart eligibility. |
| Automatic monitoring, persistence and socket recovery | Restore with authoritative service/host ownership. |
| Replacing/dry-wet script events | Depend on P05/P06/P07; full P03 closure remains gated on these interactions. |

Retain old-stack fixtures as historical evidence. New-library and production tests
must describe the new contract explicitly, including source stop/restart/resize,
repeat waiting, transactional preflight and callback-partition invariance.

### Adapter transactions

Prepared edits reserve one logical mutation ticket. Begin/span/commit/cancel wire
requests do not consume application mutation sequences. Uploads retain a bounded
32-record window, including plans larger than the mutation-result pool. Reliable
receipts carry the actual render activation frame; projection publishes a new
version only with matching generation/version telemetry. Reliable resync includes
active spans, the ingress ID floor, candidate header/spans and admission phase.

When a pending plan would block a structural change, the production adapter
cancels the retained RT candidate, allows that structural transaction to finish,
and restages the accepted immutable spans against the new topology. This changes
no musical time. Later explicit source reassignment wins over the earlier edit's
source selection. Stop clears transport requests while preserving owning plan
edits so they can publish after Stop. A combined program/source update is atomic
and uses stopped/restart eligibility instead of early future-only adoption.

Clear retires a track-owned composite's timing identity, stops its authored child
occurrences, and installs an empty primitive with a fresh runtime generation. Its
public backend identity remains stable. Public feature publication remains gated
until the service, editor, monitoring and persistence paths are complete.

## Automatic monitoring

The service owns the automatic monitoring policy and preserves manual monitoring
separately. The render owner derives demand from actual primitive recording,
admitted starts whose countdown reached zero, and upcoming recording sections of
the active prepared program. Nested starts inspect their initial sections; a
continuous nested Play keeps its own phase. An accepted pending edit contributes
automatic demand only after it becomes the active program.

Recording onset and release do not depend on application polling. A direct child
Stop releases its automatic demand while the parent clock continues. Other
recorders on that track retain their demand, and manually monitored tracks remain
on. Repeat waiting ends outgoing recording demand; an actual restart establishes
new demand. Disabling the automatic policy changes monitoring while capture keeps
following its own transport.

Demand traversal uses prepared slot-sized arrays and a fixed queue, with at most
one queued inspection per composite. Loop ownership and strip policies use two
generation banks. Preparing a private replacement preserves active ownership;
activation selects the new generation without overwriting a policy already
confirmed for it. No render allocation, journal replay or application timer drives
this policy. The backend retains manual intent for capture and publishes automatic
monitoring from rendered state. Composite file support remains gated separately.
