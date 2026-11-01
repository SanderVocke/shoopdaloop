# Core session persistence and media I/O

## Status and delivery

Implementation in progress; the app transfer foundation and engine content-upload
foundation are implemented, along with the underlying atomic full-owner replacement
cut and deferred processor presets.
Engine, adapter and service core capture/replacement are implemented. Public file
workflows, cooperative large-media conversion and final deployment gates remain
pending. Written under
[write_plan.md](.agents/prompts/write_plan.md) for priority 1: **session save/load
and media I/O**, covering the current-core portions of parity items P01, P02 and
P11 in the [feature parity ledger](docs/native_session_recovery/feature-parity.md).

- Base: `feat/backend-replacement`, inspected at
  `3a4a2139e9efbfe2ab0a4c58a94fbe02cfbfa6d3`.
- Work branch: `feat/session-persistence-media-io`, created from that base.
- Final implementation delivery: a PR from this branch into
  **`feat/backend-replacement`**, with passing checks on the final head. Do not
  target `master`, reopen the former stacked PRs, or merge without instructions.
  This workstream's PR is explicitly requested; the long-running feature branch
  remains the integration branch for the broader backend replacement.

## Goals and scope

Restore disk persistence for sessions expressible by the current production core,
and restore standalone audio/MIDI import, export and replacement through the new
service/RT stack. Preserve the surviving reference behavior and file formats.

The core includes direct audio/MIDI/trigger tracks, primitive loops and sync
relationships, recorded audio and MIDI, names/order, musical selection/targeting,
supported global and track controls, the fixed Master and supported connections
and mixer routes. Include representable stored loop/channel timing metadata;
restoring it is distinct from enabling the deferred timing-edit UI.

Standalone formats are the existing loop-audio archive, WAV, exact-MIDI archive
and Standard MIDI File. Include channel selection/mapping, existing resampling
confirmation, optional loop-length update and format-loss warnings.

This is a partial parity slice. Content duplication and generated click insertion
remain explicitly tracked under P11; click generation/preview remains P15. FX,
dry/wet, composites, additional buses, latency/take-alignment controls, live driver
switching and session-owned script/resource workflows remain their existing rows.
Do not silently discard these features from loaded files or saved live state.
Machine-local settings and script runtimes remain client-owned. Multi-UI support,
remote deployment, host-crash recovery, automatic script restart and shared-memory
reattachment remain outside scope.

## Immutable acceptance criteria

These criteria and the goals above require explicit user approval to change.

1. **Core session round-trip:** native and browser clients save and reload both
   structure-only and media-bearing core sessions through the production service.
   Structure, supported controls/routes, names/order, sync, selection/target and
   representable channel metadata survive. At an unchanged sample rate, audio
   samples and exact MIDI bytes, ordering and frame positions survive unchanged.
   Document identities/references remain consistent through live-ID mappings and
   session-generation fencing. Reloaded loops start stopped; a file is not a
   transport-position or Lua-VM recovery image.
2. **Coherent capture:** one save represents one defined application/engine cut,
   including settled admitted changes and matching immutable media versions.
   Playback may continue. As in reference `begin_save_session`, save during
   recording/replacing is refused with an actionable busy message; it neither
   stops capture nor silently saves an older take. Recheck at the capture cut.
3. **Media workflows:** existing audio/MIDI import/export entry points work for
   core loops. Audio mapping and optional length changes follow the reference.
   Replacing audio preserves MIDI, replacing MIDI preserves audio, and both retain
   loop identity and unaffected channel metadata. Exact formats are lossless;
   standard MIDI quantization/format limitations are reported. Sample-rate changes
   require confirmation and use existing resampling semantics, without switching
   the host device.
4. **Transactional mutation:** load/import validation, staging, capacity failure,
   cancellation or rejection before commit leaves the current session, mappings
   and media intact. The old session continues while preparation proceeds.
   Commit publishes one complete replacement at a defined RT boundary, with no
   visible partially rebuilt session. Stale targets, epochs and content revisions
   are rejected. The same request cannot apply twice after a lost acknowledgment.
