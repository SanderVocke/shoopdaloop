# Composite Schedule Remodel: End-to-End Plan

## Implementation Status

- [x] Replace persisted parallel playlists with positioned loop instances.
- [x] Migrate version-three playlist documents to stable instance IDs and absolute cycle positions.
- [x] Lower positioned instances to the existing backend timeline transport.
- [x] Address editor operations and intents by stable instance ID.
- [x] Make repeated occurrences independently resizable and mode-editable.
- [x] Replace playlist-specific deletion compensation with direct instance removal.
- [x] Update application, API, editor, session, and migration tests.
- [ ] Complete workspace-wide validation and browser checks.

This status is updated as implementation proceeds; the detailed plan and acceptance criteria remain below.

## 1. Goal

Replace the front-end/session representation of a composite loop from:

```rust
Vec<playlist<
    Vec<section<
        Vec<parallel event>
    >>
>>
```

with a representation matching the editor:

```rust
CompositeDocument {
    kind,
    instances: Vec<CompositeLoopInstanceDocument>,
}

CompositeLoopInstanceDocument {
    id,
    loop_id,
    start_cycle,
    n_cycles,
    mode,
}
```

Today, `CompositeDocument::playlists` encodes placement indirectly through playlist origins, serial sections, parallel entries, and per-entry delay. 【F:src/rust/shoop_session/src/document.rs†L305-L326】 The editor then has to reverse that structure into absolute `start_frame`/`end_frame` values. 【F:src/rust/shoop_app/src/lib.rs†L7978-L8037】

The new model should make every visible block a first-class object with:

- A **stable instance ID**.
- The referenced source loop ID.
- An **absolute start cycle**.
- An optional forced cycle count.
- An optional script-mode override.

“Stable” is important: using the vector index as the ID would recreate the current problem in a different form because deletion and insertion would change the identity of later blocks.

## 2. Immutable Acceptance Criteria

1. Every block shown in the composite editor corresponds to exactly one stored instance.
2. An instance’s horizontal position is stored directly as an absolute cycle count.
3. Multiple occurrences of the same source loop can be edited independently.
4. Overlap is represented only by intersecting absolute time ranges—not by playlist or parallel-section membership.
5. Adding, deleting, moving, duplicating, resizing, and changing script mode operate by stable instance ID.
6. Saving and reopening a session preserves instance IDs, positions, lengths, modes, and composite kind.
7. Existing sessions using `playlists` migrate without changing their effective schedule.
8. The audio engine continues receiving an equivalent compiled schedule.
9. Native and browser/worklet backends remain behaviorally equivalent.
10. Failed backend reconfiguration remains transactional: the front-end document must not change if the backend rejects the new schedule.

## 3. Design Boundary

This should be a **front-end and persisted-session remodel**, not necessarily an engine remodel.

The backend interface still accepts nested timelines, sections, and entries through `BackendCompositeConfig::timelines`. 【F:src/rust/shoop_backend/src/lib.rs†L451-L461】 The native backend translates that directly into engine timelines and sections. 【F:src/rust/shoop_backend/src/native.rs†L462-L504】 The engine compiler already reduces those structures to scheduled occurrences with absolute starts and ends. 【F:src/rust/shoop_engine/src/composite_plan.rs†L348-L387】

Therefore:

- **Canonical application/session model:** flat positioned instances.
- **Backend representation:** keep the existing nested transport model initially.
- **Compiler/adapter:** lower flat instances into backend timelines immediately before configuration.

A simple lowering is one backend timeline containing one section whose entries all have `delay = start_cycle`. Another safe option is one timeline/section per instance. The former is more compact, but it needs tests proving that all engine semantics—including repeated references to the same child—remain equivalent.

## 4. Required Changes

### Stage 1 — Introduce the canonical session model

- [ ] Replace `CompositeDocument::playlists` with `instances`.
- [ ] Rename `CompositeEventDocument` to something such as `CompositeLoopInstanceDocument`.
- [ ] Add a persistent `instance_id: u64`.
- [ ] Rename `delay` to `start_cycle` and define it as absolute from the composite timeline origin.
- [ ] Retain `loop_id`, `n_cycles`, and `mode`.
- [ ] Decide whether IDs are unique within a composite or globally; per-composite uniqueness is sufficient and easier to validate.
- [ ] Add validation for:
  - Duplicate instance IDs.
  - Zero forced cycle counts.
  - Stale source loop IDs.
  - Invalid modes for the composite kind.
  - Cycle-count/range overflow.

