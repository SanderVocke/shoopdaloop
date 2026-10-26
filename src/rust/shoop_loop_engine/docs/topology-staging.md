# Prepared topology ingestion and recycling

`topology::staging::{Stager, StageId, Status, Progress, RECORD_BYTES}` receives a
prepared timing representation without compiling source sections or allocating
on the callback. It is portable native/Wasm code with no browser dependency.

**Ready means timing metadata is validated.** It does not certify runtime graph
bindings, media/resource compatibility, current activation epochs or historical
loop-generation watermarks. The facade does not yet expose live activation using
this receiver. Do not bypass that missing transaction with unrestricted mutation
of a running facade.

## Preparation and lifecycle

`Stager::prepare(limits)` allocates and pre-touches seven maximum-capacity metadata
vectors and fixed validation scratch. Their lengths may change, but callback
operations cannot increase their capacity. Scope allocation is preparation-only.
`required_storage_bytes(limits)` checks the combined metadata/scratch/header size
against `i32::MAX` before allocation. Allocator overhead and separately retained
owners are not included.

The lifecycle is:

1. **Idle → Receiving/Validating:** `begin(&header)` checks format, exact configured
   limits and declared counts, clears prior contents without freeing allocations,
   and returns an endpoint-scoped `StageId`. An empty image goes directly to
   Validating, not Ready.
2. **Receiving:** `append(id, first_record, bytes, record_budget)` accepts ordered
   complete records. `first_record` must equal the acknowledged received count.
   The byte length must be divisible by `RECORD_BYTES`; the record count must fit
   both the supplied budget and the remaining declared records.
3. **Validating:** after all records arrive, `validate(id, step_budget)` advances
   a finite validation state machine and returns used steps, cumulative saturating
   diagnostic steps and readiness. Budget zero does no work. No call waits for
   another message or polls a producer.
4. **Ready:** `view(id)` lends the immutable topology. Repeating validation uses
   zero steps. `take(id)` transfers the whole owning topology and leaves Taken.
5. **Taken:** `recycle(topology)` accepts compatible fully provisioned storage and
   returns to Idle. Wrong state, limits or insufficient capacities return
   `(error, topology)` without consuming/destroying that owner.

`cancel(id)` clears Receiving, Validating, Ready or Failed contents and returns to
Idle without deallocation. It cannot cancel an extracted owner in Taken, and is
not an idempotent success in Idle. A successful new begin advances generation;
stale/foreign tokens cannot address the new attempt. The final nonzero generation
can be used, but the following begin fails Exhausted rather than wrapping.

Syntax, offset, scope and budget failures leave a whole append chunk unconsumed.
The receiver checks every record's syntax before appending any record from that
chunk. Cross-record semantic errors instead latch `Failed(error)` during
validation; view/take remain unavailable until cancellation and a new begin.
Neither incomplete nor failed metadata is exposed as a valid `PreparedTopology`.

Copying a token or view acquires no owner. Rust rejects cancellation/extraction
while a later-used receiver view is borrowed, and rejects moving an owner into
recycling while its data remain borrowed. The production-type
[lifetime prototype](../../../../docs/loop_layer/prototypes/topology_staging_lifetime.rs)
contains positive use and deliberate E0502/E0505 checks. Its full preparation loop
is off-render; it is not an example of replenishing callback budgets.

Two receivers with identical limits may exchange compatible returned owners.
Ordinary `compile` output is sized to content and usually cannot replenish a
maximum-capacity receiver; failed recycling returns it unchanged. Reserve a
compatible bank or reliable owning-return location before activation. Actual
metadata/scratch destruction, including destruction of a Taken receiver's scratch,
is off-render. Recycling retained allocations is not a destructor exemption.

## Wire representation: LTP1

`PreparedTopology::encode_header(&mut [u8; 64])`, `encoded_records()` and
`encode_record(index, &mut [u8; 64])` are allocation-free readers. An out-of-range
record index leaves the output unchanged. All records are exactly 64 bytes,
integers are little-endian, and unused bytes must be zero. No Rust layout, pointer,
`usize` or shared-memory address is transferred.

Header fields:

| Offset | Field |
| ---: | --- |
| 0 | Four bytes `LTP1` |
| 4 | Total body record count, u32 |
| 8,12,16,20,24,28 | u32 limits: loop slots, relations, program entries, program segments, pending requests, journal events |
| 32,36,40,44,48,52,56 | u32 counts: nodes, triggers, programs, targets, segments, components, members |
| 60 | Deduplicated timing-relation count, u32 |