5. **Honest operation lifetime:** pre-commit uploads are reclaimable on client
   loss. Once a mutating commit is accepted, the host owns it through a confirmed
   result even if that UI disappears; reconnect reconciles rather than replays.
   Cancellation after irrevocable commit reports the actual outcome. Read-only
   capture/export may be cancelled on loss. Capture, encoding, handing off a file
   and platform output completion are distinct; none falsely claims disk durability.
6. **Bounded work:** transfers, retained captures, staging, archive expansion,
   decoded media, concurrent jobs and reclamation have explicit finite budgets
   and overload results. Sessions exceeding one existing 32 MiB media-view message
   are handled through bounded transfers within the configured session budget.
   Ordinary commands/snapshots do not contain whole session media. RT execution
   remains prepared, bounded and free of allocation/deallocation, blocking, file
   I/O, codecs and Lua work. Client/service pumping remains cooperative.
7. **Compatibility and refusal:** reuse `shoop_session` formats, migrations and
   codecs. Enumerate unsupported requirements before any mutation, including
   scripts/resources and non-default unsupported metadata. Never flatten, omit or
   reset them to make a load succeed. Corrupt, oversized and unsupported documents
   preserve the running session. Missing external ports retain connection intent
   with a diagnostic rather than connecting a different device silently.
8. **Deployment coverage:** the same operations work through typed local delivery,
   native socket IPC, supported shared-memory delivery and browser main-thread
   service/AudioWorklet. Lua uses the same client operations where exposed by its
   public API. UI/VM loss does not recreate the service, adapter or audio device.
9. **Traceable parity and delivery:** ledger entries identify exactly which core
   subitems were restored, their production-route evidence and every remaining
   gap. Do not mark P01/P02/P11 wholly complete while deferred behavior remains.
   The final PR targets `feat/backend-replacement`; applicable platform CI and
   regression checks pass for its final implementation head.

## Design rules and baseline findings

- The app service owns shared state, admission and mutating jobs. The client owns
  dialogs, file/URL access, encoding/decoding orchestration and platform output;
  native workers and cooperative browser work must not block the service/audio.
  The RT adapter translates neutral contracts; the engine knows no session-file,
  UI, script or filesystem semantics. Keep the browser service on the main thread.
- Extend the existing `shoop_backend` capture/replace/content contracts and typed
  app/engine protocols where needed. Local delivery keeps immutable ownership
  without compulsory serialization. Wire delivery uses bounded versioned records,
  scoped transfer tokens, declared sizes, ordered chunks and checked identities.
- Separate `staging -> ready -> commit admitted -> finished` from client file-task
  state. Reserve resources/results before admission. Bound and reclaim abandoned
  stages/captures without invalidating an already accepted mutation.
- Current `shoop_app/src/service/media.rs` reads are selection-oriented, assemble
  a whole loop and are cancelled with the view/client. They are not a coherent
  session capture or durable mutation protocol. Save handles must not be cancelled
  just because the details pane changes selection.
- Current `shoop_rt_backend/src/session.rs` prevalidates structure-only loads but
  starts replacement by removing existing tracks. This is not transactional full
  restore. Stage the complete candidate and preflight old-plus-new resource
  headroom; do not obtain staging capacity by destroying the old session first.
- Reuse `shoop_loops` immutable exports, resource leases and prepared activation
  mechanisms where applicable. Its media ledger alone does not certify a musical
  cut, and topology staging alone does not certify resource/graph readiness.
  Establish the integrated engine cut and reliable retirement/receipt ownership.
- `shoop_session` currently uses document version 13 and format 1.0. Its default
  archive limits allow 16 GiB/one million entries, and standalone codecs also have
  large fixed limits. Apply explicit call-site budgets and bounded decoding for
  all formats; account for copies, compressed expansion and retained old media.
- Preserve existing recording pool limits. Capture pins and staged content need
  separately accounted headroom and explicit refusal. Reclaim on the proper side
  of the RT boundary, including cancellation, failed publication and host teardown.
