# Fixed-graph topology activation through the facade

`Engine::prepare_with_staging` adds prepared timing-topology candidates to the
coordinated facade. Both immediate timing-topology replacement and owning
single-program zero/stopped-cut activation are implemented **fixed-graph paths**.
Neither replaces the signal graph, physical roles or logical generations.

## Preparation

The method takes the ordinary preparation arguments, followed by
`TopologySettings { candidates, records_per_callback, validation_steps_per_callback }`
and an optional prepared observer. All three settings are positive when enabled;
all zero disables staging. Existing constructors keep it disabled.

Preparation allocates candidate receiver banks and validation scratch, and copies
the initial topology into a maximum-capacity bank using the canonical codec.
This initial conversion, including complete validation and compiler-owner disposal,
is off-render. Each later activation exchanges whole prepared owners. Retained
metadata/scratch sizing is checked before allocation; allocator overhead and
transient preparation/compiler memory are separate. Explicit black-box writes
pre-touch initialized receiver metadata and scratch, including vectors subsequently
cleared for reuse. This is not a guarantee of pinned pages or callback deadlines.

Provision the generic runtime's transfer allowance as well: its scheduler payload
stride is capped by `budgets.transfer_bytes`. A facade record allowance does not
enlarge that runtime allowance. Headers are 64 bytes; chunks contain complete
64-byte records. Source compilation and payload allocation belong off-render (or
in an independently prepared host transport), not in an AudioWorklet message
handler as an allocation workaround.

## Operations and ownership

Use `Controller::submit_payload` for header/chunk bytes and ordinary `submit` for
payload-free operations. Payload ownership follows the generic runtime's owning
completion path; admission is distinct from the typed execution result.

| Operation | Meaning |
| --- | --- |
| `TopologyBegin { slot }` | Consume one LTP1 header; return scoped StageId and Receiving/Validating status. |
| `TopologyAppend { id, first_record }` | Transactionally accept an ordered complete-record chunk. |
| `TopologyValidate { id }` | Spend remaining callback validation steps; return Progress, without waiting for more work or data. |
| `TopologyStatus { id }` | Return current candidate status. |
| `TopologyCancel { id }` | Cancel unpublished data; return Idle, or Taken while a cancelled pending program owner awaits interval finish. Payload-free and recovery-safe. |
| `TopologyActivate { id, graph_epoch, topology_epoch }` | Check and activate a Ready candidate at delivery; changed programs must already be stopped. |
| `ProgramActivate { id, target, graph_epoch, topology_epoch }` | Retain a Ready owner and reliable request until the selected composite reaches an eligible zero/stopped cut. |
| `TopologyRecycle { id }` | Recycle the displaced owner held after successful activation; return Idle. |

Unexpected payloads on other operations are refused. Stale/foreign stage IDs
cannot address a later attempt. Syntax/chunk errors do not consume a partial
chunk; semantic failures remain unreadable. Failed activation retains the exact
Ready candidate for inspection, retry or cancellation.

After successful activation the candidate slot reports Taken and owns the
**displaced** topology in prepared retirement storage. It cannot begin a new image
until explicit recycling returns that storage to the receiver. Cancellation does
not undo an active configuration and is refused in this state. Clearing/recycling
Copy metadata does not destroy its allocations; final destruction is off-render.
Stopped cleanup recovers all receiving, validating, ready, failed and retired
candidate states, including after controller loss, and is idempotent.

## Work and receipt separation

Record and validation allowances are shared across all candidate slots and every
operation/internal interval in one host callback. The generic runtime supplies
`OperationContext::callback_start` and `PlanContext::callback_start`; the facade
resets these allowances only when that enclosing callback changes. A failed
append/validation conservatively consumes the offered allowance. Begin/reset,
identity lookup and ownership lifecycle work are separate finite prepared-size
costs; the step counter is not a universal instruction budget.

Validation is explicit external preparation. Call again in a later callback if
more work remains; it never waits inside a call. Once music is admitted, neither
validation progress, unread metadata results nor unrecycled retired banks gate
its recurring events. Recovery cancellation has an independent runtime receipt
slot and does not need a normal domain-request slot.

## Immediate topology activation

`TopologyActivate` happens during operation delivery, **before musical preview and
sample/event execution at that frame**. It checks the runtime-provided graph
epoch, expected resolver topology epoch, and exact ordered logical IDs and kinds.
The active signal graph, physical consumers, resource claims, media selections
and logical generations are not changed by this API. Existing primitive content
and capture state remain authoritative; a node's creation-length metadata does
not replace recorded/imported content.

The resolver checks program compatibility before publishing anything. A changed
program must already be stopped; unchanged running programs and continuing
primitive capture/playback may remain active. A Stop merely queued for that same
frame has not yet settled and does not satisfy this precondition. Conversely,
same-frame musical Play after a successful configuration operation uses the new
program during settlement, even if its request was delivered earlier in that
frame. Post-settlement State/Export queries expose the resulting configuration.

