# Compiled topology storage and borrowed views

`compile(limits, &description)` is allocating **off-render preparation**. Its
`PreparedTopology` result owns six flat arrays: nodes, triggers, program records,
target records, segments and a source-before-dependent loop order. Compiled output
has no separately allocated target list per program or segment list per target. Compiler scratch may still use temporary nested
collections; compilation is not a callback operation.

[Topology staging](topology-staging.md) now adds a canonical native/Wasm encoding,
reusable maximum-capacity receiver storage, incremental validation and owning
extraction/recycling. The planner's separate [publication paths](topology-activation.md) add identity,
node pair/media compatibility and epoch checks. Timing Ready alone grants no graph or
logical-owner authority.

## Lookup and lifetime contract

- `nodes()`, `triggers()` and checked `node(id)` retain their existing interfaces.
- `program(id)` returns `Option<CompiledProgram<'_>>`. Its scalar metadata is
  copied; its target and segment data borrow the topology.
- `program.target(id)` performs binary lookup and returns a borrowed
  `TargetTimeline<'_>` value. `target.segments()` is a borrowed slice;
  `target.at(iteration)` performs sparse half-open indexed lookup and returns a
  copied `Segment` or `None`. It does not walk preceding history.
- `program.targets()` returns a `TargetTimelines<'_>` view. Use `len`, `is_empty`,
  `get(index)` or `iter`/`IntoIterator`. Unlike the former owned-target slice,
  indexing with `targets[index]` is not provided. Out-of-range `get` returns None.
- `dependency_order()` returns a borrowed `&[LoopId]` in canonical
  source-before-dependent order. Each loop occurs exactly once.
- Target iterators and the dependency-order slice traverse prepared arrays
  without allocating.

Copying/cloning a view **does not acquire an independent owner**. Segment/order
borrows cannot be used after replacing or dropping their topology. Local copies
of scalar metadata are values, not configuration edits. No reference-counted
owner, self-referential structure, raw pointer cast or dynamically allocated
reference list is involved.

The production-type lifetime prototype at
[`topology_view_lifetime.rs`](../../../../docs/loop_layer/prototypes/topology_view_lifetime.rs)
checks a usable positive path plus deliberate E0502 refusals for retaining a
program, target or dependency-order borrow across resolver topology replacement.

## Identity and ordering

Compilation remains canonical under supported declaration reordering. Program
records address the global flat arrays, but each target's segment range is
relative to that program's own segment slice. Consequently comparing two program
views compares only that program's metadata and content. Inserting an unrelated
program earlier in the global tables does not falsely change the identity of an
unchanged running program.

Segments retain their existing mode, ordinal, occurrence-start and half-open
interval semantics, including overlap resolution and contiguous continuation.
Sync links, triggers and program actions must form an acyclic dependency graph
between distinct loops. Compilation rejects cycles with
`InstantaneousDependencyCycle`. Self-sync remains valid and adds no ordering edge;
a self-targeted state-changing action remains invalid. Among ready loops, the
smallest ID is chosen first, making ordering independent of declaration order.
Each loop settles once and observes already-delivered upstream pulses. Self-sync
reacts once after its own pulse is delivered. Flat storage and nominal duration
do not create a second musical clock.

## Capacity accounting

`PreparedTopology::storage_bytes()` reports its header plus the actual owned
flat-array bytes. It excludes allocator overhead and temporary compiler scratch.
Control-side compilation sizes arrays to content; its empty arrays do not require
separate allocations. Receiver-originated topologies retain their maximum prepared
capacities even when logical tables are empty, and the byte count includes those
capacities.

`Limits::topology_storage_bytes()` reports the checked maximum compiled metadata
size at the declared limits. This is a representability calculation, not full
logical configuration validation; use `Limits::validate()` as well. Validation
now checks the **aggregate** compiled-metadata maximum against `i32::MAX`, in
addition to the existing individual-array and resolver-storage checks. A formerly
individually representable configuration may therefore be refused if its combined
maximum is too large, even when a particular small description would fit.

For L nodes, T triggers, P programs, Q target timelines and S segments,
actual storage is:

- the fixed `PreparedTopology` header;
- L node records and L dependency-order IDs;
- T trigger records;
- P program records;
- Q target records;
- S segment records.

P is bounded by loop slots; Q by total expanded program entries; S by the
segment limit; T by the relation limit. Storage is linear in these **total**
counts, not programs × targets × maximum segments. It is independent of musical
duration: a one-billion-tick and a `u64::MAX`-tick sparse program can occupy the
same bytes. Native/Wasm element widths may differ; use the sizing methods rather
than assuming a wire layout. Internal `usize` ranges are **not** a wire format.

These byte estimates do not include resolver state, media pools, retired owners,
additional candidate banks or receiver validation scratch. Use
`Stager::required_storage_bytes` for a fully provisioned receiver, and account for
separately retained owners without double-counting exchanged allocations. They are not a
callback-time or CPU deadline guarantee.

## Replacement remains an owning operation

Standalone `Resolver::replace_topology` still checks the expected epoch,
generations, removed-loop state and pending requests. It returns the displaced
owner on success and the candidate owner on failure. Destroy either off-render.
An unrelated table relocation preserves an unchanged running program; a genuinely
changed running program is still safely refused. Stopped replacement resets its
cursor and nominal duration under the existing rules.

The loop planner does not expose unrestricted mutation through this standalone
API. Fixed-graph timing/program changes, fixed node-pair live routes, stopped-owner
configuration and paired running-program/graph changes have distinct preconditions
and owning returns. The latter still preserves logical IDs and physical node pairs;
generation edits use already-stopped ConfigurationActivate. None constructs new
physical processors or resizes ports. Read-only views alone do not substitute
for those transactions or reliable owning transport.