- Preserve the existing capability/refusal architecture and all unrelated UI,
  assets, configuration and useful reference tests. A capability must not imply
  support for still-deferred operations that currently share its client match arm.
- Scope changed-source validation to each milestone; run full applicable gates
  at integration/final validation. Run selected Miri last after ordinary checks
  stabilize. Preserve seeds, workloads and meaningful watchdogs.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Staged implementation

Execute in order. Stages 3 and 4 share Stage 2's engine mechanisms; Stage 5 needs
the capture/mapping and content-installation contracts from both. Each stage ends
with a committed milestone and recorded verification; unchecked work is pending.

### Stage 0 — branch, inventory and planning

- [x] Create `feat/session-persistence-media-io` from the integration branch.
- [x] Read repository planning/rules/testing guidance and inspect the service,
  client refusals, RT adapter, session codecs and reference save/load/media flows.
- [x] Record immutable scope, parity exclusions and the final PR target here.
- [x] Commit and push this plan; do not begin implementation in the planning turn.

Verification: clean base and correct ancestry; review plan links, scope and
acceptance-to-stage coverage; run the parity inventory check if its ledger changes.

Planning verification: base ancestry, relative Markdown links, required execution
contract/stage structure, `git diff --check` and
`python3 scripts/check_feature_parity.py` pass. This is documentation-only evidence;
no implementation, runtime validation or new PR is claimed by this checkpoint.

### Stage 1 — persistence contract and operation/transfer protocol

- [x] Write a field/capability matrix against session documents and reference
  behavior: core fields, unsupported fields, client-local fields and ID remapping.
  Cover stopped-load semantics, busy recording policy, external connections,
  import offsets/length and every existing file-task completion/cancel transition.
- [x] Define finite budgets for session metadata, audio/MIDI bytes/events, archive
  entries/expansion, transfer chunks, concurrent stages/captures and retained results.
  Include full-session media larger than 32 MiB and old-plus-new peak memory.
- [x] Add neutral capture/stage/commit/result contracts and bounded app/engine
  messages as needed. Fence by authority, session/target generation, transfer/job
  identity and expected revision; version codecs and reject incompatible peers.
- [x] Define command ordering/capture barriers, chunk acknowledgments, cancellation,
  duplicate handling, disconnect cleanup, commit point and late-result reconciliation.

Verification: focused domain/protocol/codec tests for typed and wire equivalence,
hostile sizes, overflow, missing/repeated/reordered chunks, stale scopes and bounded
backpressure. Check native/Wasm and engine dependency boundaries.

Current milestone: [persistence-contract.md](docs/native_session_recovery/persistence-contract.md)
records the field matrix, budgets and lifecycle. App protocol v7 adds bounded
upload/capture messages, commit identity and a fenced ingress receiver; client/local
and wire envelopes share validation. Production service operations still explicitly
refuse persistence until the engine/service stages are implemented. Engine protocol
extensions and integrated admission/lifetime checks remain unchecked above.

Foundation validation: warning-denying builds of `shoop_app`, `shoop_app_client`
and `shoop_app_protocol` pass; 20 protocol/client and 21 production service native
tests pass. The same 20 protocol/client tests pass in Node/Wasm, including the
>32 MiB transfer and bounded/fenced reply queue. Engine/runtime/loop dependency
boundaries, Rust test-attribute policy, parity inventory and whitespace checks pass.
The local runners require the prepared sysroot's pkg-config paths and a writable
`WASM_PACK_CACHE`/`XDG_CACHE_HOME`; initial missing-path/cache errors were corrected,
not counted as test passes. Miri remains reserved for the final stage.

### Stage 2 — prepared RT capture and content/session activation

- [x] Add prepared bounded audio/indexed-MIDI upload, short-chunk buffer reuse,
  validation, cancellation/disconnect cleanup and explicit resource-budget refusals.
- [x] Provide the loop/runtime full-owner replacement cut and token-scoped processor
  presets that leave live settings unchanged until graph publication.
- [x] Expose a private engine replacement image with bounded edits, resource
  preparation, explicit commit/cancel and host-owned completion after disconnect.
