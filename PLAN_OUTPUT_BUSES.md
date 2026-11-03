# Output bus parity without FX

## Status and delivery

Completed on `feat/output-bus-parity`, delivered in
[PR #926](https://github.com/SanderVocke/shoopdaloop/pull/926) into
`feat/backend-replacement`. Written under
[write_plan.md](.agents/prompts/write_plan.md) for P13 in the
[feature parity ledger](docs/native_session_recovery/feature-parity.md).

- Original inspected base: `feat/backend-replacement` at
  `3aabf43789708a5640a8f5b368f737651ef1d29d`.
- Implementation base rechecked at `64a4ad0634de1664b1ff723b36c635cd243d819b`,
  including the intervening click-track restoration.
- Reference: `master` at `92079fb6f00308be0f6b407b7e8c02229662e420`, with
  executable bus behavior preserved in `shoop_app_reference`.
- Implementation branch: `feat/output-bus-parity`, created from the current
  integration branch. Deliver its PR into
  **`feat/backend-replacement`**; do not target `master` or merge without instructions.
- User-approved scope: retain replacement-backend capacity limits, allow zero
  buses, treat Master as an ordinary default bus, and exclude bus FX.

## Goals and scope

Restore output bus lifecycle, mixing, routing, presentation, persistence and
recovery through the production service/RT stack. Reuse the surviving UI,
application contracts and reference behavior within the replacement backend's
prepared limits. This work closes P13 with that explicit capacity qualification;
P08 remains open.

Include named audio buses, mono/stereo/N-channel shapes within negotiated limits,
gain/mute/stereo balance, per-channel output meters, visual reordering, removal,
track-to-bus and bus-to-host routing, named-bus track defaults, UI/Lua controls,
and structure-only and media-bearing session round trips.

Bus FX, processor hosting, FX-control MIDI fan-out, wider legacy channel capacity,
bus-to-bus routing, and unrelated parity gaps are outside scope. Do not change
existing track/loop channel, media or device limits to deliver bus parity.

## Immutable acceptance criteria

These goals and criteria require explicit user approval to change.

1. **Ordinary buses and zero buses.** Fresh/New Session creates the default stereo
   bus named Master through the ordinary bus lifecycle. It has no protected
   identity or removal privilege. Users can remove every bus, keep using direct
   track-to-system routes, save/load that session, reconnect the UI, and add buses
   again. Loading or attaching to a session never recreates a missing Master.
   New Session also succeeds when the old session has zero buses.
2. **Honest capacity.** Support bus widths from one to the negotiated prepared
   maximum, bounded by the existing eight-channel RT representation
   (`MAX_STRIP_CHANNELS`). Bus count, total prepared channels, routes and host
   links have finite advertised limits. Do not promise master's 64-channel buses.
   At-limit requests succeed; out-of-limit requests fail without leaking slots,
   identities, ports or routes or disturbing existing audio.
3. **Mixing and meters.** Each bus sums its explicitly routed track channels and
   applies its own gain and mute, with balance available only for stereo buses.
   Preserve existing normalization and meter semantics. Controls and meters are
   independent between buses; control and routing changes preserve the RT ramp
   behavior and unrelated paths.
4. **Routing and defaults.** Track outputs can feed multiple bus channels and
   buses; bus outputs can feed supported host outputs. Direct and bus routes
   coexist. Named defaults select the first compatible matching bus in visual
   order, as in the reference: equal widths map one-to-one, mono fans out, and
   missing/incompatible names leave the track unconnected. Such names do not
   reject track creation. Existing startup/New Session auto-connect preferences
   apply to the newly created default bus; loaded explicit connection intent is
   preserved, including absent host ports, and reconnect does not rerun bootstrap.
5. **Identity and presentation.** Bus/channel/port IDs remain stable during their
   live lifetime and are not reused. Engine slot reuse uses generations. Visual
   reordering persists without rewiring audio or changing Lua's stable bus-index
   ordering. Duplicate names retain the reference semantics and never substitute
   for identity. Removed IDs refuse subsequent operations.
6. **Confirmed operations and recovery.** UI and Lua share service admission and
   confirmed state. Creation/removal, controls and connections report pending,
   confirmed and refused outcomes accurately; failed/partial operations reconcile
   to actual engine state. Once admitted, a mutation survives UI loss and is
   reconciled on reconnect without replay. Removal cleans its ports, incoming
   routes, outgoing links and related pending/error state only when appropriate
   to the confirmed result. No service/device restart is needed for UI recovery.
7. **Persistence and replacement.** Save/load preserves all supported buses,
   channel shapes/labels, controls, visual order, explicit routes and missing-port
   intent, with zero buses valid. Reuse the existing `.shoop` format and migration
   path. Load creates fresh live IDs and atomically publishes the complete
   candidate. Invalid/oversized/unsupported documents and pre-commit failures
   leave the running session intact. Media, track controls, sync and other
   supported session state retain existing round-trip behavior.
8. **FX exclusion and deployment.** BusManagement is enabled only when the complete
   production path is ready; BusFx remains false and FX controls stay disabled.
   FX-bearing bus requests/documents refuse explicitly without dropping FX state
   or mutating the session. The same bus behavior works through typed local
   delivery, native socket and supported shared-memory delivery, and the browser
   service/AudioWorklet. Final-head CI and applicable platform checks pass.

## Design rules and implementation map

- The service owns bus metadata, identities, display order and admitted work;
  the adapter owns application/engine mappings; the engine owns prepared bus
  slots and audio execution. UI/Lua consume the same projection. Keep the generic
  runtime free of application bus policy.
- Generalize `MasterBus` into a reusable bus processor and prepare bus capacity
  separately from track/loop slots. Inactive buses do not render. Master is the
  initial ordinary bus, not a permanent hidden engine node. Track-only and
  zero-bus graphs remain valid.
- Retain current numeric channel, record, host-port and media bounds. Add a finite
  bus pool to startup configuration and negotiate its actual limits; existing
  application bus/route constants remain outer ceilings. Carry these limits to
  the add-bus UI and settings validation. Choose/document prepared defaults and
  memory/graph budgets in Stage 1; do not reserve legacy-wide capacity implicitly.
- Allocate, prepare, validate and retire resources off the callback. Extend
  existing structural transactions, bank crossfades, receipts, generation fencing
  and replacement staging; no callback allocation/deallocation, locks or waits.
  Account for live and staged graphs without freeing live buses for load capacity.
- Extend RT requests, state/resync records, telemetry, limits and replacement
  edits coherently. Version changed app/RT/host/worklet wire layouts and test
  mismatches; typed and encoded delivery must have the same semantics.
- Reuse existing bus request/control validation, channel-label generation,
  operation receipts and client creation-result handling. Keep visual ordering
  separate from stable scripting order; the client currently enumerates the
  projected bus array and will need that distinction.
- Extend the persistence image from one mandatory `Master` to a bus collection
  plus display order. Update validation, capture, codec conversion, service
  mappings, adapter capture and engine replacement together. The existing
  `shoop_session` document already represents buses/order; a new file format is
  not required. Preserve whole-document capability diagnostics and bounded work.
- Reference anchors: `shoop_app_reference/src/lib.rs` tests
  `bus_lifecycle_and_visual_order_use_stable_identity_without_backend_reorder`,
  `bus_structure_rejection_and_timeout_reconcile_without_false_state`,
  `bus_controls_reconcile_confirmation_failure_timeout_stale_ids_and_peaks`, and
  `new_tracks_route_to_named_bus_with_equal_channels_and_mono_fanout`.
  Existing fixtures are behavioral evidence; add production-path verification.

Native recovery verification follows the existing recovery contract: socket UI
reattachment retains admitted work; shared-memory delivery supports one attachment
per host lifetime. The new process test initially attempted a second shared-memory
attachment and received the existing Unsupported refusal from `serve.rs`'s consumed
shared port. Test bus behavior on both carriers and reconnect on sockets; retain
the ledger's explicit shared-memory reattachment exclusion. This refines Stage 7's
verification method and does not expand the underlying recovery scope.

### Prepared capacity decision

The default is eight separately prepared bus slots with eight channels per slot
(64 prepared bus channels), configurable down to one slot and a two-channel
maximum width. An active bus can still be mono. The default stereo bus consumes
one slot. Route banks use the existing `u64` mask as eight groups of eight bits.
Graph budgets include `3 * buses * bus_channels` bus ports, both strip route
banks and both bus-to-host output banks; telemetry storage includes every bus.
Track/loop slots, channel bounds, media pages and wire record bounds are unchanged.
Limits travel through native host settings, worklet configuration and service
snapshots; old saved pool settings receive the new defaults.

Completion checkpoint: warning-denying workspace build, all-target and
no-default-feature service/adapter checks pass. The complete native suite passed
2,543 tests (four optional/platform skips), including admitted creation/control/
removal recovery. Hosted and single-file browser multi-bus/media/zero-bus workflows
pass at 900×600 with zero xruns; built worklet/codec contracts pass. The single-file
HTML was served alone over localhost because managed Chromium blocks direct
`file:` navigation. The guarded multi-bus audio test passes Miri on
nightly-2026-05-04, seed 0. Dependency boundaries pass for all four targets.
[The Miri report](docs/native_session_recovery/evidence/miri/output-buses/result.json)
records its source and hashed log; subsequent native meter assertions also pass.
[Acceptance evidence](docs/native_session_recovery/evidence/output-buses.md)
maps criteria to executable coverage. [PR #926](https://github.com/SanderVocke/shoopdaloop/pull/926)
targets the integration branch. Runtime head
`884f1b90441f7e304cda3e4bf7c57c14e1f6ec7b` passes the complete
[platform matrix](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645542902),
[CodeQL](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645542848),
[documentation](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645542840)
and both Codecov checks. All eight debug/release platform jobs, coverage and
bare-metal checks pass. Hosted and isolated single-file optimized builds also
pass the 33 MiB media workflow through zero-bus save/load, add and New Session.
P13 is complete within negotiated replacement limits; P08 remains open. No
automated code-review submission was provided. The coverage report was reviewed
and passes the configured patch floor (94.87% versus 80%).

The completion commit changes documentation only. Its final-head checks are
tracked on the [PR checks page](https://github.com/SanderVocke/shoopdaloop/pull/926/checks)
and must pass before handoff. The PR remains unmerged.

Final review found a multi-target Lua balance validation gap: a stereo target
could be queued before a later non-stereo target refused. Prevalidate every
target's control normalization before queueing any mutation; the typed/encoded
service regression verifies that a rejected group leaves all bus balances intact.
Bus slots also remain reserved until removal confirms, so a simultaneous add
cannot steal a still-live slot if removal refuses. The injected-refusal service
test now queues both operations and verifies the survivor and new bus independently.
Linux CI exposed a race in the new native test's result helper: confirmed results
can precede their corresponding projected snapshot. The helper now waits for
the confirmed revision, and the recovery case also waits for the old operation's
retained result in the fresh client's snapshot before asserting controls. This
keeps the real delivery ordering and unchanged deadlines.
The same ordering issue explained Windows CI's comparison of a pre-reorder
snapshot with the recovered display order. The service round-trip also exercises
successful multi-bus Lua gain/mute commands and their persisted controls.
The 33 MiB browser scenario retains that media during the new zero-bus file
round-trip. Its added save/load stages use the existing 120-second large-media
budget; ordinary bus stages retain their 30-second deadline. CI's preceding
large load took 75 seconds, so a 30-second zero-bus reload was not an appropriate
verification budget for that same fixture.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Staged implementation

Execute in order. Stage 2 establishes engine mechanisms for the adapter/service
in Stages 3–4. Stage 5 uses those mappings and replacement mechanisms. Stage 6
enables user-facing support only after the lifecycle and persistence path is ready.

### Stage 0 — planning checkpoint

- [x] Read repository planning/rules/build/test guidance and inspect the reference
  and production bus, routing, protocol, host-setup and persistence paths.
- [x] Record user-approved capacity, zero-bus, ordinary-Master and FX constraints.
- [x] Write this plan; leave implementation for a subsequent execution request.

Verification: review scope/acceptance/stage coverage, local Markdown links and
`git diff --check`. Record documentation checks separately from runtime evidence.

### Stage 1 — branch, contracts and capacity

- [x] Create the implementation branch from the latest integration head, preserve
  unrelated work, recheck the inspected assumptions, and commit/push the plan.
- [x] Define generational engine bus identities, create/remove/controls, route and
  output references, limits, telemetry/resync and replacement bus records.
- [x] Define finite bus pool/count/channel/graph budgets and defaults in host
  configuration without shrinking existing track/loop pools. Plumb native setup,
  worklet config, backward-compatible saved settings and client-visible limits.
- [x] Extend the application persistence schema for zero-or-more buses and display
  order; identify all mandatory-Master call sites and version affected wire layouts.

Verification: protocol/codec round trips, malformed and mismatched-version records,
settings default migration, capacity/config validation and native/worklet setup
round trips. Check encoded record bounds and compile affected consumers.

### Stage 2 — engine bus graph and replacement

- [x] Generalize the bus DSP/node, prepared pool, model, cells, graph builder,
  control scheduling and telemetry/resync to ordinary active bus slots.
- [x] Implement create/remove, bus controls, strip-to-bus fan-out and bus-to-host
  links through structural transactions and existing ramp/crossfade machinery.
  Clean references on removal and reject stale generations.
- [x] Extend private replacement candidates and commit/retirement to arbitrary
  supported buses, including an empty set. Remove permanent-Master assumptions
  from startup and graph construction.

Verification: guarded engine tests for mono/stereo/prepared-maximum widths,
independent summing/gain/mute/balance/meters, mixed direct/bus paths, route changes,
zero buses, repeated slot reuse, capacity refusal and atomic replacement failure.
Keep render/control-endpoint heap guards active; run focused ownership/generation
Miri cases after ordinary tests stabilize.

### Stage 3 — RT backend adapter

- [x] Implement `create_bus`, `remove_bus` and per-bus controls with reserved
  application IDs, engine slot generations, channel/port mappings and receipts.
- [x] Replace the single-Master mirror, ports, meters, connection targets and
  routing map with dynamic confirmed bus state; handle resync and negotiated limits.
- [x] Extend backend capture/replacement mappings and outcomes for every bus and
  for no buses; preserve unrelated track/loop state and missing-host-port intent.

Verification: adapter/contracts/persistence tests cover confirmed and refused
creation/removal/control/routes, cleanup, pool reuse, stale IDs, resync, zero-bus
capture and failed replacement. Exercise typed and encoded engine delivery.

### Stage 4 — authoritative service and scripting

- [x] Add service-owned bus/channel counters, registry, display order, pending
  effects and reconciliation, following existing admitted-operation ownership.
- [x] Implement bus commands, reorder validation, per-bus projection, accurate
  control/structural pending/errors and removal cleanup. Validate all Lua bus
  targets before mutation; preserve stable script indices across visual reorder.
- [x] Generalize named-bus track defaults to reference-compatible mapping; remove
  the non-Master-name rejection and modulo-only Master routing.
- [x] Create the fresh/New Session default through the ordinary lifecycle and
  scope auto-connect to its bootstrap; do not infer a protected bus from name,
  ID, first position, reconnect or an empty loaded bus list.

Verification: service/client tests for UI and Lua controls, duplicate/missing names,
mono fan-out/equal-width/mismatch defaults, display versus script order, last-bus
removal, explicit direct routes, creation outcomes and rejection reconciliation.
Native recovery tests lose the UI during admitted add/remove/control/route work
and confirm retained outcomes without replay or an unintended default bus.

### Stage 5 — complete persistence and zero-bus workflows

- [x] Capture all buses/order from a coherent settled cut; generalize persistence
  image validation, codec conversion and source-to-live ID/channel/port mapping.
- [x] Load structure-only and media-bearing sessions with zero, one and multiple
  buses using the adapter/engine atomic candidate path. Preserve saved output and
  route intent; report unsupported widths/counts/FX before live mutation.
- [x] Make New Session independent of the old session's buses and synthesize a
  fresh default bus. Cover save/cancel/load fencing and UI loss during accepted
  replacement, retaining existing recording/busy and media-preservation rules.

Verification: production save/load/save fixtures for no buses, no Master, reordered
and duplicate-named buses, prepared-maximum width, controls, routing and missing
host ports. Compare exact supported PCM/MIDI and document references. Exercise
New Session from zero buses and reject oversized/FX/malformed candidates intact.

### Stage 6 — UI integration, capabilities and documentation

- [x] Enable BusManagement through backend and service capability projection after
  Stages 2–5 pass; retain BusFx=false, an empty bus processor catalog and explicit
  service-side FX refusals.
- [x] Use actual prepared limits in add-bus UI/settings. Reuse mixer strips,
  confirmation dialogs, drag order, meters and connection tabs; show zero buses
  as a usable state with Add bus available. Retain pending/failure feedback.
- [x] Update affected user/crate guides, architecture and recovery ownership
  documentation. Update P13 and its P01/P02 interactions with the approved capacity
  qualification and evidence; keep P08 and unrelated gaps open.

Verification: UI interaction tests drive add/control/reorder/remove/last-remove,
connection routes and track defaults through production commands/projections.
Confirm FX remains disabled and forged FX commands refuse. Run parity inventory,
documentation links and tracing/dependency boundary checks.

### Stage 7 — final end-to-end validation and delivery

- [x] Run one production scenario through typed local delivery, native socket and
  supported shared-memory delivery, and hosted/single-file browsers: create mono,
  stereo and prepared-maximum buses; record/play into multiple buses; change
  controls/routes/order; save/load; reconnect; remove every bus; save/load again;
  create another bus; then New Session. Verify audio, meters, identities, order,
  missing-port intent and operation results. Repeat UI loss at admitted mutation
  and replacement boundaries. Keep unrelated core looping/media workflows green.
- [x] Run reference bus fixtures plus new production tests and the full applicable
  Linux/macOS/Windows and Wasm/browser matrix. Follow `.agents/info/build.md` for
  environment selection and `.agents/info/test.md` for gates:
  `cargo fmt --all -- --check`, a workspace build with `RUSTFLAGS="-D warnings"`,
  the complete workspace nextest suite with its documented features/profile,
  `scripts/check_shoop_test_usage.py`, `scripts/check_feature_parity.py`, closed
  tracing inventory, RT dependency boundaries, and built-worklet artifact checks.
  Record focused Miri results and any unavailable external hardware separately.
- [x] Record acceptance evidence and final source heads; mark P13 complete only
  when every criterion above is met within negotiated replacement limits. Update
  ledger summary counts without claiming bus FX or unrelated parity completion.
- [x] Commit milestones, push the implementation branch and create a reviewable
  PR targeting `feat/backend-replacement`. Ensure CI is green on the final head.
  Check for automated review after pushes; if it arrives, address actionable
  feedback and repeat review until approval, rerunning checks affected by fixes.
  Report remaining blockers honestly; do not merge without instructions.

Verification: final acceptance-to-evidence mapping, passing final-head CI, automated
review approval when provided, correct PR base and a clean implementation worktree.

## Planning verification

Planning checkpoint passed: plan structure and acceptance/stage coverage, the required execution
contract, local Markdown links, whitespace checks, `git diff --check`, and
`python3 scripts/check_feature_parity.py`. The initial planning turn changed documentation only. Implementation evidence
and delivery status are recorded above as the work progresses.
