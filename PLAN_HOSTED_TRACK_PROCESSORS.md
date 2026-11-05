# Host-side built-in FX, OxiSynth and dry/wet track parity

## Status, base and delivery

Implementation started on `feat/hosted-track-processors` from `2e19210b`. Written under
[write_plan.md](.agents/prompts/write_plan.md).

- Start from `feat/fixed-quantum-runtime`, inspected at `2e19210b`.
- Reference: fetched `master` at `92079fb6`, plus executable behavior retained in
  `shoop_app_reference`, `shoop_engine`, and `reference/legacy_browser`.
- Implementation branch: create `feat/hosted-track-processors` from the current
  `feat/fixed-quantum-runtime`; record its actual base before implementation.
  Initial base: `feat/fixed-quantum-runtime`. At the user's request, PR
  [#946](https://github.com/SanderVocke/shoopdaloop/pull/946) is now retargeted to
  **`feat/backend-replacement`**. Reconcile the implementation with that target
  (inspected at `32179102`), including its runtime profiling and host-manager changes.
  Keep the original `2e19210b` implementation base as provenance.
  Reconciliation was merged in `3fcddfc1`. The first CI run exposed an outdated
  profiling benchmark initializer and browser artifact contract, plus small-stack
  failures in native startup and JACK session replacement. The follow-up uses
  prepared heap storage for the runtime, avoids large MIDI initialization copies,
  and validates indexed MIDI without reconstructing unused state. Focused local
  evidence: 1 MiB startup/cleanup, nine MIDI-index tests, and actual JACK replacement
  over all four native carriers pass. CI owns the repeated platform/artifact checks.
- Follow `AGENTS.md`, `.agents/index.md`, `.agents/rules/{mandates,style}.md` and
  `.agents/info/{build,test}.md` during execution. No merge is authorized here.

## Testing strategy: focused local checks, broader CI coverage

Updated at the user's request to shorten the development loop. This strategy
supersedes the earlier expectation of running full local regression, platform and
feature matrices at each milestone. The staged verification lists below define
coverage to deliver across local tests and CI; they are not a local checklist to
repeat after every change. Product scope and correctness requirements remain.

- **During implementation:** run the smallest meaningful test set for the changed
  behavior. Prefer a filtered existing test or one production-path fixture with a
  clear audio/MIDI/state oracle. Add a regression for a demonstrated failure or a
  material new boundary; avoid duplicating the same case across every host, width
  and execution environment locally.
- **Before a Rust milestone commit:** format, build the changed production packages
  with warnings denied, and pass the relevant targeted tests. Run the test-attribute
  check when tests change. Run tracing or dependency/feature audits when their
  inventories or boundaries change. A workspace build or full test suite is not
  required for every local commit or push.
- **Native versus Wasm locally:** use one representative native production route by
  default. Run a filtered Wasm/worklet smoke check when changing a Wasm-specific
  interface, packaging, imports or render-time behavior that the native fixture
  cannot exercise. Do not repeatedly run the full Wasm suite or four feature builds.
- **CI owns broad coverage:** full workspace regressions and warning-denying builds;
  Windows/macOS builds and tests; Linux host/transport integration; Chromium and
  Firefox packaged browser paths; native/Wasm neither/FX/synth/both build and startup
  matrices; excluded-asset/import checks; complete policy/boundary inventories; and
  selected Miri lifecycle checks. Use existing jobs and fixtures where possible.
  Startup/resource and callback-cost measurements can run once in a representative
  CI job, with configuration and limitations recorded.
- **Get CI feedback early:** push compilable milestones and open a draft PR before
  the entire implementation is finished. Continue development while CI runs. For a
  failure, reproduce the affected test or job locally when useful, fix it, and rerun
  that targeted check; let CI repeat the broad matrices.
- **Completion:** required CI and applicable automated review still need to pass on
  the delivered revision. Record job links and platform limitations honestly. Do
  not substitute mocks for platform evidence or weaken audio, MIDI, lifetime,
  failure, optionality or persistence assertions to make checks faster.

## Goals and scope

Restore built-in FunDSP FX and OxiSynth in the production host, using synchronous
render-thread backends behind a production `BufferedProcessor`. Their audio and
MIDI stay in the host's graph. Both implementations are independently optional
at build time and host startup; the host advertises the effective capabilities.

Restore the dry/wet track foundation needed to use these processors as on master:
creation/removal, channel roles, monitoring, ordinary dry/wet capture and wet
playback, dry-through-wet playback, dry-into-wet rerecording, default actions,
track controls, editors/MIDI Learn, role-aware media I/O, persistence and recovery.
Include External dry/wet tracks as the other existing topology of that foundation,
using existing host ports rather than introducing a processor transport. Include
the existing global FX-control MIDI input for track destinations only.

Native CPAL/JACK/dummy hosts and applicable browser AudioWorklet hosts are in
scope, including split native hosts and hosted/self-contained browser delivery.
Browser Wasm can use `std`; OxiSynth is not restricted to native solely because it
requires `std`. Minimal/core and actual `no_std` targets must not acquire it.
Retain the replacement engine's existing negotiated channel capacity (currently
at most eight audio channels per direction); increasing that ceiling is separate
work. A maximum-width dry/wet track must support eight dry plus eight wet channels.