- [x] Stage a private adapter mirror, stream canonical media into prepared sources,
  publish fresh mappings after engine success and retain exact original channel data.
- [ ] Inventory existing export/import/configuration primitives and integrate the
  smallest missing engine operations. Acquire immutable media and metadata at a
  certified cut; retain them independently of future clears/removals/recordings.
- [ ] Stage decoded audio/MIDI resources off-render, validate shapes/timing and
  reserve graph, loop, strip, media, result and retirement capacity before commit.
- [ ] Implement atomic content replacement and complete core-session activation.
  Preserve old playback until the defined cut, perform required MIDI cleanup,
  publish complete mappings/results and start restored loops stopped.
- [ ] Handle insufficient headroom, rejected activation, stale generations,
  abandoned uploads/captures, repeated release and teardown without partial mutation
  or resource leaks. Preserve `shoop_loops`/`rt_runtime` no_std boundaries.

Verification: offline production-engine tests inspect full samples/events, timing,
opposite-media preservation and old exports after replacement. Fault-inject every
fallible preparation/publication step, saturate independent pools, and heap-guard
callback activation, cancellation and bounded reclamation.

Upload milestone: RT protocol v2 advertises the actual prepared chunk limit and
defines begin/write/finish/release. Eight reusable write buffers and indexed-MIDI
initial-state owners return through reliable runtime completions. Uploaded sources
remain inaudible. The production app capability set is unchanged. Imported MIDI
initial-state preservation, coherent capture and content/session activation are
still pending; this upload foundation currently initializes MIDI from default state.

Validation: full native runtime/engine/protocol suites and adapter/carrier suites
pass; heap-guarded upload cases cover uninterrupted existing audio, malformed MIDI,
32 repeated release/cancel cycles, pipelined writes and 24 loss/reconnect cycles.
Node/Wasm passes 194 runtime, 23 engine and 4 protocol tests. Warning-denying affected
builds, formatting, test-attribute policy and whitespace checks pass. A pipelining
fixture initially skipped the client's disconnect notification/credit reset; it now
uses the real Goodbye path and preserves the eight simultaneous upload requests.

Atomic-cut milestone: `ConfigurationReplace` preflights complete owner/media/graph
publication at callback start, retires playing owners into fresh stopped identities,
retains old exports and requests bounded host MIDI cleanup only after success.
Active/prepared recording and an interior-callback cut are refused unchanged.
Processor configurations are prepared under the next graph epoch and a token,
applied during processing only after that epoch publishes, and explicitly discarded
on cancellation. Preparing settings before publication avoids a control/render race
that could otherwise expose default or partially restored controls. The engine
protocol replacement job and service/client integration remain pending.

Cut validation: full native `rt_runtime`, `shoop_loops` and `shoop_rt_engine` suites
pass. Focused Node/Wasm checks pass for two MIDI-reset cases, two complete-replacement
cases and the staged/cancelled/live-monitoring DSP case. Warning-denying native and
`thumbv7em-none-eabihf` runtime/loop builds, loop/runtime dependency boundaries,
formatting and test-attribute policy pass. These are kernel checks, not file-workflow
or final acceptance evidence; Miri has not run.

Engine-job milestone: protocol capability `COMPLETE_REPLACEMENT` exposes private
begin/edit/prepare/commit/cancel messages. Preparation validates fresh generations,
sync references, source ownership/shape, graph capacity and all processor presets.
Only commit changes the live model; all restored loops start stopped. Pre-commit
disconnect cancels, whereas an admitted commit finishes without its client. Idle
capture-group roots are released at the validated replacement cut so displaced
recordings do not consume the next upload's storage; retained snapshot leases and
resource revisions remain valid. MIDI initial-state upload is still pending.

Job validation: full native runtime/loop/engine/protocol suites pass; seven
heap-guarded replacement cases cover old playback, exact content/controls, in-flight
and prepared cancellation, active-recording rejection, empty images, tight-pool
reuse and disconnect on both sides of commit. A direct runtime test covers retained
snapshot readability and prepared-group reuse after idle content reclamation.
Node/Wasm passes all 31 engine and four protocol tests plus the runtime reclamation
case. Warning-denying affected native and runtime/loop embedded builds, dependency
boundaries, formatting, Rust test policy and whitespace checks pass.

