# Apply: coordinated stopped-owner configuration activation

Implementation parent: `f510466d`. This is a scoped implementation milestone, not a claim that
Stage 6 or final documentation acceptance is complete. Reviewed destinations:
`src/rust/shoop_loop_engine/docs/runtime.md` and `docs/topology-activation.md`.
Their hashes are recorded in `configuration-baseline.json`. Re-read any changed
destination before applying this note. Preserve the separate documentation worker's
book/archive changes. Do not copy this work log into the teaching chapters.

## Correct existing claims

Combine this slice with `loop-facade-live-graphs.md`: the Controller can compile,
stage, activate and release prepared signal graphs. Fixed-role routing, stopped
paired graph/timing replacement, fixed-graph running-program replacement, and the
new stopped-owner configuration transaction are **distinct** interfaces. Remove
blanket claims that live graphs and all generation changes are still unavailable.
Do not imply that running-program and graph publication are now combined.

In runtime.md, extend preparation/ownership, commands/recovery, boundary sequencing,
exports and shutdown. In topology-activation.md, retain the existing fixed-identity
paths, but add a separate task-led section for changing logical owners over prepared
physical templates. Update the title/navigation if necessary. README/architecture,
media guide and book chapters 3–6, 8–10, 12–19 need cross-reference reconciliation;
this note does not certify those destinations as fully reviewed.

## Teach a complete workflow

1. Prepare with `Engine::prepare_with_configuration_staging`, supplying the ordinary
   facade arguments, `TopologySettings`, `RoleSettings`, then optional observer.
   `RoleSettings { candidates, sources, source_packets_per_callback }` reserves
   edit banks and source slots. All zero disables it; source slots and their packet
   allowance must either both be zero or both be positive. Timing-only role edits
   can use zero sources. Enable generic graph banks separately with
   `prepare_graph_staging` before rendering.
2. Supply physical `Binding` templates off-render. Every active primitive still
   needs exactly one playback/capture pair. Additional unassigned templates may
   occupy slots not initially used by primitives, and must be outside the initial
   graph. Their node IDs, signatures, channel/MIDI shape, smoothing and capture
   groups remain prepared/fixed. This is not arbitrary callback processor creation,
   destruction or physical NodeId generation replacement.
3. Acquire local source leases with `RoleSnapshot { resource }`. Its reply includes
   scoped `RoleSourceId`, immutable snapshot metadata and status. Audio is Ready;
   indexed MIDI starts Validating. Repeated `RoleSourceValidate` calls eventually
   return Ready with maximum packet size. Validation scans packet headers with a
   callback-shared packet allowance (including EOF), across all sources and internal
   intervals; it does not chase a producer. Source snapshot acquisition uses existing
   resource budgets. Indexed MIDI is required; validation failure stays latched
   until release. `RoleSourceStatus` observes it, `RoleSourceRelease` releases it.
4. `RoleBegin { slot, changes }` returns a scoped `RoleStageId`. Append unique-slot
   edits with `RoleAppend { id, previous, initial }`. `RoleInitial` contains target,
   frame length, optional audio/MIDI source IDs, finite gain and `preserve_input`.
   `previous: None` adds an unassigned primitive; `initial: None` removes one;
   replacement stays in the same slot and uses a strictly newer logical generation.
   Initial revision is zero. Shape/duration/index/packet capacity and current owner
   are checked before transferring source leases. Successful append invalidates its
   source IDs and transfers them into the candidate. Rejection transfers nothing.
   A source ID is not a pointer, wire address or cross-instance authorization.
5. Independently prepare matching timing and graph candidates. The graph compiler
   uses a retained catalog of all physical templates and includes only bindings for
   consumers present in the requested graph. Generic validity alone does not prove
   domain validity: every resulting primitive needs exactly its prepared pair and
   decision slot. Resulting primitive IDs/lengths must match initial media edits.
6. Submit `ConfigurationActivate { roles, topology, graph, graph_epoch,
   topology_epoch }` at the desired boundary. Old edited owners and changed programs
   must **already be stopped at delivery**. A Stop queued for that same frame is not
   sufficient. Capture rows must have no active/prepared/interval/frozen owner and
   their prepared groups must be idle. Same-frame Play/Record after publication
   settles against the new configuration; State/Export sees the post-settlement cut.