Pending countdowns are revalidated against the new sync references; invalidated
requests receive their own reliable terminal failure. Rejections leave the active
configuration unchanged. Successful activation retains the displaced owner before
reporting success; recycling is a separate operation and cannot retroactively
turn that activation into an ordinary refusal.

`Cut` includes `graph_epoch` and `topology_epoch` alongside its frame and state.
These identify the configuration of that coherent cut, not a newer label for a
previously retained media snapshot. Imported/recorded media and exports retain
their existing immutable revisions, content and capture cuts.

## Owning single-program activation

`ProgramActivate` moves a validated full-capacity receiver owner into the resolver's
[owning transaction](program-changes.md). Only the selected composite's program and
compiled nominal length may change. IDs/kinds, sync/triggers, other programs and
length metadata must match; existing media, graph, consumers and capture claims
stay authoritative. Rejection before admission restores the exact Ready owner.
The runtime graph epoch is checked at delivery and rechecked during planning; the
resolver checks the expected topology epoch. The restricted controller does not
currently expose a way to change the signal graph after preparation.

Admission reserves one normal domain slot and the resolver's single configuration
owner slot. The candidate reports Taken while pending. An idle clock can keep the
change pending indefinitely, without inventing a boundary or blocking existing
music. At an eligible cut, real capture preparation and first-interval reservation
run against candidate decisions. A configuration-origin failure supplies a family
refusal and re-previews the old topology before publication. No old zero-actions
are executed merely to discover eligibility, and refused candidates create no
capture revision gap or configuration event.

`Reply::ProgramChanged { id, graph_epoch, topology_epoch, completion }` is the
terminal domain result, not admission acknowledgement. The completion contains
requested/effective/outcome frames; the generic execution separately preserves
requested, delivered and actual completion frames. The reply epochs identify the
settlement cut, including on failure. The same cut's State/Export queries see the
published topology and media. Raw `Transition::Program` is refused by the facade.

After a successful interval finish, collection frees the internal domain slot and
moves the displaced owner into that candidate's retirement storage. Its status
stays Taken until `TopologyRecycle`. A failed/cancelled request instead restores
Ready for retry or cancellation. External unread results do not delay this internal
collection or autonomous progression. Other retired banks may remain unread while
a later change uses a different Ready bank.

`Cancel { sequence }` distinguishes pre-delivery cancellation from cancellation of
the retained resolver request. Cancelling a delivered pending program this way
restores Ready after finish. Recovery-safe `TopologyCancel` additionally requests
recycling of the unpublished owner: its immediate status remains Taken and becomes
Idle after terminal collection at finish. It cannot undo an applied change.
Higher-sequence direct Stop and same-frame controls retain the resolver's documented
precedence; a successful intervening immediate topology replacement fails an
unactivated program request as stale while retaining its candidate.

Stopped extraction permanently quiesces the resolver before collecting any pending
or already-applied owner, including a render fault after configuration publication
but before interval finish. Quiescence does not finish an unrendered interval,
advance time, roll back published state or invent a musical Stop. Outstanding
nonterminal requests become Cancelled at the last logical frame; published results
remain unchanged. Resource cleanup uses the actual committed capture prefix.
Repeated cleanup is idempotent; allocations are destroyed only off-render.

## Evidence and remaining work

Six guarded integration tests cover complete independent stereo samples, MIDI
packets, journals and every observer-provided public State field, three callback
partitions, declaration reversal, journal capacities 0/1/1024, stale/running
refusals, owning payload returns, callback-shared budgets, 32 stalled normal
receipts, recovery, countdown invalidation, cancellation and teardown. Activation
during monitored recording preserves exact FX audio/MIDI, prefix/full exports,
all initial MIDI channels and final resource recovery.

Native debug/release, Node/Chromium and broad regression gates passed for this
implementation step. The new cases also completed in the interrupted Miri run;
that is partial evidence, **not** a complete stage-level Miri pass. See
[the job ledger](../../../../docs/loop_layer/MIRI_JOBS.md).

Five additional guarded facade cases now verify running program-bank activation
with independent full audio/MIDI/state/journal comparisons; real capture and
immutable exports versus resource-family refusal; pending cancellation, stale
replacement, disconnect and post-publication render-fault extraction; delivery
validation, pre-dequeue cancellation and stage generations; and 130 one-sample
clock wraps with full normal domain slots, unread results, recovery and bank retry.
A sixth standalone case verifies permanent quiescence in five lifecycle phases.
See [facade-program-stage6.json](../../../../docs/loop_layer/evidence/facade-program-stage6.json)
for131 loop tests per native/Wasm profile and2110 workspace passes/four existing
skips. New cases are registered for consolidated Stage6 Miri, not yet interpreted.

General graph/role-generation activation, coordinated changes beyond these
fixed-identity preconditions, broader mixed lifecycle stress and consolidated
Stage6 verification remain open. Independent actual loop AudioWorklet acceptance
remains Stage7 work.