Content milestone: prepared source adoption and `ContentApply` now replace a
single loop's complete media through the existing validated selection transition,
preserving loop identity and playing mode. Active capture refuses unchanged.
Short MIDI initial packets update the indexed upload before event bytes; malformed
or unrepresentable initial packets refuse explicitly. Bounded initial-state reads
emit the existing MIDI restoration projection one channel at a time. Imported
canonical initial packet sequences still need adapter retention for exact export.

Content validation: full native runtime/loop/engine/protocol suites pass, followed
by the new direct initial-packet validation test. Node/Wasm passes all 197 runtime,
166 loop, 33 engine and four protocol tests. Heap-guarded cases include 12 repeated
live content replacements and initial-state activation/export. Warning-denying
native and embedded builds, formatting and Rust test policy pass. The restored
MIDI helper keeps its existing full attempted-event accounting on sink failure;
the retained regression test caught and verified that detail during extraction.

Adapter milestone: session and loop-content jobs send acknowledged bounded records
and publish the candidate mirror only after successful engine commit. Cancellation,
unsupported metadata, invalid references and old-plus-new pool exhaustion preserve
the old image. Sync edges are prepared inside the complete image. Original audio
and MIDI remain in immutable control-side ownership; playback uses a bounded
normalized stream, so unequal channel/loop lengths, gains, offsets and alignment
are retained exactly for subsequent capture. Nonzero preplay and unrepresentable
MIDI initial packets refuse explicitly. Missing external ports retain intent with
a diagnostic. Save/load/import refuse active or scheduled recording with an
actionable message; service capture still must enforce the admission barrier.

The shared structure-replacement contract now stops unrelated recordings left by
earlier contract cases before testing structure replacement. Active-recording
rejection remains independently covered without stopping or changing its take.
This isolates the existing structural assertions; it does not remove capture tests.

Adapter verification: seven production adapter persistence cases
cover exact canonical round-trip, recording capture/refusal, cancellation and
invalid input, opposite-medium preservation while playing, sync edges, missing
ports and eight exhausted-headroom attempts followed by successful reuse. Public
app capabilities remain masked until the complete service/client route is ready.
All 25 native and 24 Node/Wasm adapter tests pass (the native thread-move check is
platform-specific), along with 21 service and 14 socket/shared-memory regression
tests. The expanded recording save/load refusal case also passes separately in
native and Node/Wasm. Warning-denying adapter/backend/service builds, dependency
boundaries, Rust test policy, formatting and whitespace checks pass. The carrier
tests require socket permission in this sandbox; their initial permission-denied
run was rerun successfully with network/socket access. Miri and final CI are pending.

Application-image milestone in progress: protocol v8 defines engine/file-independent
core metadata and immutable audio/MIDI payloads. Typed local messages share these
owners; native wire connections expand them into existing bounded persistence
records. A bounded source supports indexed MIDI byte ranges without rescanning a
whole event stream per chunk. Snapshot and ordinary-command epochs fence global
and fixed-Master actions as well as object-specific commands. Local-only variants
cannot enter ordinary wire records. The service workflow still refuses these
operations until its ownership/admission implementation is complete.

Image validation: all 46 protocol/client/service tests pass in native and Node/Wasm,
including exact typed/wire reconstruction, random MIDI byte ranges, the >32 MiB
image, refusal reconciliation for stale epochs and immutable local ownership.
Warning-denying protocol/client/service/host builds, dependency boundaries,
formatting, Rust test policy and whitespace checks pass. These are transport and
fencing checks; they do not claim restored file menus or service persistence yet.

Reader-release milestone: explicit engine bulk cancellation returns every active
or queued read credit, stops its reader and releases immutable exports before its
reliable completion. The adapter coalesces cancellation separately from ordinary
mutation receipts. Cancelling a view/capture therefore cannot retain old media
storage indefinitely. Heap-guarded tests cancel/reuse both credits twice while
music continues, then replace the session and consume the tight pool's newly freed
storage. Full native engine/protocol/adapter suites and all 34 engine, four protocol
and 24 adapter Node/Wasm tests pass. Warning-denying affected builds, formatting,
Rust test policy and whitespace checks pass.

