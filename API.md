# Public Loop Runtime API Refactoring Plan

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Goals

- Provide a crates.io-ready, frontend-neutral Rust API for building native, offline, and browser loop applications.
- Make tracks, basic loops, composite loops, content access, routing, and runtime observation available without exposing the realtime graph implementation or ShoopDaLoop UI concepts.
- Preserve `shoop_engine` as the realtime implementation layer and make the reusable runtime API the normal integration boundary.
- Give local/native and browser AudioWorklet runtimes equivalent domain semantics behind the same public control API.
- Retain ShoopDaLoop behavior while moving product policy, scripting, dialogs, selection, and file workflows above the reusable runtime boundary.

## Scope

### In scope

- A neutral domain/API crate shared by runtime implementations and consumers.
- Refactoring `shoop_backend` into the reusable runtime implementation and compatibility boundary.
- Semantic commands, structured errors, capabilities, snapshots, command completion, and strong unit types.
- First-class editable and validated composite-loop specifications.
- Separation of loop control, driver hosting, connections, processors, content, session transfer, and offline progression.
- Native, deterministic/offline, fake, and browser AudioWorklet adapters.
- Versioned browser protocol mappings and compatibility tests.
- Migration of `shoop_app` and `shoop_app_api` onto the neutral API without changing user-visible behavior.
- crates.io metadata, feature design, documentation, examples, and publishability checks.

### Out of scope

- A new frontend toolkit or changes to the egui visual design.
- New loop modes, composite semantics, DSP algorithms, session features, or processor features.
- Redesigning the session archive format except where an adapter is required at the runtime boundary.
- Stabilizing every low-level `shoop_engine` module as part of the initial public runtime release.
- Replacing the browser wire encoding solely for performance; encoding may change only when required by the new API or compatibility contract.

## Immutable acceptance criteria

1. A downstream Rust program can depend on the public runtime crates and create tracks, basic loops, nested composite loops, control transitions, inspect snapshots, and access audio/MIDI content without depending on `shoop_app`, `shoop_app_api`, `shoop_egui`, or ShoopDaLoop product resources.
2. The neutral runtime API contains no UI gesture vocabulary, egui types, file-picker/dialog concepts, script-dialog state, or ShoopDaLoop selection policy.
3. `shoop_backend` and lower layers do not depend on `shoop_app_api`; neutral dependencies point toward the domain API, never toward product or frontend crates.
4. Native/local, deterministic offline, fake/test, and browser AudioWorklet implementations expose the same core loop and composite commands, state, structured errors, and completion semantics. Platform-only facilities are reported through explicit capabilities.
5. Composite specifications use named structures, have documented units and timing semantics, support nesting, and can be validated without starting an audio driver.
6. Public mutation APIs distinguish command acceptance from eventual commit or rejection and correlate outcomes with stable command tickets.
7. Public library APIs return structured errors rather than `anyhow::Error` or relying on free-form "unavailable" strings for control flow.
8. Audio callback/render processing remains bounded and realtime-safe; the refactoring introduces no callback allocations, blocking waits, or unbounded queues.
9. The browser protocol remains explicitly versioned and has contract tests covering every supported public command, result, error, capability, and snapshot representation.
10. Existing ShoopDaLoop native and browser behaviors and session compatibility remain covered by the existing test suites.
11. The intended public crates pass package verification for crates.io with complete metadata, explicit versioned internal dependencies, documented features, and no unpublished mandatory dependency.
12. Public API documentation includes native, offline, and browser examples plus a migration guide for the pre-refactor internal interfaces.

## Design rules and constraints

- Keep `shoop_engine` responsible for realtime graph execution, scheduling, storage, and DSP; do not make downstream application policy part of it.
- Use a neutral crate, provisionally `shoop_runtime_api`, for domain types and contracts. Use `shoop_backend` as the implementation crate unless a rename is justified before its first publication.
- Keep dependency direction acyclic: product/UI crates may depend on runtime crates, while runtime crates must not depend on product/UI crates.
- Express caller intent semantically (`Transition`, `SetGain`, `ReplaceContent`) rather than as presentation events (`Clicked`, `Changed`). UI gesture translation remains in the product controller.
- Separate an editable domain specification from its validated/compiled engine representation. Do not expose internal graph indices, engine handles, state mirrors, ringbuffers, or command reservations in the normal runtime API.
- Use opaque stable entity IDs and transparent newtypes for samples, frames, cycles, iterations, sample rates, gains, and balances. Define range validation and conversion behavior at API boundaries.
- Prefer a small core runtime command/snapshot contract plus focused capability interfaces over one trait containing unrelated platform services.
- Model unsupported facilities through typed capabilities and typed errors. Do not infer support by invoking a method and parsing its error text.
- Keep control-plane asynchrony independent of Rust async executors. Commands, tickets, events, and polling must work in native threads, cooperative browser loops, and deterministic tests.
- Keep wire DTOs distinct from ergonomic domain types, but centralize conversions and require exhaustive round-trip tests. Protocol evolution must define compatibility and rejection behavior.
- Keep archive I/O and document migration in `shoop_session`; runtime session instantiation consumes a validated neutral specification and returns an entity mapping.
- Represent processors generically in the core API. Carla and OxiSynth integrations are optional capabilities/features, not required domain dependencies.
- Default crate features must remain lightweight. Native host libraries, browser bindings, plugin hosting, synths, serialization, and tracing integrations must be optional where feasible.
- Preserve bounded capacities and explicit backpressure across local queues and the worklet transport.
- Add compatibility adapters before migrating call sites; remove legacy APIs only after native and browser consumers have moved and equivalent tests pass.

