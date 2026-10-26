# Compiled topology storage and borrowed views

`compile(limits, &description)` is allocating **off-render preparation**. Its
`PreparedTopology` result owns seven flat arrays: nodes, triggers, program records,
target records, segments, component ranges and component members. Compiled output
has no separately allocated target list per program, segment list per target, or
membership list per component. Compiler scratch may still use temporary nested
collections; compilation is not a callback operation.

[Topology staging](topology-staging.md) now adds a canonical native/Wasm encoding,
reusable maximum-capacity receiver storage, incremental validation and owning
extraction/recycling. The facade's separate [fixed-graph activation path](topology-activation.md)
adds identity checks and receipts while preserving existing roles/resources.

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
- `components()` returns a `Components<'_>` view in canonical dependency order.
  Use `len`, `is_empty`, `get`, `last`, indexing, or iteration. Members are slices
  of `LoopId`, not individual boxed allocations. Indexing panics out of range;
  checked `get` returns None.
- Target/component iterators are cloneable, exact-sized and double-ended. They
  traverse prepared arrays without allocating.

Copying/cloning a view **does not acquire an independent owner**. Segment/member
borrows cannot be used after replacing or dropping their topology. Local copies
of scalar metadata are values, not configuration edits. No reference-counted
owner, self-referential structure, raw pointer cast or dynamically allocated
reference list is involved.

The production-type lifetime prototype at
[`topology_view_lifetime.rs`](../../../../docs/loop_layer/prototypes/topology_view_lifetime.rs)
checks a usable positive path plus deliberate E0502 refusals for retaining a
program, target or component borrow across resolver topology replacement.

## Identity and ordering

Compilation remains canonical under supported declaration reordering. Program
records address the global flat arrays, but each target's segment range is
relative to that program's own segment slice. Consequently comparing two program
views compares only that program's metadata and content. Inserting an unrelated
program earlier in the global tables does not falsely change the identity of an
unchanged running program.

Segments retain their existing mode, ordinal, occurrence-start and half-open
interval semantics, including overlap resolution and contiguous continuation.
Pure-sync SCC membership and canonical dependency order are unchanged. Neither
flat storage nor nominal duration creates a second musical clock.

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

For L nodes, T triggers, P programs, Q target timelines, S segments and C sync
components, actual storage is:

- the fixed `PreparedTopology` header;
- L node records and L component-member IDs;
- T trigger records;
- P program records;
- Q target records;
- S segment records;
- C component ranges.

P and C are bounded by loop slots; Q by total expanded program entries; S by the
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

The runtime facade does not expose unrestricted mutation through this standalone
API. Its staged fixed-graph path preserves exact logical identities and media-role
bindings; iteration-zero running changes and general graph/role-generation
transactions remain Stage 6 work. Read-only flat views alone do not substitute
for those transactions or for reliable owning transport.