### Stage 3 — service-owned coherent session capture and exports

- [x] Add service capture jobs that settle prior admitted work, capture matching
  app/engine state, recheck recording policy and pin exact content revisions.
- [x] Capture core application metadata, live/source ID relationships and connection
  intent, including channel annotations. Keep document codec conversion client-side.
- [x] Expose bounded session/selected-loop media reads from retained capture handles,
  with fair progress beside controls/snapshots and independent cancellation/release.
- [ ] Integrate client bundle assembly and existing session/audio/MIDI encoders
  off the service/RT path, retaining loss/quantization diagnostics and budget checks.

Verification: production service/adapter capture compared with reference documents;
mixed audio/MIDI/empty/sync cases, changes during transfer, capture while playing,
recording refusal, selection changes, cancellation and lost UI. Round-trip encoded
bytes through `shoop_session`; prove capture completion is not file-output success.

Service-core milestone: one owned job covers upload, preparation, immutable capture,
retained reads and accepted commit. A capture waits for prior admitted work, freezes
its application cut and acquires unchanged media while safe controls continue.
Media-changing work refuses until acquisition completes; view requests defer and
view cancellation cannot cancel the capture. A selected-loop capture uses a backend
subset and can finish while another loop records. Committed loads/imports survive
UI loss and reconcile by operation ID without replay. Complete publication installs
fresh app mappings and advances the epoch; fixed global/Master commands are fenced
too. New Session uses the same transaction, and an empty loaded session remains
usable. Sync-source lookup follows current objects rather than historical IDs.

Canonical import storage now has an aggregate 256 MiB/one-million-event ceiling;
derived read caches are reclaimable. Media views retain original imported samples,
offsets and event positions, while render upload uses normalized streams. The
adapter pipelines eight bounded writes and terminates engine-loss jobs with an
explicit unknown-outcome error. Large-media wire assembly, validation and backend
data copying still need cooperative progression before final responsiveness gates.

The dependency checker requires service/protocol/client layers to stay independent
of file codecs. Neutral application images preserve that boundary; `shoop_session`
document conversion and native/browser file output belong in the frontend provider.
Public I/O capabilities stay masked until that complete route is implemented.

Service-core validation: all 27 service and 27 native adapter tests pass; Node/Wasm
passes all 27 service and 26 portable adapter tests. The expanded deferred-view
resume assertion also passes natively and in Node. Warning-denying affected builds,
production dependency boundaries, test-attribute policy, formatting and whitespace
checks pass. This milestone does not claim file-format, UI, deployment or final
workspace acceptance.

### Stage 4 — bounded standalone media import and replacement

- [ ] Restore WAV/loop-audio/exact-MIDI/SMF decoding and channel mapping/selection
  using existing semantics, with bounded expansion and clear malformed-file errors.
- [ ] Restore sample-rate confirmation/resampling and revalidate target, host rate,
  topology/content revision and capacity after dialogs/background work finish.
- [ ] Upload/stage media through the service and commit one validated replacement.
  Preserve opposite media, loop identity and unaffected metadata; apply only the
  reference-prescribed offset resets and requested loop-length update.
- [ ] Separate restored import support from still-refused duplication/click actions
  when exposing capabilities. Leave their explicit ledger follow-ups intact.

Verification: actual client -> service -> adapter -> RT imports, exact export/import
round-trips, standard MIDI warnings, channel reorder/subset mapping, resampling
accept/decline, stale target, playing/recording conflict, quota failure and cancellation.
No rejected/partial upload changes any existing media.

### Stage 5 — transactional core session load

- [ ] Decode/migrate/validate the complete bundle and inventory all required
  capabilities before staging. Reject unsupported documents as a whole, with all
  useful diagnostics; preserve future-format and corruption checks.
