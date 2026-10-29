# Owning single-program changes in the standalone resolver

`Resolver::stage_program(expected_epoch, target, sequence, at, candidate)` retains
a prepared topology until a program change can be selected and committed. It
returns a normal `RequestId`; admission is not activation. The coordinated facade
exposes it through staged-bank ProgramActivate (fixed graph), or ProgramGraphActivate
(coordinated prepared routes); see [publication paths](topology-activation.md).
Immediate TopologyActivate still requires changed programs already stopped.

Run the independent [program_graph_cut](../examples/program_graph_cut.rs) example:

```sh
cargo run --locked -p shoop_loops --example program_graph_cut
cargo run --locked -p shoop_loops --release --example program_graph_cut
```

A one-frame source clocks an old four-iteration program. The pair is delivered at
start+1, waits until actual zero at start+4, then publishes a two-iteration program
and bypass route together. Scalar effect output changes from 1 to 2 at that exact
sample, both epochs advance, and the continuing child retains its four cycles.
Whole/single/split calls and zero at the cut agree under heap guards. Node pairs are
timing-only: those samples witness the independent effect route, not loop media.
Timing storage and the displaced graph return independently; stopped cleanup
recovers the remaining owners. Mixed-media refusal/cancellation remains covered
by the larger regression scenarios, not claimed for this example.

## Scope, capacity and ownership

This path changes one composite's program and compiled nominal length. All logical
IDs/generations, node kinds, sync sources, triggers, other node lengths and other
programs must match. The candidate must already be compiled/validated, with the
same limits. It may change the action DAG through that one program's targets;
normal candidate validation still prohibits action feedback and recursion.

There is one retained configuration-owner slot per resolver, in addition to a
reserved normal request slot. A busy owner slot refuses another candidate with
`Capacity`, even if the earlier result is terminal but uncollected. This is finite
external admission, not a musical-event quota. Candidate rejection returns the
same owning topology; no metadata is destroyed on that path. Stage admission checks
the aggregate size of the two metadata owners plus the transaction descriptor.
Compiler/receiver preparation and eventual destruction remain off-render.

Use ordinary `completion(id)` and `cancel(id)` for the retained request. Collect a
terminal change with `take_program_change(id)`, which returns:

- the normal `Completion`, distinguishing requested/effective/actual frames;
- `activated_epoch: Some(epoch)` on activation, otherwise `None`;
- the displaced topology on activation, otherwise the cancelled/refused candidate.

`take_completion(id)` refuses to release this request on its own: dropping its
identity without returning the owner would lose the ownership contract. Collection
is forbidden during a committed interval; finishing the interval permits it.
Request-slot generations advance normally, so stale handles cannot collect or
cancel a reused slot. Unread outcomes/owners do not stop automatic music.

For final teardown, `quiesce()` permanently disables further execution, invalidates
speculative decisions and cancels outstanding nonterminal requests at the last
logical frame. It permits owner collection even after an unfinished committed
interval. Already-published state, epoch and results remain unchanged; no time is
advanced and no unrendered interval is declared finished. This is not pause/resume,
rollback, musical Stop or resource cleanup. `is_quiescent()` reports the terminal
lifecycle state. The caller must separately recover resources and destroy owners
off-render; the facade's stopped cleanup coordinates that work.

`Transition::Program` is the receipt/journal tag for this transaction, not a raw
musical command. Direct `submit` of that tag is rejected without a candidate.

## Choosing and publishing the cut

A request waits until its effective frame. A stopped program can change at that
boundary. A running program waits for its next actual entry to iteration zero;
a source that leaves a cursor idle at zero does not repeatedly qualify. A
successful explicit seek/restart to zero can establish another cut. No nominal
clock, fixed delay or propagation quota is introduced.

Selection probes the old configuration's unpublished control/clock state before
that program emits its child actions. The candidate is selected only if the
request remains live and the resulting cut qualifies. A higher-sequence direct
Stop cancels the older pending configuration request through normal cancellation
rules; an older Stop may settle and a newer configuration request may activate
at the resulting stopped cut. A refused seek does not create an artificial zero
entry. Configuration selection is separate from musical-mode arbitration.

After selection, the candidate DAG is used for the real preview. Removed active
targets receive Stop intents before component traversal, so removal also works
when the new DAG places them before their former parent. An authoritative
seek/restart/stop cut also stops removed declared targets. These intents participate
in normal source/ordinal/mode precedence; a winning direct child command still
wins. Removed nested programs stop their descendants through ordinary settlement.