Bus processors, bus FX editors or MIDI destinations, Carla, asynchronous DSP,
audio/MIDI IPC to processor workers, live enable/disable of host-wide processor
support, live batching changes, general graph latency compensation, and unrelated
parity work remain out of scope. Already-supported track-to-bus output routing
continues to work. Restore the latency/take bookkeeping needed for correct
processed tracks; the general P09/P10 configuration controls remain separate.
Documents requiring other unsupported features still refuse transactionally.

## Immutable goals and acceptance criteria

These goals and criteria must not change without explicit user approval.

1. **Host execution and isolation.** Real FX and synth processing runs in the
   production host's render context through `BufferedProcessor`, without an
   audio/MIDI round trip to the UI/service or another process. Production does not
   depend on the legacy `shoop_engine` or reference application. `rt_runtime`,
   `rt_buffered`, `shoop_loops` and the engine core retain `no_std + alloc` and
   their dependency boundaries. OxiSynth may use `std` in an optional host package.
2. **Independent optionality.** Builds support neither processor, FX only, synth
   only, or both. With both compiled, startup independently enables either,
   neither, or both. An excluded implementation brings no normal/build dependency
   or packaged asset into the corresponding minimal host artifact. A startup-
   disabled implementation prepares no DSP instances or decoded resources.
   Explicit unsupported startup requests fail clearly; normal minimal startup
   succeeds. UI/Lua and session admission use host truth, independent of the
   client's build. Startup settings survive the existing configuration/launch
   round trips and take effect only for a newly started host.
3. **Master's valid shapes.** Built-in FX supports matching mono/stereo/N dry/wet
   audio widths with required dry MIDI; OxiSynth retains exactly two dry audio,
   two wet audio and one dry MIDI channel, ignoring dry audio samples. External
   tracks retain independently sized dry/wet sides and optional dry MIDI where
   the host supports them. Advertise finite existing host channel/instance/memory
   limits and reject invalid shapes before mutation. Dry and wet widths each use
   their negotiated limit: do not halve it by flattening both into one bank.
   All valid widths up to the advertised maximum work, not just stereo presets.
4. **Complete dry/wet behavior.** Preserve master/reference routing and capture
   semantics across monitoring, ordinary record/play/stop, dry-through-wet,
   dry-into-wet, immediate/scheduled transitions, multiple loops and supported
   grab/replacement interactions. Rerecording preserves dry media and replaces
   wet media, forces monitoring off as in the reference, and respects transition
   cancellation. Default clicks and Lua default playback follow the track's
   chosen default; explicit Play remains regular wet playback. Direct/trigger
   tracks keep their existing behavior.
5. **DSP and controls.** Preserve the FX stage order, types, continuous controls,
   smoothing, independent stage bypass and skipped disabled DSP. Preserve synth
   presets, Panic, sends, supported MIDI filtering and attribution. Restore
   embedded editors, per-track activation and state restore, validated
   learn/assign/remove/clear mappings, and confirmed control state including
   MIDI-origin changes. Capture the processor state associated with a recorded
   wet take and restore it through the existing loop action; a failed restore
   leaves the current processor usable. Track-only global FX control
   keeps master's filtering, additive dual-routing and inactive-value retention;
   it neither records control-only traffic nor wakes sleeping audio DSP.
6. **Timing and realtime safety.** Preserve exact-Q graph processing and the
   buffered delay `L = (batch_quanta + slack_quanta) * Q`. Ship real processors
   initially with M=1/S=0; keep the adapter's general timing contract tested.
   Report node and host delay separately. Correct dry/wet playback/capture and
   take alignment account for this delay without cumulative drift or silently
   changing an active take. Preserve MIDI offsets and equal-time order without
   new buffering-induced event quantization. All render/operation/admission and
   worklet control-pump paths allocate/free nothing, never block or perform I/O,
   and have finite work/storage bounds, including cold first notes and new voice
   peaks. Synchronous slack is not an execution-time allowance.
7. **Lifetime, failure and capacity.** Preparation, activation, sleep/wake, reset,
   reuse and removal cannot expose stale voices, tails, buffered MIDI/audio or
   another track's state. Ordinary routing changes do not accidentally reset
   processors. Bounded errors, terminal outcomes, MIDI cleanup and disposal follow
   the existing runtime contracts. Capacity/preparation/control failures preserve
   the old usable track/session and unrelated audio, release reservations, and
   refuse stale generations. Default startup budgets provide replacement capacity
   for an admitted session; no temporary overcommit or stop-and-rebuild loading.
8. **Persistence, media and recovery.** Structure-only and media-bearing save/load
   and New Session preserve topology, channel-role order, default mode, FX state,
   presets/sends, mappings, connections/missing-port intent and recorded dry/wet
   media/take metadata, including the take's recorded processor state. Role-aware
   audio/MIDI import, replacement and export remain
   correct. Disabled/unavailable processors, malformed state and exhausted candidate
   capacity refuse the whole load while the old session keeps running. UI loss
   does not stop admitted work or reset DSP; reconnect reconstructs confirmed state
   without replay. Host replacement renegotiates capabilities and fences old IDs.