The existing session validator only traverses nested playlists to detect stale loop references, so that traversal must change to `instances`. 【F:src/rust/shoop_session/src/archive.rs†L850-L871】

**Verification**

- Unit-test direct serialization/deserialization of empty, regular, script, overlapping, repeated-source, and nested-composite schedules.
- Verify deterministic archive output remains deterministic.

### Stage 2 — Add backward-compatible session migration

The session document version is currently `3`, and the archive loader accepts older versions before applying targeted migrations. 【F:src/rust/shoop_session/src/document.rs†L8-L14】【F:src/rust/shoop_session/src/archive.rs†L214-L250】

- [ ] Increment `SESSION_DOCUMENT_VERSION`.
- [ ] Add a legacy composite DTO containing `playlists`.
- [ ] For every old playlist:
  1. Start its section origin at cycle zero.
  2. Convert each event to an absolute start:
     `absolute_start = section_origin + event.delay`.
  3. Calculate the event duration using the same natural/forced-cycle rules as the current details projection.
  4. Advance the section origin by the maximum `delay + duration` among its parallel entries.
  5. Emit one new instance per legacy event.
- [ ] Generate deterministic stable IDs during migration, for example in legacy traversal order.
- [ ] Preserve all parallel playlists as overlapping instances; do not concatenate them.
- [ ] Define migration behavior for stale source loops and arithmetic overflow—prefer rejecting the archive rather than silently changing timing.
- [ ] Save only the new format after loading an old session.

The existing application projection computes natural duration by rounding each source length up to a synchronization cycle, then overrides that with `n_cycles` when supplied. Migration must share this calculation rather than reimplementing it differently. 【F:src/rust/shoop_app/src/lib.rs†L7991-L8025】

**Verification**

- Load a legacy archive with multiple playlists, serial sections, delayed entries, forced lengths, modes, and duplicate source references.
- Compare its pre-migration effective `(loop, start, end, mode)` schedule with the migrated schedule.
- Save and reopen it as the current document version.
- Confirm migration is deterministic byte-for-byte when given identical input.

### Stage 3 — Refactor the application model and schedule projection

Currently `LoopModel` keeps both the full composite document and a secondary `script_composition: Vec<Vec<LoopId>>`. 【F:src/rust/shoop_app/src/lib.rs†L872-L881】 Session loading reconstructs that secondary structure from only the first playlist, so it cannot faithfully represent arbitrary positioned schedules. 【F:src/rust/shoop_app/src/lib.rs†L7641-L7656】

- [ ] Make `CompositeDocument::instances` the sole authoritative application schedule.
- [ ] Remove or replace `script_composition`.
- [ ] If non-composite-capable backends must still be supported, replace the serial-section fallback with an absolute-cycle occurrence scheduler derived from `instances`.
- [ ] Centralize calculations for:
  - Natural duration in cycles.
  - Effective forced duration.
  - End cycle.
  - Composite total length.
  - Dependency/source signature.
- [ ] Rewrite `composite_details_snapshot` as a straightforward projection:
  - `start_frame = start_cycle * sync_length`
  - `end_frame = start_frame + effective_duration`
  - no playlist or section accumulation.
- [ ] Rewrite dependency traversal and backend-length signatures to iterate over instances.
- [ ] Keep nested-composite cycle detection and restore ordering intact.
- [ ] Decide whether a source loop length change affects an instance with no forced length; if it does, retain the current backend signature refresh behavior.

The current dependency signature collects referenced loop IDs through all playlist dimensions and combines them with source lengths. 【F:src/rust/shoop_app/src/lib.rs†L4879-L4898】 That logic remains necessary, but its input becomes the flat instance list.

**Verification**

