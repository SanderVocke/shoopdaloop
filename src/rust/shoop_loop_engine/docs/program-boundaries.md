# Unpublished composite boundary probes

`Resolver::probe_program_boundary(target, observations)` resolves a composite's
prospective state at the current absolute frame, stopping **before that target's
program or trigger actions are emitted**. It returns a copy-valued
`ProgramBoundary { frame, topology_epoch, state }`. This is an informational cut, not candidate ownership or activation authority.
The owning transaction uses this mechanism internally before actual publication.

The probe uses the resolver's ordinary retained requests, observations, component
order, direct/reactive arbitration, pure-sync clock propagation and checked cursor
advancement. Incoming nested-program actions are resolved normally. The target's
old iteration-zero child actions are not executed and then undone. Normal preview
uses the same cursor-advancement implementation followed by action emission.

## Interpretation

- `enters_zero()` is true when the prospective composite is Playing and enters
  iteration zero **at this frame**, including a successful start/restart or seek
  to zero. An idle source that leaves a running cursor at zero does not repeatedly
  produce eligible zero cuts.
- Inspect `state.mode` separately for a stopped cut. An explicit one-pass program
  ending at this frame reports Stopped, not a manufactured new playing iteration.
- Pre-clock Stop masks, reactive Stop after an actual pulse, highest-sequence
  arbitration, cancelled requests and preflight refusals keep their usual rules.
  A refused upstream capture start does not invent the missing clock edge.
- The epoch identifies the topology used for this computation. The result does
  **not** certify a replacement candidate, reserve an epoch, own any metadata,
  authorize a later activation, or guarantee that future observations agree.
  Changed inputs require a new probe and the eventual transaction must revalidate
  its own candidate/generations/resources.

## Publication and borrowing

The probe invalidates any previous **uncommitted** preview and reuses the prepared
scratch arrays. It creates no committable interval: `decisions()` and `commit()`
return `InvalidState` until an ordinary preview succeeds. Calls during a committed
interval are rejected without disturbing that interval's decisions.

Committed musical state, pending/terminal receipts, journal events, dropped-event
counts, frame and topology epoch remain unchanged. Repeat probes cannot consume a
countdown, apply a request, advance a pass count or publish an event twice. A later
ordinary preview starts from committed inputs again. Existing decision borrows
cannot cross this mutable call.

The work is a finite prefix of the existing component settlement, with the same
per-source clock coalescing and prepared bounds. No allocations, destruction,
producer waits, propagation quota or fixed time quantum are added. This is not a
cheap constant-time query or a host deadline guarantee. It belongs to the single
render-owned resolver, not a second control-side musical clock.

## Evidence and limitations

Three guarded integration cases plus one private scratch-state case verify:

- 128-frame regular/explicit scalar clocks, whole/single/irregular partitions,
  declaration reversal, journal capacities 0/1/1024 and all request slots retained;
- full decisions/journals/loss counters against execution without probes;
- eleven same-frame Stop/seek/restart/reactive/cancel/refusal/source-stop cases;
- nested request-family refusal, upstream capture-start refusal, stale/primitive
  targets, duplicate observations, committed-phase protection and epoch changes;
- no old child or trigger intents at the probe cut, including recording actions,
  and no publication of provisional request outcomes.

See [program-probe-stage6.json](../../../../docs/loop_layer/evidence/program-probe-stage6.json)
for the original native/release, Node/Chromium and workspace evidence. That
milestone deferred interpreter execution; consolidated Stage 6 evidence is recorded
separately in repository [Miri jobs](../../../../docs/loop_layer/MIRI_JOBS.md), not
rerun for these prose corrections.

The standalone [owning program transaction](program-changes.md) and facade
ProgramActivate/ProgramGraphActivate use actual zero/stopped cuts for running
changes. Immediate replace_topology/TopologyActivate still require changed programs
already stopped. See [publication paths](topology-activation.md) and the runnable
[program/graph example](../examples/program_graph_cut.rs); independent loop hosts
and deadline evidence are not supplied by these tests.