9. **Public usability and delivery.** Actual add-track forms, controls, Connections,
   editors, UI/Lua actions and packaged resources work through the production
   service on native and browser paths. Tests prove audio/MIDI results, not merely
   descriptor presence. Required regression/platform CI and automated PR review
   are completed with evidence tied to the delivered revision. Keep P07 explicitly
   partial for Carla and P08 open for bus FX; close other rows only against their
   full acceptance criteria and evidence.

## Design rules and implementation boundaries

- **Portable seam, optional implementations.** Extract reusable DSP, MIDI mapping,
  preset/state definitions and resources from the old engine into focused packages;
  keep reference fixtures runnable against the extracted code. The buffered backend
  interface belongs to `rt_buffered`; concrete FunDSP/OxiSynth implementations and
  resource preparation belong to optional host processor packages. The engine
  accepts prepared processors through a small portable interface/node extension;
  it must not import concrete DSP libraries, application contracts or OS services.
  Keep protocol/state types usable without either DSP feature. Do not weaken
  dependency audits to admit the legacy engine into production.
- **Production adapter.** Generalize the test-only stereo/in-place seam to validated
  input/output shapes, including zero-port sides, consumed MIDI and asymmetric
  external/test shapes. Backends receive prepared batch storage and bounded events,
  never retain borrowed graph buffers, and declare additional fixed latency and
  their capacity needs. Keep collection, source-frame/generation labels, ordered
  completion, scheduled release and overflow policy in the adapter. There is one
  direct/unified processing core and one real backend path; preserve the dummy as
  a test/example. Do not invent an asynchronous transport protocol now.
- **Controls have a source-frame contract.** Carry bounded parameter/MIDI events
  with their batch-relative offsets and stable ordering; split DSP work where
  required. Controls must not retroactively affect already-computed output, or be
  applied to older collected audio merely because submission happens later.
  Distinguish accepted, applied and projected state. State capture includes applied
  MIDI-learned values at a coherent boundary, rather than stale UI-side mirrors.
- **Prepared lifetime on every host.** Use host-owned finite processor slot banks
  prepared before rendering, with explicit per-type instance limits and aggregate
  memory budget. Rack slots support any admitted width up to their prepared maximum,
  allocating needed stereo/non-stereo state in advance to preserve master DSP.
  Startup-disabled types have zero slots. Reserve independent active/candidate
  owners sufficient for transactional replacement, and account for retiring owners.
  Claim/configure/publish slots at an acknowledged graph/domain cut; reuse only after
  MIDI cleanup, reset and retirement complete. No construction/destruction in the
  worklet's render-time record pump. Large reset/cleanup jobs must be split into
  budgeted work while the slot is inactive; final destruction is quiescent/off-render.
- **OxiSynth preparation is a prerequisite.** Pinned 0.1.0 starts its voice `Vec`
  empty and grows it on note-on, and interpolation tables use `LazyLock`. Existing
  allocation tests warm notes before guarding. Add an explicit deterministic
  preparation API, using a minimal pinned upstream revision or a documented local
  patch if needed: reserve the full configured voice pool, initialize every render-
  reachable lazy table and pre-touch storage before activation. Audit voice creation,
  stealing, preset selection/reset, logging and final-owner drops; reserving only
  output buffers or warming a single note is insufficient. Preserve 256-voice
  reference behavior by default; any lower advertised host limit is explicit.
- **Resource accounting and configuration.** Name separate Cargo features such as
  `builtin-fx` and `oxisynth`; do not reuse Carla's `native-fx`. Forward them through
  host/app packaging and the separate worklet build. Shared host startup settings
  carry independent auto/on/off policy, instance capacities and memory limits.
  Product builds default to both available/enabled when supported; minimal builds
  start with no accidental enable request. Explicit On for a missing implementation
  or unsupported actual sample rate refuses with a reason. Auto may advertise it
  unavailable. Capability resolution uses the actual negotiated sample rate and
  resources. Count buffered storage, FX delay/reverb state, voice pools, per-instance
  decoded SoundFonts, candidate banks and metadata; do not mistake the adapter's
  32 MiB bound or the 5.7 MiB embedded asset for total synth/host memory cost.
- **Dry/wet composition and protocol.** Keep logical dry/wet roles distinct from
  physical prepared slots. Inspect `shoop_loops` playback/capture primitives before
  extending them: dry/wet is currently absent from the production RT protocol and
  must become an authoritative operation, not a client-only mode label. Coordinate
  role-specific loop playback/capture and processor activation in one planner-owned
  transition; count a logical wrap once. Use finite versioned messages, limits,
  snapshots and replacement records throughout both app and RT protocols. Runtime
  pointers/trait objects never cross a wire. Native host port publication distinguishes
  external sends/returns from private hosted endpoints. Reuse ordinary wet output
  routes to existing buses without adding processors to buses.