At a natural zero cut, targets continuing in the same mode retain their playback
or capture progression rather than being restarted. New targets enter their new
mode at the cut. Pass history is retained across the change; explicit start/retrigger
keeps its normal reset semantics. An empty new program stops instead of fabricating playback.
When an old explicit one-pass program reaches its end, it remains stopped after
installation; installation does not implicitly start another pass. A coincident
explicit restart can instead establish a playing zero cut using the new program.

The preview emits one configuration transition event and associates its direct
and nested program actions with the configuration request's `Decision::origin`.
OnPulse actions retain their ordinary autonomous attribution. To reject the
configuration's capture/resource preflight atomically, supply
`Observation::Refused { request: change_id, reason }` and preview again. The
candidate then stays unpublished, the old working program settles normally, and
its legitimate due actions execute once. Target-only `CaptureUnavailable` retains
its distinct target-only meaning; it is not a substitute for family refusal.

Only `commit` swaps active/candidate metadata owners, advances the topology epoch,
and publishes states, outcomes and journals together. The displaced owner remains
in the reserved slot. No ordinary failure remains after that publication point.
Cancelling or changing retained inputs invalidates a speculative preview. A
successful intervening `replace_topology` terminates an unactivated program
request as stale (or with its earlier identity/sync failure), retaining its owner.

## Work and integration limits

Preparation uses existing prepared resolver scratch and one inline owner
reservation. Candidate compatibility checks are bounded by prepared metadata.
While a change is waiting, selection may perform an old-configuration prefix probe
and then normal settlement. At activation, old/new target comparisons use indexed
lookups and finite target traversal. This adds no allocation, destruction, blocking,
external-producer retry or arbitrary musical-work limit. These costs are not a
constant-time operation or a callback deadline guarantee.

This standalone API does not update a runtime signal graph, physical node pair bindings,
resource claims or logical generations. The facade connects staged receiver owners, reliable execution tokens, current
epoch checks, capture-family preflight and stopped cleanup to it. ProgramGraphActivate
holds only a reference to an unlocked Ready graph while waiting, then rechecks it
and borrows publication authority at the real cut. Logical IDs/kinds, sync/triggers,
other programs and physical node pairs stay fixed. Stopped-owner ConfigurationActivate
is a different transaction for generation edits. No path offers arbitrary processor
construction, port resizing, independent-host or deadline certification.

## Evidence

The original nine guarded native/release/Node/Chromium cases cover:

- independent complete positions/cycles/cursors and ordered journals across whole,
  single and irregular partitions, reversed declarations and journals0/1/1024;
- exact zero activation, preserved contiguous children and changed nominal length;
- capture-family refusal/retry without a revision gap, and no old zero-action
  pulse or capture restart in the candidate preview;
- stalled clocks, cancellation, stale epochs, retained-owner admission pressure,
  raw-tag/shape refusal and reliable owning returns;
- same-frame Stop/seek/restart/refusal/supersession of musical controls, cancellation
  after speculative selection, stopped/empty updates and explicit-program endings;
- removed nested programs with a different candidate DAG order and direct override;
- all128 one-sample intervals with every request slot occupied and the displaced
  owner/result left unread.

See [program-change-stage6.json](../../../../docs/loop_layer/evidence/program-change-stage6.json)
and its provenance report for125 loop tests per native/Wasm profile,2104 workspace
passes/four existing skips,19 scoped gates and production borrowing proofs.
That original report deferred Miri and registered nine cases; the later
consolidated [Stage 6 interpreter evidence](../../../../VALIDATION.md#miri-evidence)
has its own immutable implementation/source scope. No new interpreter run follows
from documentation edits. The initial empty-program fixture wrongly
expected a caller-supplied length of42; compilation derives the empty program's
nominal length as0. That failed log is retained, and nonempty length publication
is checked separately. No workload or guard was reduced.

The subsequent facade milestone adds a tenth standalone case covering quiescence
before preview, during speculation, during an applied or still-pending committed
interval, and after finish. It checks unchanged state/time, permanent execution
refusal and correct candidate/displaced owner return. An additional E0502 proof
rejects quiescence across a retained decision borrow. These and the five new facade
cases are covered by [facade-program-stage6.json](../../../../docs/loop_layer/evidence/facade-program-stage6.json);
that milestone claimed no new Miri invocation. Consolidated evidence is recorded
separately; the public example runs above are native debug/release evidence only.