- [ ] Confirm rate conversion, resample media and timing with existing helpers,
  then revalidate the current host/session before service admission.
- [ ] Stage complete core topology, content and app state. Atomically publish the
  engine candidate and corresponding service mappings/projection using a session
  epoch; resolve selection/target, sync, routes, controls and external connection
  intent without exposing old/new mixtures or retargeting queued old-session work.
- [ ] Make accepted loads host-owned through finish. Reconcile results on socket
  reattach, reclaim uncommitted uploads on loss, and route New Session through the
  same safe replacement mechanism. Keep the host/device and client Lua VM alive.

Verification: production save/load/save equivalence plus reference compatibility;
load while old audio plays, structure-only/empty/mixed-media sessions, repeated
loads and resets, insufficient old-plus-new capacity, rejected unsupported features,
bad hashes/IDs/lengths, cancellation and loss before/after commit. Confirm no false
rollback/success and no stale operations reaching newly loaded objects.

### Stage 6 — public native/browser/Lua workflows and parity accounting

- [ ] Replace only the relevant refusals in `shoop_app_client/src/runtime.rs`;
  restore existing menus, file/URL loaders, exports, mapping/rate dialogs, progress,
  cancel and error handling through `shoopdaloop`'s platform adapters.
- [ ] Keep files, URLs and dialogs client-local. Handle native failed/cancelled
  writes without corrupting the existing file; distinguish browser download
  handoff from a guarantee that bytes reached disk. Ignore stale task callbacks.
- [ ] Verify the same workflows through existing public Lua entry points where
  exposed; keep Lua VM execution and script/resource workflows in their proper scope.
- [ ] Update user/developer docs and the ledger with restored subitems and precise
  remaining P01/P02/P11 dependencies. Add production UI tests alongside retained
  reference fixtures; expose capabilities only after the complete route works.
- [ ] Add `feat/backend-replacement` to the PR base filters in build/test, CodeQL
  and docs workflows. Preserve production release and docs-deployment guards.

Verification: native typed/socket/shared-memory workflows and browser main-thread
service/AudioWorklet scenarios, including actual file output/input, URL failure,
cancel/failed-save/retry and Lua completion/refusal behavior. Run the parity checker
and workflow/config checks; confirm feature-branch PRs run checks without deployment.

### Stage 7 — final end-to-end validation and PR

- [ ] Exercise packaged native and hosted/self-contained browser workflows: record
  audio/MIDI, save/export, mutate, reload/import, compare content/state and play it
  back. Include >32 MiB media, near-budget refusal and malformed/unsupported files.
- [ ] Repeat socket UI-loss/reattach during upload, capture and accepted load/import;
  verify exactly-once mutation, complete projection, resource cleanup and responsive
  audio/control delivery. Cover shared-memory operation without claiming reattach.
- [ ] Run formatting, warning-denying workspace build, full native nextest with
  retained reference fixtures, Python boundary/parity/tracing/test-policy checks,
  all Node/Wasm packages and applicable Chromium/Firefox browser gates following
  repository testing guidance. Record hardware/mock and platform limitations.
- [ ] Run selected Miri on changed portable ownership/protocol/staging paths only
  after ordinary gates stabilize. Rerun affected checks after fixes; retain earlier
  evidence only with exact source provenance and a justified unchanged scope.
- [ ] Reconcile each acceptance criterion and ledger subitem with test/evidence
  links and final commit IDs. Commit milestones, push with this branch's own origin
  upstream, and create the PR with `--base feat/backend-replacement`.
- [ ] Inspect actual final-head checks with raw `gh`/git. Ensure Linux/macOS/Windows
  and web debug/release matrices, coverage, CodeQL and docs pass; fix failures and
  rerun. Use workflow dispatch if needed, without treating a missing check as a pass.
- [ ] Inspect automated review if present, address actionable feedback and repeat
  until approval. Record absence of a reviewer/verdict honestly; do not invent one.
  Update the plan/PR with final evidence and leave the PR open for the user.

Verification: green checks tied to the final implementation head, reviewed diff,
complete acceptance evidence and an open PR targeting the integration feature branch.