- **Latency and activity.** Use the actual buffered delay plus declared backend
  delay for processed-source alignment and persisted future-take policy; account
  for host delay once. Keep recorded wet playback from being processed twice.
  Specify startup silence, delayed tail handling, preroll, stop/panic, suspension
  and reactivation with note cleanup and fresh stream origins. Do not use the current
  quiescent-only `reset` during live processing without a safe lifecycle extension.
  Preserve reference tail/sleep behavior and introduce no unintended mute or stale
  release. General manual latency/timeline editing remains separately gated.
- **Platform and recovery.** Wire startup settings through native host config,
  Host Manager/in-process launch and browser worklet config/build scripts. The
  import-free worklet contract and bounded `process()` pump remain intact; no new
  imports/threads are justified merely by linking OxiSynth's `std`. Package the
  SoundFont, presets and notices only in synth-enabled host artifacts; client editor
  metadata comes from the host and must not embed the SoundFont. Service ownership
  and generation-fenced recovery remain independent of any UI lifetime.

## Staged implementation

Stages are ordered; each consumes the verified interfaces of earlier stages.
Each stage includes its own verification and a commit before moving on.

### 1. Establish the branch and executable parity contract

- [x] Create the implementation branch, record base/reference SHAs, and add a narrow
  behavior/evidence document at `docs/native_session_recovery/track-processors.md`.
  Record exact default pools/budgets, sample-rate support and effective feature matrix
  there before implementing admission. Budget active and full replacement sessions.
- [x] Turn the reference topology/routing truth tables, channel-role order, loop-mode
  semantics, sleep/tail and recorded-take behavior into the production acceptance
  matrix. Distinguish processor shape constraints from negotiated host limits.
- [x] Map existing primitives and missing work in RT mode/schema, engine graph/pools,
  backend/session adapter, service projection/admission and persistence. Identify
  compatibility/version changes; keep unrelated unsupported features refused.
- [x] Verify against named reference tests listed below and master documentation;
  establish baseline regressions. Commit the contract and plan.

### 2. Promote BufferedProcessor to a reusable production node

- [x] Implement the public prepared synchronous backend contract, arbitrary validated
  port counts, separate bounded input/output/event/control capacities, generic
  construction and direct/unified entry points. Add live activation/deactivation,
  drain/abort/reset and retirement operations with explicit generation/timing rules.
- [x] Preserve exact source-time release for M/S, MIDI cleanup and all existing
  overflow/error semantics. Reject arithmetic/shape/storage limits before mutation.
  Keep lifecycle work bounded and distinguish ordinary graph epochs from resets.
- [x] Verify Q=1/16/32/64/128, M=1/2/3, S=0/1/>M; mono/N/asymmetric/zero-port
  shapes; Q/B boundary MIDI and control events; startup/tail; mute/bypass; sleep/wake;
  removal/reuse; exact-fit/overflow; callback partitions and zero-length callbacks.
  Compare direct and unified paths to independent sample/event expectations under
  allocation/deallocation guards. Extend the existing production-strip composition
  fixture. Run adapter/runtime boundary audits and native/Wasm tests; commit.

### 3. Extract and prepare both real DSP backends

- [x] Extract FX, synth, processor-owned mapping/state and resource recipes without
  pulling `shoop_engine` into production. Keep legacy tests/imports functional and
  serialization/preset IDs compatible. Separate host-only assets from client metadata.
- [x] Implement both synchronous buffered backends and bounded sample-relative
  controls. Preallocate FunDSP units/delay lines for every admitted rack width and
  stage/type change. Fix OxiSynth voice allocation/lazy initialization through the
  preparation API; document/pin any upstream or local patch and its build provenance.
- [x] Verify rack stage order, bypass/no-disabled-DSP calls, types/smoothing, stereo
  and independent N-channel behavior, mapping values; synth preset differences,
  dry-audio independence, filtered MIDI, sends and Panic. Compare to reference DSP
  with declared buffering delay, including non-block-aligned note offsets.
- [x] Guard the first post-preparation note, increasing/saturated polyphony and voice
  stealing, all presets, control changes, reset/reuse and error paths from a cold
  process. Count frees as well as allocations; prevent third-party render logging
  from reaching blocking sinks. Test unsupported sample rates explicitly. Verify
  neither/FX/synth/both dependencies and Wasm compatibility; commit.

### 4. Add host startup policy, prepared banks and truthful advertisement

- [x] Add independent feature forwarding and shared startup configuration, validation,
  native/Host Manager launch controls, browser configuration versioning and worklet
  feature forwarding. Show startup-only settings in existing host launch flows;
  reconcile the current settings allowance of 16 channels with the engine ceiling
  of eight so configuration, advertised limits and admission agree.
- [x] Prepare finite active/candidate processor banks and resources before callbacks;
  implement bounded claim/return/reset with generations. Wire the portable prepared
  node seam into `EngineNode`/pool composition without concrete DSP dependencies.
