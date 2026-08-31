# Authoritative Composite Start and Seek Plan

## Goals

- Make every explicit composite start and seek establish an authoritative snapshot over all targets referenced by the active composite plan.
- Preserve incremental, delta-based behavior during natural schedule advancement and wraparound.
- Preserve the existing boundary intent precedence model, including its deterministic runtime-identity tie-breaks.
- Apply the same semantics to primitive and nested-composite targets, immediate and delayed transitions, native and browser backends, without adding realtime allocation.

## Scope

This work changes composite runtime reconciliation and its tests. It may update developer- or user-facing composite semantic documentation where the contract is described. It does not introduce persisted composite priority, change session IDs or formats, change the current direct/script/regular/natural precedence ordering, or implement hierarchy-based precedence.

## Immutable Acceptance Criteria

1. An explicit non-stopped start at iteration `N` emits `SetMode` for every referenced target desired at `N` and `Stop` for every referenced target not desired at `N`, regardless of the composite runtime's remembered active targets.
2. An explicit seek to iteration `N` establishes the same complete target snapshot as an explicit start at `N`.
3. A delayed transition establishes the same authoritative snapshot when it executes as the equivalent immediate transition.
4. Explicit stop continues to stop every referenced target.
5. Authoritative operations on nested composites propagate to primitive descendants at the same sample through normal boundary intent resolution.
6. All generated operations continue to participate in the existing precedence and conflict-tracing model; direct, script, regular, and natural priority is unchanged.
7. Natural iteration advancement, regular wraparound, script completion, and plan replacement remain delta-based unless they execute an explicit start or seek; they do not repeatedly stop referenced targets that are already inactive in that composite runtime.
8. Empty plans retain their existing stopped behavior, invalid seeks and modes retain their existing errors, and stale targets retain their existing counter/error behavior.
9. Composite processing remains bounded and allocation-free on the realtime path.
10. Native and WebAssembly composite test coverage passes.

## Design Rules and Constraints

- Model reconciliation policy explicitly, preferably with a private enum such as `Delta` and `Authoritative`; do not add another unexplained positional boolean.
- Authoritative means emitting a complete set of intents for the selected iteration. It does not bypass conflict resolution or grant exclusive ownership after that boundary.
- In authoritative reconciliation, an undesired current target receives `Stop` even when the runtime's local active entry is false. A desired target follows existing mode, offset, retrigger, empty-child, and recording rules.
- Delta reconciliation retains the current stop condition: emit a stop only for a locally active target that is no longer desired.
- Reuse the immediate-transition path for delayed transition execution so both forms cannot diverge semantically.
- Keep target ordering, fixed-capacity batches, stale-identity checks, counters, transactional boundary commit, and no-allocation guarantees intact.
- Treat engine source identity as a deterministic runtime tie-break, not a persisted session priority. Application loop IDs are persisted, but this work must not make engine slot/generation identity part of the session contract.
- Avoid unrelated refactors, formatting changes, or changes to composite plan compilation.

## Execution Contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Staged Implementation

### Stage 1: Lock Down Runtime Semantics

- [x] Add focused state-machine tests showing that an explicit start stops a referenced primitive that is not desired at the selected iteration even when the composite does not remember it as active.
- [x] Add a two-target test showing that one authoritative operation emits `SetMode` for the desired target and `Stop` for the undesired target.
- [x] Add explicit-seek coverage for the same complete destination snapshot, including a target whose external state cannot be inferred from local `active` bookkeeping.
- [x] Add delayed-start coverage proving execution matches an immediate start.
- [x] Verify the new tests fail for the missing authoritative stops while existing start, seek, invalid-input, empty-child, and recording cases remain meaningful.

**Verification**

- [x] Run the targeted `shoop_engine` composite state-machine tests.
- [x] Confirm failures are limited to the new authoritative expectations before changing runtime code.

### Stage 2: Implement Explicit Reconciliation Policy

- [x] Add a private reconciliation policy type and thread it through composite runtime reconciliation.
- [x] Change the stop pass so authoritative reconciliation emits `Stop` for every undesired installed target, while delta reconciliation preserves the existing active-to-inactive behavior.
- [x] Use authoritative reconciliation for explicit non-stopped immediate transitions and explicit seeks.
- [x] Confirm delayed transitions execute through the authoritative immediate-transition path.
- [x] Keep natural advancement, regular wraparound, recording pass completion, script completion, and plan activation/replacement on the appropriate delta paths.
- [x] Preserve desired-target mode selection, offsets, retrigger behavior, local active state, stale-target handling, batch capacity, counters, and empty-plan behavior.

**Verification**

- [x] Run the targeted composite state-machine tests and confirm all Stage 1 cases pass.
- [x] Run the realtime no-allocation composite test to ensure the policy adds no allocation.
- [x] Review every reconciliation call site and record its authoritative or delta rationale in the implementation commit or plan progress notes.

Immediate transitions and seeks use authoritative reconciliation. Delayed transitions reuse the immediate path when due. Natural advancement, pass completion, wraparound, and plan activation use delta reconciliation because they continue or retire existing runtime ownership rather than establish an externally requested snapshot.

### Stage 3: Verify Nested Propagation and Conflict Resolution

- [x] Add a boundary-timeline test where starting a root composite starts a nested composite and stops a deep primitive that is referenced but not desired at iteration zero.
- [x] Add or extend a conflict test where an authoritative composite stop conflicts with a higher-priority direct start, proving the direct intent still wins and the losing conflict is traced.
- [x] Add a regression test proving natural advancement does not emit redundant stops for referenced targets that remain locally inactive.
- [x] Confirm same-sample propagation remains topology-ordered and transactional for all affected nested operations.

**Verification**

- [x] Run targeted composite timeline, state-machine, timing, control, and app-backend tests.
- [x] Inspect boundary traces in the new cases for the expected origin, action, winner, and losing-conflict count.

### Stage 4: Document and Audit the Contract

- [ ] Document that explicit start and seek establish a complete target snapshot, while natural schedule advancement is incremental.
- [ ] Document that authoritative operations still participate in normal conflict resolution.
- [ ] Where tie-break semantics are documented, state that engine source identity is a deterministic runtime tie-break and not a persisted user-defined priority.
- [ ] Audit native and AudioWorklet command paths to confirm they both reach the same engine runtime semantics without backend-specific duplication.

**Verification**

- [ ] Review terminology consistently for `authoritative`, `snapshot`, `explicit`, and `delta` behavior.
- [ ] Confirm no session schema, wire protocol, or public API change is required.

### Stage 5: End-to-End Validation

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run the repository's Rust test-usage check if Rust tests were changed.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown` and run the documented browser smoke checks when browsers are available.
- [ ] Run `python3 scripts/run_wasm_tests.py --profile ci --runtime node --package shoop_engine` and also use the Chrome runtime when available.
- [ ] Review the final diff for unrelated changes and verify every immutable acceptance criterion has direct test or inspection evidence.

**Verification**

- [ ] Record every command and outcome, including any environment-only limitation.
- [ ] Confirm native and WebAssembly behavior uses the same authoritative runtime implementation.
- [ ] Commit the completed validation milestone with this plan updated to reflect actual progress.