## Staged implementation plan

### Stage 0: baseline and API inventory

- [ ] Record the current crate dependency graph, public backend/app API inventory, enabled feature combinations, and native/wasm package build matrix.
- [ ] Add characterization tests for basic-loop transitions, grabs, nested composites, content replacement/readback, session replacement, mutation failure reporting, and browser command replay where coverage is missing.
- [ ] Document current callback allocation, queue-capacity, snapshot polling, and command settlement invariants that the refactor must preserve.
- [ ] Decide final public crate names before publishing them; record any rename and migration implications in this plan without changing the goals or acceptance criteria.
- [ ] Verify with the complete native test suite, wasm builds/tests, dependency checks, and realtime allocation tests; retain results as the comparison baseline.

### Stage 1: introduce the neutral domain API

Depends on Stage 0.

- [ ] Add `shoop_runtime_api` with opaque entity IDs, strong unit types, track topology, loop modes, semantic commands, composite specifications, content descriptors, runtime snapshots, capabilities, command tickets/events, and structured error enums.
- [ ] Define named composite structures in place of anonymous nested vectors and expose pure validation with structured errors and documented timing/nesting rules.
- [ ] Define generic processor descriptors, state envelopes, and commands without Carla- or OxiSynth-specific core variants.
- [ ] Make serialization an optional feature and define stable serialized forms only for types that cross a persistence or worklet boundary.
- [ ] Add compile-time dependency guards or repository checks preventing neutral crates from depending on product/UI crates.
- [ ] Verify domain validation/unit tests, serde round trips under the serialization feature, minimal/default feature builds, documentation tests, and wasm compilation.

### Stage 2: adapt the engine-backed runtime

Depends on Stage 1.

- [ ] Add an adapter from neutral domain commands/specifications to `shoop_engine` operations while keeping engine handles, graph indices, mirrors, and scheduling internals private.
- [ ] Split the current backend responsibilities into a core loop runtime and focused interfaces for driver control, host connections, content access, session transfer, processors, and offline rendering/simulation.
- [ ] Implement capabilities from actual constructor/feature/platform support rather than default methods that return unavailable errors.
- [ ] Introduce command acceptance, settlement events, and stable correlation tickets; map engine validation, capacity, lifecycle, and execution failures into structured runtime errors.
- [ ] Keep a temporary legacy `Backend` adapter so existing consumers can migrate incrementally.
- [ ] Verify parity tests against the legacy engine backend, composite compiler tests, command settlement ordering, capacity/backpressure behavior, warning-denying builds, and realtime no-allocation checks.

### Stage 3: separate platform and processor integrations

Depends on Stage 2.

- [ ] Move driver discovery/switching behind the driver-control capability and keep deterministic/offline progression separate from callback-driven native processing.
- [ ] Move host-port discovery and connection mutation behind the connections capability.
- [ ] Adapt Carla and OxiSynth through generic processor APIs and optional features; keep their typed convenience APIs in integration-specific modules or crates.
- [ ] Audit `shoop_engine` dependencies and features so pure loop/composite logic does not enable native drivers, file dialogs, plugin hosting, or product settings.
- [ ] Verify feature combinations independently: minimal core, offline engine, native CPAL/MIDI, JACK where available, Carla, OxiSynth, and wasm.

### Stage 4: align session and content boundaries

Depends on Stages 2 and 3.

- [ ] Define validated runtime session specifications and entity mappings distinct from archive documents and live backend snapshots.
- [ ] Add explicit conversion between `shoop_session` documents and runtime specifications while preserving current archive compatibility and source identity mapping.
- [ ] Move bulk audio/MIDI content reads and replacements behind the content capability with bounded chunking and explicit revision/currentness semantics.
- [ ] Unify synchronous local completion and chunked remote completion under the same tickets, progress events, and structured outcomes.
- [ ] Verify archive compatibility fixtures, save/load round trips, sample-rate conversion paths, large chunked transfers, cancellation/failure recovery, stale-content handling, and bounded-memory behavior.