- [ ] Publish effective per-type descriptors, preset metadata, instance/width/resource
  limits and useful unavailable reasons through engine/backend/service snapshots.
  Keep the bus processor catalog empty and BusFx disabled. Clients never infer host
  support from their own Cargo features. Validate requests on the authoritative side.
- [ ] Verify every build combination and all four startup combinations, explicit
  missing-feature requests, real sample-rate resolution, at-limit and one-over-limit
  admission, disabled startup with zero DSP preparation, client/host build mismatch,
  config/launch round trips and no Wasm imports or callback heap activity. Measure
  cold startup time against host readiness deadlines and memory against the complete
  declared bank budget. Commit.

Startup/bank milestone: host factories, native/worklet wiring, versioned startup
configuration and catalog transport are implemented. Slots have generation-fenced
claim/control/start/retire operations. Retirement clears one prepared DSP unit per
cleanup turn, with at most one turn across the whole bank per quantum. Capability
bits remain gated until graph, service and persistence admission are complete.
Library/configuration tests and the four build combinations pass; the linked
worklet has no Wasm imports. Remaining Stage 4 admission and public projection
checks run alongside Stages 5–7.

### 5. Implement dry/wet graph roles, loop modes and latency

- [ ] Extend RT protocol/codecs, engine model/graph/pools and adapter track/port mappings
  for distinct dry audio/MIDI, processor input/output and wet capture/playback. Restore
  External sends/returns and native dynamic publication; hosted endpoints stay internal.
  Carry roles through loop creation, content reads/meters and resynchronization.
- [ ] Implement authoritative dry-through-wet/rerecord transitions and per-role capture
  policy over loop primitives. Preserve dry takes during wet rerecord, ordinary wet
  playback, input gain/monitoring/mute/balance, reference prerouting and multiple-loop
  behavior. Integrate current grab/replacement without treating them as new features.
- [ ] Implement automatic buffered/host latency and future-take alignment needed by
  these modes, including audio/MIDI preroll, initial silence and pending-output tails.
  Keep active take metadata immutable and compensate each delay once. Restore
  default-mode actions while keeping explicit Play regular.
- [ ] Verify deterministic audio impulses and MIDI note pairs through actual FX/synth
  nodes for every mode and reference routing case, with aligned/nonaligned transitions,
  record-to-play, canceled scheduling, wet-only playback, looping/rerecord across wraps,
  simultaneous loops, dry-media identity preservation, no double processing, no drift,
  and no stuck notes. Cover External unequal widths, all admitted FX widths and synth
  shape; prove maximum dry plus maximum wet is supported simultaneously. Use a targeted native fixture for local failure/partition regressions; cover
  CPAL/JACK/dummy and worklet partitioning in CI; commit.

Immediate-wake prerequisite: FX delay invalidation and a pinned opt-in FunDSP
reset patch eliminate delay-length scans on sleep/wake. Opaque backends now have a
settings-preserving suspend operation. Retirement remains incremental; a clean
same-owner wake preserves controls and cannot release old voices or buffered data.
Native and Wasm DSP checks pass; production routing/activity is still pending.

Loop/media foundation milestone: prepared bindings now have independent primary
and auxiliary audio widths. One resolver owns dry-through-wet and wet-rerecord
modes, including one wrap clock, scheduling and cancellation. Wet rerecord forks
only wet audio using the existing COW stores, retains dry audio/MIDI leases and
preserves take length. Capture groups have separate prepared role variants;
reserved snapshot credits protect edit completion. Native and Wasm primitive
regressions cover 8+8 channels and unequal side widths. Production graph routing,
processor activity/latency and end-to-end host admission remain pending.

Production hosted-graph milestone: RT protocol v10 admits generation-owned FX and
synth tracks against the effective per-type limit. The graph separates dry live
input/playback, processor sends, computed wet capture and delayed wet output.
Source-aligned three-role capture shares one planner cut; ordinary wet playback
has a pure node delay and bypasses DSP. Slots sleep only after releasing pending
output, wake with fresh stream origins, and retire after graph removal. Controls
accepted at a sleep boundary survive draining. Logical stereo balance is separate
from each role's prepared channel capacity. An edit-only COW store reserves one
quarter of the configured media arena when hosted processors need wet rerecording
without retrospective history. Direct/shared-history regression suites still pass.
Real FX/synth graph fixtures cover widths 1–8, record/wet play/dry play/rerecord,
voice cleanup, exact impulse delay under irregular callbacks, and sample-exact
synth comparison with nonaligned note pairs. This milestone does not close Stage 5:
External port publication, complete transition/failure coverage, take metadata and
public integration remain outstanding. Host capability bits remain gated.

External core milestone: RT protocol v11 adds AudioSend/AudioReturn/MidiSend and
independent connection banks/fades. External tracks capture wet return, dry input
and dry MIDI, play wet without sends, route dry playback to sends, and rerecord wet
without changing dry media. Generic External preparation is independent of both
DSP build/startup policies; minimal library engines can leave it disabled. Worklet
configuration v5 carries the preparation flag. Native product/browser startup
requests it. Native tests cover unequal widths and 8+8, MIDI/sustain cleanup and
cleanup saturation without repeated recovery. Wasm engine/worklet suites pass.
Service/backend port mapping and actual JACK publication still require integration
and validation; this checkpoint does not close the stage or enable capabilities.