Body tables follow that count order. Record kind is determined by the table,
not an unchecked caller tag. Relation count deduplicates source/target/action-versus-sync
triples; it does not remove trigger records. A loop ID is two u32 words: slot then generation.
A range is two u32 words: start then length.

| Record | Fields and byte offsets |
| --- | --- |
| Node | ID0; kind8 (primitive0/composite1); sync-present9 (0/1); length16 (u64); optional source ID24 |
| Trigger | source ID0; target ID8; mode16 |
| Program | source ID0; kind8 (regular0/explicit1); nominal unit16 (u64); iterations24 (u64); target range32; segment range40 |
| Target | target ID0; program-relative segment range8 |
| Segment | start0, end8, occurrence-start16 (u64); ordinal24 (u32); mode28 |
| Component | member range0 |
| Member | ID0 |

Modes encode Stopped0, Playing1, Recording2. Reserved bytes, unknown enum values
and zero ID generations are refused. The header deliberately includes no native
pointer and no activation epoch. A host/control transport must supply its own
endpoint routing, StageId, record offset, finite payload ownership and delivery
acknowledgments. This metadata codec is not a complete controller/AudioWorklet
transport envelope or a content-authentication protocol.

The representation is a prepared timing IR. Validation checks its supported
structural semantics; it does not reconstruct original source sections or editing
intent. Native compilation produces the same canonical bytes checked by the
independent golden and native/Node/Chromium roundtrip tests.

## Incremental validation and work

The validator checks:

- Sorted unique node slots, exact-generation references and configured capacities.
- One program per composite, ordered source IDs, complete contiguous table ranges,
  checked nominal duration and agreement with composite node length.
- Sorted target identities, sparse non-overlapping nonempty segments, half-open
  bounds, occurrence starts, canonical contiguous-mode merging, ordinal ownership/
  mode consistency and entry-count lower bounds.
- Regular versus explicit modes, including refusal of composite recording.
- Complete unique ordered component membership and nonempty components.
- Recursive programs using a prepared queue and indegrees.
- Deduplicated sync/action relations and their declared count; no internal action
  edge in a sync component and no backward component dependency.
- Strong pure-sync components and canonical minimum-ready-ID dependency order.

A multi-node pure-sync component has one sync source per node and no internal
action edge. Following those sources must visit exactly its declared members and
return to the first. A falsely merged cycle-plus-downstream-tree therefore fails;
checking only that members have some incoming edge would be insufficient.

Each validation step handles constant-sized metadata; cursors, queue positions
and finite searches advance monotonically. No musical duration is enumerated.
The implementation deliberately uses simple fixed scratch: duplicate-edge searches
and ready-component/outgoing-edge scans give worst-case total work
`O(rows + relations² + components² + components × relations)`. Memory remains
linear in configured counts. This is not a linear-time preparation claim or a
universal latency/deadline guarantee.

The caller owns the background budget. Pass the **remaining callback allowance**
and subtract `Progress.used`; do not replenish it at every internal musical
interval or retry until Ready on the callback. An error has no Progress tuple:
conservatively charge the offered allowance on error rather than reusing it for
another receiver. The same conservative policy applies to an append syntax error
after inspecting part of a chunk. Begin/reset and owning lifecycle operations are
separate finite, prepared-size work, not included in the validate step count. Admission/validation may pause
between calls, while already admitted music continues independently. Configuration
readiness is distinct from a promise to activate at a musical frame. Diagnostic
step saturation neither drops validation nor declares a candidate ready.

Append performs two bounded syntax/canonicalization passes; the second appends
typed records into prepared vectors. It is not a zero-copy path. Its budget counts
records, not actual machine instructions or all transport copies. Preparation,
encoding, ingress, validation, extraction and recycling have different ownership
and accounting responsibilities; the eventual facade/host adapter must preserve
those distinctions.

## Evidence and remaining integration

Guarded tests include an independent exact byte golden, chunk/budget/scope errors,
malformed semantic records, full storage reuse, 512 sync/action graphs against
transitive reachability, 128 nested regular/explicit images against raw entries,
`u64::MAX` sparse bounds, 128 standalone-resolver owning replacements, and clock
progress while another candidate is incomplete/failed. Scope/generation exhaustion
and production-type borrow/move refusals are also checked.

These results do not implement generation/epoch/resource/view-binding preflight
for the runtime facade, reliable candidate-operation receipts, cancellation tied
to those receipts, atomic graph/domain activation, or next-iteration-zero running
program changes. The existing standalone resolver still safely refuses changed
running programs. Those remain Stage 6 work; the actual loop AudioWorklet remains
Stage 7 work.