- Projection tests cover unordered stored instances, overlaps, gaps, repeated source loops, nested composites, and overflow boundaries.
- Changing a source loop’s natural length refreshes the backend plan and editor end position.
- Reordering the `instances` vector does not alter schedule timing.

### Stage 4 — Add a flat-to-backend compiler

`backend_composite_config` currently maps the persisted playlists almost one-to-one into backend timelines. 【F:src/rust/shoop_app/src/lib.rs†L4940-L5008】

- [ ] Replace it with a compiler from flat instances to `BackendCompositeConfig`.
- [ ] Resolve primitive versus nested-composite targets as today.
- [ ] Convert `start_cycle` to backend entry delay.
- [ ] Preserve forced cycle counts and script modes.
- [ ] Sort only if deterministic output requires it; do not use vector order as timing semantics.
- [ ] Ensure empty composites produce the existing intended “no backend composite” state.
- [ ] Keep backend creation/reconfiguration transactional.
- [ ] Document that playlists/sections below this boundary are an engine transport detail rather than the canonical user model.

The engine already checks negative delay, entry limits, arithmetic overflow, and source metadata while compiling occurrences. 【F:src/rust/shoop_engine/src/composite_plan.rs†L348-L385】 Front-end validation should catch user/document errors earlier, while engine validation remains the final safety boundary.

**Verification**

- Golden tests compare the backend occurrences compiled from:
  - A migrated legacy schedule.
  - The equivalent new flat schedule.
- Run the same cases through native and worklet backend adapters.
- Retain failure-injection coverage proving backend rejection does not commit the application document.

### Stage 5 — Replace positional event identity in the application API

The public UI API currently identifies a block by the structural triple:

```rust
playlist_index
section_index
parallel_index
```

【F:src/rust/shoop_app_api/src/lib.rs†L929-L951】

- [ ] Replace `CompositeEventId` with a stable instance ID, or make it wrap one.
- [ ] Replace the three structural index fields in `CompositeEventDetailsState` with `instance_id`.
- [ ] Change delete, relocate, duplicate, resize, and mode intents to refer to instance IDs.
- [ ] Change forced-length editing to target one instance, not a source loop.

The last item fixes an existing ambiguity: `set_composite_loop_cycles` currently finds a source loop ID and changes **every** occurrence of that source. 【F:src/rust/shoop_app/src/lib.rs†L5356-L5389】 With direct instances, the context menu for one block must resize only that block.

Existing intents already provide most required operations—compose at a position, delete a group, relocate/duplicate a group, resize, and change mode—but their identifiers and resize target need changing. 【F:src/rust/shoop_app_api/src/lib.rs†L1613-L1643】

**Verification**

- API equality/debug/intent-name tests use stable instance IDs.
- Two instances of the same source can be resized or mode-edited independently.
- Stale IDs produce an error and no mutation.
- Multi-selection retains identity after deleting or inserting unrelated instances.

### Stage 6 — Simplify editor state and mutations

The editor duplicates the structural identity triple in `CompositeEventKey` and uses it for selection, drag payloads, context menus, and widget interaction IDs. 【F:src/rust/shoop_egui/src/composite_loop_widget.rs†L20-L55】

- [ ] Replace `CompositeEventKey` with the stable instance ID.
- [ ] Store `BTreeSet<InstanceId>` for selection.
- [ ] Send instance IDs in drag, delete, resize, and mode-change intents.
- [ ] Keep swimlane packing purely visual; overlapping blocks continue to be assigned lanes based on `start_frame` and `end_frame`.
- [ ] Remove playlist/section/parallel indices from visual sorting except for an optional final deterministic tie-breaker using instance ID.
- [ ] Ensure duplicate creates new IDs while move preserves IDs.
- [ ] Ensure save/reload preserves selection targets if editor state is retained across reloads; otherwise explicitly clear selection.
- [ ] Update terminology and comments so “parallel playlist” no longer appears in front-end concepts.

Swimlane packing is already based primarily on absolute ranges, so it does not need a conceptual redesign; only its structural tie-breakers need replacing. 【F:src/rust/shoop_egui/src/composite_loop_widget.rs†L82-L125】

**Verification**