Adapter role milestone: track admission and resynchronization now retain topology,
independent External send/return roles and processor slot generations. External
ports use the native role order and naming; hosted tracks expose only dry inputs,
wet outputs and dry MIDI, leaving DSP endpoints inside the graph. Type limits
cover all admitted widths together. Dry playback now passes through the loop's
presence fade and meter before the processor send. Node configuration runs beside
graph preparation, preserving the existing Direct-routing latency limits.
Automatic rerecord monitoring changes update the authoritative model after
execution, retry reliable projection when the outbox is full, and survive resync.
This remains internal foundation work: capabilities are still gated pending
public controls, media/persistence and end-to-end validation.

Shape and continuity milestone: External 0/8, 8/0 and MIDI-only tracks now have
executable record/wet-play/dry-play/rerecord coverage. It exposed a general cursor
handoff bug when rerecord committed a retained dry MIDI prefix and returned directly
to dry playback. The MIDI renderer now resumes the equivalent retained prefix at
its exact record boundary, without cleanup, controller restore or note retrigger;
changed content still requires a discontinuity. Loop meters use the active role's
logical width and clear unreported lanes. Stereo-balance availability remains tied
to the wet side as in the reference; non-stereo meters show the loudest channel.
Native 610-test regressions and real-DSP service fixtures pass. Wasm covers the
retained-prefix case, core 83, backend 46 and both-DSP host 22 tests. General media
writes, grab/candidate interactions and transactional persistence remain open.

### 6. Restore service operations, editors and track control MIDI

- [ ] Enable DryWetTracks/DryWetModes/TrackFx only with real end-to-end support; replace
  unconditional service refusals and empty track catalogs. Restore pending/results,
  confirmed projection and UI/Lua actions for creation/removal/default modes, track
  controls/processor activation/state restore, FX parameters, synth presets/sends/Panic
  and mappings. Editor visibility stays transient; unsupported external-UI/recovery/
  process-log facets must not be advertised for the built-ins.
- [ ] Reconnect surviving add-track/editor/Connections widgets to the production route.
  Restore input/output metering and External port visibility; show effective startup
  availability/limits. Keep Carla and bus FX controls unavailable.
- [ ] Restore track MIDI Learn and the global FX-control input with reference filtering,
  additive routing, idle-value retention and bounded saturation handling. MIDI-origin
  values enter authoritative projection/save state; control-only traffic is not recorded.
- [ ] Verify actual public UI and Lua default/explicit actions, each editor control class,
  learned/removed mappings, stale/invalid controls, operation supersession/results,
  active/inactive/global+dry MIDI routing and slow/full telemetry. Use distinct tracks
  to detect cross-instance state leaks. Test attach/reconnect during an admitted
  create/control/remove and prove continuing DSP and one terminal outcome; commit.

Processor control milestone: protocol v12 adds authoritative default playback
selection, bounded scalar/full-state controls and coherent applied-state records.
The adapter projects editor values, mappings, actual processing phase and slot
generation from host state. FX controls affect production audio; mapped MIDI
changes and synth host-preset selection survive resynchronization. Editor visibility
is transient. Manual activation clears stream state on deactivation and yields to
subsequent automatic routing changes. Full restores preflight values, shape,
mapping uniqueness and queue capacity before changing DSP; canonical envelope
restore preserves live mappings. DSP encoders and DSP-free clients share the
portable state envelope encoder. Valid restores reset histories and start new
settings at one source boundary, including parameter smoothing initialization.
Native control/codec, graph and existing regressions pass. Service commands,
MIDI Learn/global controls, recorded-take state and public capability admission
are still outstanding, so no Stage 6 checkbox is closed by this milestone.

Global MIDI milestone: protocol v13 gives the global control input an authoritative
host connection mask and resync records. Hosted and External tracks filter global
CC/channel-pressure/pitch traffic, keep bounded latest values while inactive, and
merge deferred, ordinary and current global events in reference order. Global
traffic is excluded from capture; a source connected to both inputs stays additive.
Idle dry MIDI updates explicit mappings without rendering DSP or creating voices.
Global overflow cannot panic ordinary synth voices. External stop cleanup remains
audible to the peer even when audio demand falls. Prepared banks account for the
additional fixed storage; snapshots carry bounded retention/saturation counters.
Native and Wasm graph/adapter/protocol regressions pass. MIDI Learn, service/UI
commands and persistence admission remain open, so capabilities remain gated.

Service integration milestone: the service projects the host catalog, confirmed
topologies, independent input/output widths, processor editor state and default
playback choices. It forwards typed FX/synth/editor/mapping operations and resolves
default UI/Lua actions per track. Ordered private default-mode intent preserves
rapid reversals before acknowledgement without optimistic confirmed state. Rerecord
gestures schedule the reference number of sync cycles and restore the previous
mode. Protocol v14 carries bounded latest dry-input MIDI observations for the
existing Learn widgets; slot reuse clears them. Wire-level External tests cover
unequal widths, default reversals, default actions and explicit wet Play; core
heap-guarded tests cover MIDI retention/injection/reuse. These tests explicitly
expose foundation capabilities in a fixture while production admission stays gated
until persistence, media and recorded-state support is complete.