7. Check the reliable typed `ConfigurationActivated` reply and resulting epochs,
   separately from admission. Failed preflight leaves all candidates Ready, including
   failure in the final resolver compatibility check. Graph and media permits are
   held across that final fallible publication, followed by infallible media/graph
   commits. There is no partial owner batch or speculative retirement-credit reuse.
8. Recycle Taken role metadata with `RoleRecycle`; separately recycle the displaced
   topology and release the displaced graph. `RoleCancel` cancels Receiving/Ready
   candidates and releases their sources, not an already published configuration.
   Role cancellation and source release are payload-free recovery-safe operations.
   Other new operations are normal-lane only and reject unexpected payloads. Recovery
   still has finite owning-result credit: consume its result before reusing it.

## Required semantics and caveats

- `preserve_input` explicitly retains the existing physical capture projection. It
  is invalid for a previously unassigned row. Otherwise reset occurs before the
  first new capture reservation and before the consumer processes that interval;
  it is cleared after finish, not repeated each interval. Initial MIDI projection
  is strictly before F; events at F remain captured originals. This policy does not
  synthesize cleanup messages or reconcile arbitrary downstream synth/FX voices.
- Old media residency, exports and pending pins retain their original logical owner,
  revision and capture cuts. Replacement cannot retarget old requests merely because
  a slot is reused. Pending requests invalidated by topology publication fail with
  their own terminal results; retained source content remains readable until explicit
  release. New recording revision numbering belongs to the new generation.
- Banks reserve up to loop-slot-count edits. Storage sizing checks the aggregate
  bank vectors and source slots against signed linear-memory bounds before allocation.
  Source/role IDs never wrap. Preflight scans bounded prepared tables and sources;
  packet validation has a separate allowance, not a universal CPU-time budget.
  Metadata capacity must cover every new initial owner before publication, without
  counting old references that might later retire. Expensive valid work may xrun.
- Disconnect does not Stop or undo admitted publication. `Stopped::cleanup` reclaims
  validating sources, partial/Ready edit banks and Taken banks, plus ordinary ledger
  and capture claims, including publication followed by a render fault before finish.
  It is permanently quiescent/idempotent. Imported resources remain caller-owned.

## Example and evidence instructions

Use `tests/support/configuration_scenarios.rs` and its four sibling modules to make
an example-led recipe: source preparation → Ready banks → rejected running cut →
Stop at an earlier frame → atomic generation/route change → recapture → old/new
export reads → independent retirement. Preserve exact MIDI ordering: restoration
notes belong to their restored channel, whereas source packets follow restoration.
Include expected output and ownership checks rather than copying the test harness.

`docs/loop_layer/evidence/configuration-stage6.json` records 19 scoped gates:
148 loop tests in native debug/release, Node and Chromium; 2136 workspace passes,
four existing skips; warning-denied build/rustdoc/examples; boundaries/policies and
14 Miri-helper tests. Eight refreshed positive lifetime prototypes and 23 expected
compiler refusals are in `target/loop-layer-configuration-stage6/proofs.json`.
The new seven integration cases and private exhaustion/failure case are registered
for Miri: **223 = 128 runtime + 95 loop**, not interpreted in this slice. Package
Wasm tests do not substitute for the required actual loop AudioWorklet.

Preserve failed fixture evidence: range-token syntax; restored-note channel order;
cumulative dropped-journal count; attempted reuse of the single unread recovery
receipt; topology-scope error's Timeline classification; zero-callback diagnostic
epoch zero; and a two-slot store's two (not sixteen) snapshot credits. Logs remain
under `target/loop-layer-configuration-stage6/`. No workloads/assertions were relaxed.
Full earlier-stage backfill, broader lifecycle stress, combined running-program/graph
publication, consolidated Stage 6 Miri, independent hosts and final delivery remain
open. A separate documentation worker must apply and verify the final prose.