- Clicking, box-selecting, deleting, dragging, Ctrl-duplicating, resizing, and script mode editing all assert stable instance IDs.
- Duplicate-source blocks remain independently selectable.
- Equal-start/equal-end blocks pack deterministically by instance ID.
- Screenshots or egui paint snapshots confirm no visual regression.

### Stage 7 — Update composition commands and legacy gestures

The existing “append serial or parallel” helper mutates playlist sections directly. 【F:src/rust/shoop_app/src/lib.rs†L884-L918】

- [ ] Redefine “append to end” as:
  - Find the maximum end cycle in the current schedule.
  - Insert each serial source at the current end and advance it.
- [ ] If the legacy “parallel with final section” gesture remains:
  - Define it in editor terms, such as “place at the start cycle of the latest-ending block/group.”
  - Avoid reconstructing section membership.
- [ ] Prefer eventually removing the `parallel: bool` control-operation parameter and routing all editor additions through explicit `start_cycle`.
- [ ] Keep cycle detection before committing instances.
- [ ] Make conversion to an empty composite create `instances: []`.

This is also an opportunity to align the drag/drop behavior with the current editor documentation, which already describes positioned timeline drops as the primary interaction. 【F:src/rust/shoopdaloop/README.md†L89-L91】

**Verification**

- Empty-target conversion.
- Append serial to empty/non-empty schedules.
- Explicit positioned insertion into gaps and overlaps.
- Duplicate source insertion.
- Self-reference and nested dependency-cycle rejection.
- Legacy “parallel” gesture test only if that gesture remains in the product.

## 5. Test Impact

### Tests to rewrite

#### `shoop_app`

Rewrite these around flat instances and stable IDs:

- `composite_details_preserve_qml_schedule_semantics_and_canonical_session_data`
  - Rename to describe direct instance projection.
  - Remove assertions about playlist/section indices.
  - Assert stored instance IDs and absolute cycle positions.
- `compose_into_actions_convert_the_target_and_schedule_serial_or_parallel`
  - Assert explicit computed start cycles.
  - If the parallel gesture is removed, split out only the serial/positioned behavior that remains.
- `gui_conversion_and_serial_composition_are_authoritative_and_cycle_safe`
  - Replace empty `playlists` and nested indexing with empty/positioned `instances`.
- `failed_backend_composite_reconfiguration_does_not_commit_application_schedule`
  - Keep this test; adapt identifiers and document shape.
- `composite_event_groups_move_or_duplicate_while_preserving_relative_positions`
  - Keep and strengthen it: move retains IDs; duplicate allocates new IDs.
- `rich_composite_survives_session_load_and_save_without_projection_loss`
  - Keep and rewrite as the primary current-format round-trip test.

These tests are clustered in the existing composite application suite. 【F:src/rust/shoop_app/src/lib.rs†L11921-L12145】【F:src/rust/shoop_app/src/lib.rs†L12416-L12635】

#### `shoop_app_api`

- Rewrite intent tests containing `playlist_index`, `section_index`, and `parallel_index`.
- Add coverage for instance-specific resize and mode changes.
- Add stale/duplicate ID cases.

#### `shoop_egui`

Rewrite the interaction tests that currently manufacture structural event IDs:

- Selection and highlighting.
- Box selection.
- Delete key and context-menu deletion.
- Forced/natural length changes.
- External loop drops.
- Group move and Ctrl-duplicate.
- Script event mode changes.

The visual-only tests—empty message, current-position paint, scrolling, zoom, and row growth—should remain mostly unchanged because they already consume absolute detail ranges.

### Test to remove or replace

`composite_event_deletion_keeps_later_sections_at_their_absolute_positions` should be removed in its current form. Its purpose is to verify delay compensation after deleting serial sections, which is an artifact of the nested representation. The replacement should simply verify:

- Deleting one instance leaves every other instance’s `start_cycle` unchanged.
- Deleting the last instance produces an empty composite with zero displayed length.
- A stale instance ID is rejected transactionally.

The current implementation needs a substantial `composite_without_events_preserving_positions` algorithm only because deleting a section shifts the origins of later sections. 【F:src/rust/shoop_app/src/lib.rs†L5550-L5594】 That helper should disappear entirely under the flat model.