### 7. Complete persistence, media and replacement/recovery

- [ ] Extend persistence images/codecs, service capture/mapping and adapter replacement
  staging for topology/port roles/default mode, processor state/mappings, dry/wet media,
  track-only global input intent and required latency/take metadata. Keep stable
  source-to-live ID mappings and absent-port intent; do not serialize runtime owners.
- [x] Capture applied processor state at the reference wet-take recording boundary,
  retaining it independently of later edits. Restore it through
  `LoopAction::RestoreRecordedFxState` using a preflighted atomic state change;
  failed restoration cannot damage the live processor or recorded media.
- [ ] Restore dry/wet audio channel import/export labels/order, exact MIDI/SMF routes,
  content replacement and recorded-take state. Stage processor candidates in the
  reserved bank and publish graph, state and media atomically with existing fences.
- [ ] Verify structure-only and full save/load/save for mono/N FX, synth and unequal
  External roles; changed presets, mapped MIDI values and recorded/rerecorded takes;
  successful/failed recorded-state restoration, role selection/import remapping,
  sample-rate consent and rebuilt DSP at the accepted host rate. Loaded loops remain
  stopped under the existing contract.
- [ ] Verify malformed/old supported state, unavailable startup types, unsupported
  document dependencies, exhausted budgets, cancellation, failed preparation and
  replacement at full admitted capacity. Old audio/media/state must remain usable;
  candidate reservations must return. Cover UI loss before/after acceptance/commit,
  repeated reconnect without replay, New Session and fresh-host capability changes.
  No host-wide stop/restart or stale buffered output on successful replacement; commit.

Recorded-take milestone: protocol v15 projects immutable applied settings for
committed wet takes and reconstructs them on resync. Buffered backends can honor
bounded source-boundary observers after equal-time parameter/MIDI application and
before rendering the sample. The real FX/synth implementations use this seam;
M=2/S=1 collection tests independently cover exact observation offsets. Two fixed
metadata banks preserve the previous take while recording a candidate. Metadata
storage is included in startup reservation. A fault aborts dependent capture,
returns its reserved leases and retains old media/state; unrelated Direct audio
and playback of the old wet take continue. Service loop actions restore the
canonical recorded state atomically while preserving live mappings. Native
real-service wire fixtures cover both editors, learned values, mapping validation,
activation, malformed restore, preset restoration and UI loss during a control.
The External initial bus route now selects wet outputs and excludes dry sends.
These fixtures explicitly expose foundation capability bits while production
admission remains gated. Media-bearing save/load, imported take metadata and
replacement/recovery validation are still outstanding.

Role-aware read milestone: protocol v16 adds an audio-roles bulk selector. One
export retains both dry and wet snapshots through the full read, even if the loop
is cleared midway. The render reader interleaves both prepared banks in reference
order and handles partial frames across finite copy budgets without allocating.
The adapter selects independent logical widths from each bank; the service media
path includes both sides in its budget. Native and Wasm core fixtures cover 8+8,
small pages, and clear during export. Actual FX N=3 and synth service-wire fixtures
check the ordered dry samples and computed wet results. Existing Direct bulk paths
remain covered. Role-aware writes, candidate replacement and persistence codecs
are still open; public admission remains gated.

Role-aware write/replacement milestone: protocol v17 carries independent dry/wet
uploads and recorded take settings. Canonical validation budgets both prepared
banks; selected edits preserve the opposite role and MIDI. Processed candidate
strips claim reserved inactive banks, restore width/settings/mappings before the
graph cut, and publish loaded take metadata only with the new generation. The
adapter resynchronizes committed owners before acknowledging a successful load.
Repeated loads wait for bounded retirement; cancellation after candidate Claim
and unavailable synth preset preparation preserve the old session. The real
External 3→1, FX N=3 and synth fixtures check six repeated loads each and force RT
media reexports. Service images/document conversion and public admission are still
outstanding; this does not close Stage 7. The required native workspace gate
passes 2,819 tests (seven existing skips); warning-denying build and boundary/
inventory checks pass. Node/Wasm passes backend 47, core 83, protocol 6 and
both-DSP host 22 tests.

Service/file milestone: application protocol v26 carries channel roles, topology,
default playback, canonical applied settings, mappings and immutable recorded-take
settings. Capture freezes settings at the accepted cut while later live controls
continue. The service and archive codec now round-trip real FX N=3, synth and
External 3→1 sessions, including fresh IDs, stopped loaded loops, exact role media,
MIDI state, mapping preservation and imported take restoration. Selected audio
export keeps role labels and original numbering in the requested order; import
mapping supports all sixteen logical channels. The production file workflow and
waveform labels use the same role order. Synchronized family imports, automatic
alignment projection and final public admission remain open.

### 8. Final end-to-end validation, evidence and PR delivery