### Stage 5: migrate the browser AudioWorklet stack

Depends on Stages 2 and 4.

- [ ] Map every browser-supported neutral command, event, error, capability, and snapshot to an explicitly versioned wire DTO.
- [ ] Refactor the worklet host to use the engine-backed neutral runtime and the client to implement the same runtime interfaces used locally.
- [ ] Centralize domain/wire conversions and add exhaustive variant coverage plus encode/decode and client/host round-trip tests.
- [ ] Define protocol behavior for version mismatch, duplicate/replayed commands, queue saturation, transport restart, partial bulk transfer, and unsupported capabilities.
- [ ] Package the worklet artifact and client behind an ergonomic browser constructor that does not expose wire DTOs for normal use.
- [ ] Verify wasm unit tests, Node worker probes, browser smoke tests when browsers are available, realtime render allocation checks, MIDI bridging, transport restart/replay, and native-vs-browser conformance scenarios.

### Stage 6: migrate the ShoopDaLoop product controller

Depends on Stages 2 through 5.

- [ ] Change `shoop_app` to consume the neutral runtime interfaces and translate UI/product intents into semantic runtime commands.
- [ ] Keep gesture vocabulary, selection and optimistic UI policy, scripting/dialog state, file pickers, previews, and persistence workflows in `shoop_app_api`/`shoop_app`.
- [ ] Replace duplicated app/backend models with intentional domain-to-view-model projections; preserve view-specific state only where the frontend needs it.
- [ ] Migrate native startup, cooperative browser startup, fake backend tests, scripting integration, and egui call sites.
- [ ] Remove the `shoop_backend` dependency on `shoop_app_api` and enforce the final dependency direction.
- [ ] Verify application intent/model tests, native and browser startup, UI snapshot behavior, scripts, import/export, driver switching, connections, session workflows, and existing end-to-end tests.

### Stage 7: retire compatibility APIs and harden the public surface

Depends on Stage 6.

- [ ] Deprecate legacy backend/app-facing types and methods for one migration interval where practical, then remove them after all in-repository consumers migrate.
- [ ] Review public visibility and seal implementation details; expose only supported constructors, contracts, extension points, and diagnostics.
- [ ] Add missing rustdoc safety, threading, realtime, lifecycle, units, error, and compatibility guarantees.
- [ ] Run semver/API-diff tooling against a recorded public baseline once the candidate API is declared stable.
- [ ] Verify no product/UI dependency is reachable from public runtime crates and no legacy type appears in their public signatures.

### Stage 8: crates.io packaging and consumer examples

Depends on Stage 7.

- [ ] Add descriptions, license, repository, documentation, readme, keywords, categories, Rust version, include/exclude rules, and docs.rs feature metadata to every published crate.
- [ ] Give all published internal dependencies compatible crates.io versions alongside workspace paths and establish an explicit publish order.
- [ ] Eliminate or isolate mandatory unpublished/git-only dependencies and confirm packaged sources include generated assets required to build.
- [ ] Document the feature matrix, platform prerequisites, support policy, protocol compatibility policy, and public semver boundary.
- [ ] Add standalone examples for an offline basic looper, a nested composite arrangement, a native host, and a browser application; ensure examples depend only on packaged public APIs.
- [ ] Add a migration guide mapping legacy backend operations and UI-like actions to the new semantic API.
- [ ] Verify `cargo package` and `cargo publish --dry-run` for each crate in publish order, install examples from packaged artifacts in a clean temporary project, and build docs.rs-equivalent feature sets.

### Stage 9: final end-to-end validation

Depends on all previous stages.

- [ ] Run formatting, warning-denying workspace builds, tracing inventory checks, dependency-boundary checks, and the complete Rust suite.
- [ ] Run targeted realtime allocation, lock, queue-capacity, command settlement, composite conformance, content transfer, and session compatibility tests.
- [ ] Build all supported native feature combinations and both the application and AudioWorklet for `wasm32-unknown-unknown`.
- [ ] Run Node worker checks and browser smoke/conformance tests in each available browser, covering basic loops, nested composites, audio/MIDI content, session round trips, host MIDI, failure recovery, and transport restart.
- [ ] Exercise each published example against locally packaged crates rather than workspace paths.
- [ ] Confirm all immutable acceptance criteria with recorded evidence, update public documentation and migration status, and remove temporary adapters only if no supported consumer remains.