### Tests to add

1. **Legacy migration equivalence**
   - Multiple playlists.
   - Multiple serial sections.
   - Parallel entries of different natural lengths.
   - Delayed entries.
   - Forced lengths.
   - Script modes.
   - Repeated source references.

2. **Stable identity**
   - Delete unrelated instance without changing other IDs.
   - Move preserves ID.
   - Duplicate creates new ID.
   - Save/reload preserves ID.
   - Duplicate ID in a document is rejected.

3. **Instance-specific editing**
   - Two occurrences of one source.
   - Resize only one.
   - Change mode only one.
   - Delete only one.

4. **Compiler equivalence**
   - Flat schedule lowers to the expected engine occurrences.
   - Same compiled occurrences on native and worklet paths.
   - Deterministic backend configuration regardless of vector ordering.

5. **Fallback behavior**
   - If backends without composite support remain supported, validate absolute starts, gaps, and overlaps in the replacement scheduler.
   - Otherwise, explicitly test that positioned composite operations are rejected consistently when backend support is absent.

6. **Boundary validation**
   - `start_cycle * sync_length` overflow.
   - End-cycle overflow.
   - Zero forced cycles.
   - Unknown source loop.
   - Self-reference.
   - Nested composite cycle.
   - Empty source loops.
   - Very large instance collections/engine plan limits.

### Tests that should not need conceptual changes

The lower-level engine timeline/state-machine tests should remain because timelines and sections can stay as the backend/engine compilation format. The worklet wire protocol likewise currently transports nested timelines. 【F:src/rust/shoop_audio_protocol/src/lib.rs†L435-L435】 Those suites may need fixture renames only if the backend DTO is renamed; they should not be removed unless the engine model is also intentionally redesigned.

## 6. Recommended Ordering

1. **Define the flat document and shared timing helpers.**
2. **Implement legacy migration and prove schedule equivalence.**
3. **Implement flat-to-backend lowering.**
4. **Refactor the application model to make instances authoritative.**
5. **Change API identity from structural triples to stable IDs.**
6. **Update egui selection and editing.**
7. **Remove playlist-specific mutation/compensation code.**
8. **Update documentation and terminology.**
9. **Run full native and browser validation.**

Migration and lowering should exist before converting editor mutations; otherwise intermediate commits would either be unable to load old sessions or unable to configure the engine.

## 7. Final End-to-End Validation

- [ ] Open an old session containing serial and parallel composite sections.
- [ ] Confirm every legacy event appears at the same cycle and with the same duration/mode.
- [ ] Add two occurrences of the same loop at different cycles.
- [ ] Move one, duplicate one, resize one, and change one script mode.
- [ ] Save and reopen.
- [ ] Confirm stable IDs and all block properties survive.
- [ ] Play the composite through the native backend.
- [ ] Repeat through the browser/worklet backend.
- [ ] Change a referenced source loop’s natural length and confirm unforced instances and total length refresh correctly.
- [ ] Inject a backend configuration failure and confirm neither the document nor selection commits a partial edit.
- [ ] Confirm no front-end/session types, errors, comments, or tests refer to playlists, sections, parallel indices, or “parallel playlists.”

## 8. Execution Contract

- Keep this plan updated as implementation progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not change without explicit user approval.

## Validation Status

- [x] `cargo fmt --all -- --check`
- [x] `python3 scripts/check_shoop_test_usage.py`
- [x] `python3 scripts/check_tracing_coverage.py --require-closed`
- [x] `cargo test -p shoop_session -p shoop_app_api -p shoop_app -p shoop_egui`
- [x] `RUSTFLAGS="-D warnings" cargo build -p shoop_session -p shoop_app_api -p shoop_app -p shoop_egui`
- [x] `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_backend --features shoop_engine/app_backend -- --test-threads=1`
- [ ] GitHub Actions native and WebAssembly build matrix for the final commit.

The local all-workspace build reaches an environment-specific native `shoop_audio_worklet`/Tracy PIC link failure because this container does not provide the repository's Nix development shell. The supported GitHub Actions matrix supplies that environment and validates both native and WebAssembly targets.