- [ ] Exercise a focused packaged production smoke path locally when packaging or
  public flows change; use CI for the complete acceptance matrix: launch hosts with both/either/
  neither processor enabled; create mono/N FX and synth tracks through the real UI;
  edit/learn controls; monitor/record/play dry and wet, rerecord, save/reload/export;
  disconnect/reattach and verify continuing sound and confirmed state. Include External
  routing on supported native hosts and ordinary wet output to existing buses.
- [ ] Run targeted native carrier regressions locally when those paths change. CI
  covers the full typed/in-process and socket/shared-memory scenarios, Linux
  dummy/CPAL/JACK integration, Windows/macOS builds/tests, and real hosted/self-contained
  Chromium plus existing Firefox browser acceptance. Use Web MIDI fixtures/audio-output
  assertions where hardware is unavailable; do not call a mock-only path platform proof.
- [ ] Measure bounded callback cost at maximum admitted channel/voice/control load,
  startup latency and memory. Report hardware/configuration, peak cost/xruns and
  limits; synthetic stream slack is not a deadline guarantee. Use one representative
  measurement job and selected CI Miri
  coverage for portable ownership/lifecycle logic using existing tooling; do not
  claim that it certifies the third-party DSP implementation.
- [ ] Complete the CI gates below and the focused local checks, record links and
  exact revisions in `track-processors.md`, and update README/usage/startup docs and
  the parity ledger. P06 closes only with complete scoped evidence; P07 names this
  restoration while remaining open for Carla. P01/P02/P11 expand their supported
  document/media subset; P08 and general P09/P10 stay open as applicable. Preserve
  unsupported-feature diagnostics and reference provenance.
- [ ] Commit and push compilable milestones; create a draft PR early so CI runs
  alongside implementation. Update it with final scope and validation evidence and
  mark it ready when the implementation is complete. Work until required CI is green;
  inspect automated reviews, address applicable feedback and rerun affected checks
  repeatedly until the reviewer approves. Record unavailable checks honestly; a
  prior milestone's macOS waiver does not apply to this work. Leave the PR reviewable
  without merging unless instructed.

## Reference anchors and verification gates

Use retained tests as executable contracts and add equivalent production-route tests;
passing reference tests alone is insufficient:

- `shoop_app_reference`: `application_adds_external_dry_wet_tracks_from_processor_capabilities`,
  `dry_wet_track_default_action_is_dynamic_while_explicit_play_stays_explicit`,
  `dry_wet_media_io_maps_and_exports_role_order_without_flattening`,
  `builtin_fx_session_round_trip_preserves_n_channels_controls_and_assignments`,
  `oxisynth_session_round_trip_preserves_selected_preset`,
  `processed_track_session_round_trip_preserves_roles_state_and_recorded_take`, and
  `failed_recorded_fx_restore_leaves_the_processed_track_usable`. The last two use
  a Carla-shaped fake; port their generic state/transaction expectations to the
  real built-ins without enabling Carla.
- `shoop_engine/src/{builtin_fx,oxisynth}.rs` and `builtin_fx/`: reference DSP tests;
  `shoop_backend` mapping/routing and descriptor tests; engine global-FX/no-allocation
  fixtures; master `docs/source/usage.trackcontrols.rst`; retained native built-in
  save/reload frontend canary and legacy browser acceptance.
- Production: `rt_buffered`, `shoop_loops`, `shoop_rt_engine`, `shoop_rt_backend`,
  `shoop_app` service/persistence tests, `shoop_host`, `shoop_rt_host_worklet`, and
  `shoopdaloop` packaged/UI probes. Register extracted package tests with the same
  Wasm and tracing inventories.

Use the repository's selected build environment. The normal local loop is:

```sh
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build -p <changed-production-package>
cargo test -p <affected-package> <focused-test-filter>
# When Rust tests change:
python3 scripts/check_shoop_test_usage.py
```

Choose the relevant packages and filters for each change. For example, a role-aware
import fix can use the real-host adapter media/replacement fixture; a codec change
can use its round-trip and malformed-input tests plus one service save/load test.
Run additional tests when a failure or unresolved concern calls for them. Do not
repeat previously green broad suites solely because another milestone was reached.

CI runs the broader gates and the native/Wasm neither/FX/synth/both feature and
startup matrix:

```sh
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loops_boundary.py
python3 scripts/check_rt_engine_boundary.py
python3 scripts/check_feature_parity.py
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/check_shoop_test_usage.py
python3 scripts/run_wasm_tests.py --runtime node --profile dev
```

Feature-isolated CI jobs prove excluded dependencies/assets and import-free worklets;
a unified workspace feature build is insufficient for that proof. Platform CI uses
the README's native transport/persistence, browser and packaging probes with scoped
FX/synth scenarios. Real-backend jobs omit `SHOOP_ALLOW_MISSING_BACKENDS=1` where
actual host facilities are provided. Keep unavailable checks distinct from passing
checks. Before behavior-changing pushes, pass the affected local tests; CI owns
full regressions and cross-platform validation before final completion.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
