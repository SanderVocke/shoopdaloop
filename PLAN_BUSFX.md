# P08 Bus FX Implementation Plan

Restore P08 Built-in bus FX and global-control MIDI destinations through the replacement backend, including public controls, confirmed service state, bounded rendering, session files, and reconnect recovery. Complete this work using the existing hosted processor infrastructure, independently of other feature integrations.

Base: `feat/backend-replacement`, reviewed at `0b6812259ffc00b9a524bb6095cfc2c6b2112847`. Follow the branch’s [write_plan.md](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/.agents/prompts/write_plan.md), [P08 requirements](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/feature-parity.md), and [hosted processor contract](https://github.com/SanderVocke/shoopdaloop/blob/0b6812259ffc00b9a524bb6095cfc2c6b2112847/docs/native_session_recovery/track-processors.md).

Integration base updated at the user's request to `b874bbbbd3166ce0476e033ec09fe4e763b8a731`. Combine its profiling and rendering changes with bus FX, retaining preview injection before processing and the optimized output gains/meters. RT protocol 25 and app protocol 33 distinguish the combined wire format from both parent branches; update the production and browser fixtures together. Scope and acceptance criteria remain unchanged.

# Goals and scope

Restore Built-in FX selection at bus creation and on existing buses; processing, bypass, parameter/type/stage changes, state restoration, embedded editing and MIDI mappings; and global-control MIDI fanout to buses. Preserve ordinary bus lifecycle, ordering, routes, meters, host connections and Master preview.

Cover native typed and socket carriers, applicable browser AudioWorklet behavior, and negotiated firmware availability. Extend P01/P02 capture and replacement only for bus processor data; other open features remain under their own rows.

Carla hosting, bus integration, packaging, external editor, plugin logs and plugin recovery are explicitly out of scope. This user-approved scope has no dependency on P07 or any other feature integration. Completing A1–A9 is sufficient to close P08 within the Built-in FX scope; update the backlog to reflect this scope at closure. Any future Carla bus work is separate.

General latency compensation, take alignment, new processor types, wider legacy bus capacities and expanded crash/durability guarantees are outside this work. Preserve the existing fixed-quantum timing and recovery contracts.

# Immutable acceptance criteria

These criteria govern execution and may change only with explicit user approval.

| ID | Required result |
| --- | --- |
| A1 | Host-advertised Built-in FX can be selected when creating a bus and inserted, replaced, bypassed and removed on existing buses. Mono, stereo and negotiated wider shapes retain channel order; incompatible processors and shapes refuse. |
| A2 | Deterministic sample oracles prove actual DSP, bypass and restored direct output. Switching preserves upstream routes and unrelated audio. Bus gain, balance, mute, meters, host outputs and Master preview retain their reference behavior and declared processor buffering. |
| A3 | Each bus processor receives each admitted global-control event once per intended lane, independently of the number of routed tracks. Ordering, supported-message filtering, idle/bypass retention, MIDI Learn and bounded overflow behavior are tested. Intentional ordinary/global additive traffic remains additive; global-only controls do not enter loop media. |
| A4 | Public UI and applicable Lua routes use service-owned admission and confirmed outcomes. Selection, active state, editor controls, mappings and errors reconcile with actual host state; capability/catalog availability follows the connected host. |
| A5 | Accepted operations survive supported UI disconnect/reattachment without replay. Stale bus, processor-slot and session generations cannot mutate replacement owners. Refused operations leave the prior graph and processor state usable. |
| A6 | Coherent save/load/save preserves bus processor identity, state, CC mappings, routes, bus order and global-port intent. Structure-only and media-bearing loads use atomic candidates and fresh live IDs; unavailable types, malformed data and exhausted reservations refuse without replacing the active session. Loaded loops retain existing stopped-on-load behavior. |
| A7 | Rendering and callback-reachable lifecycle/control paths remain allocation/free-free, nonblocking and bounded. Prepared budgets cover simultaneous track and bus processors, candidate banks, edges, controls and telemetry. Saturation and repeated creation/replacement/removal preserve unrelated work and return resources to a stable baseline. |
| A8 | Native typed/socket production workflows and actual browser AudioWorklet Built-in workflows pass. Disabled/unsupported native, browser and firmware configurations advertise refusal truthfully. Existing track processors, ordinary buses, persistence and preview remain working. |
| A9 | Reference fixtures and new production tests substantiate the restored behavior. Applicable final-head CI passes, automated review is checked and addressed through approval or its absence/unavailability is reported, and the PR remains unmerged. P08 is marked COMPLETE within the user-approved Built-in FX scope after A1–A9 are satisfied, without requiring other feature integrations. |

# Design rules and constraints

- Reuse `shoop_dsp`, `shoop_host_processors`, `shoop_processor_protocol` and the retained editors/codecs. Keep legacy engine/backend code as executable reference evidence, outside production dependency graphs.
- Generalize processor ownership to a typed track-or-bus target with generations. Bus activity must not accidentally depend on track recording demand or access track-only capture observers.
- Prepare graph nodes, edges, storage and DSP before use. Activate structural changes at the existing admitted boundary; retain the prior graph on refusal. Do not remove live source-to-bus routes merely to insert FX.
- Establish the reference ordering of summing, preview injection, processing, faders, meters and host output before changing the graph. Account for processor buffering in sample expectations; do not introduce general P09/P10 compensation.
- Bus FX uses the existing negotiated prepared FX width. Ordinary buses may be wider; selecting FX beyond the FX width refuses. This preserves existing slot storage and startup budgets without silently increasing default reservations.
- Share and report processor capacity across track and bus owners. Existing defaults are two FX instances and a 256 MiB processor storage ceiling with candidate-bank reservations. Do not assume every bus gets a slot or silently enlarge budgets. Keep processor storage separate from the loop recording media pool.
- Keep global controls as a direct control lane to each destination. Reuse bounded pending-control handling and diagnostics; distinguish unintended graph duplication from intentional ordinary/global delivery.
- Version RT/app wire changes and host/worklet configuration together. Separate editor visibility from render state, and advertise only processor-specific features actually implemented.
- Preserve zero-bus sessions, removable Master identity, dynamic host ports, atomic whole-document refusal and existing recovery exclusions.

# Stage 1 Establish the execution baseline and contracts

- [x] Create `feat/p08-bus-fx` from the current `feat/backend-replacement` head, inspect applicable instructions, and record source/base revisions. Put the execution copy of this plan in `PLAN_BUSFX.md`.
- [x] Map reference bus actions, signal ordering, global MIDI behavior and persistence fields to production owners. Use the retained bus FX fixtures and `global_fx_port_round_trips_legacy_migrates_and_malformed_load_is_transactional`; include insertion-activation and route-preservation regressions.
- [x] Define processor targets, bus demand/bypass/tail behavior, slot accounting, confirmation outcomes and the Built-in-only capability boundary. Record any reference/quantum semantic differences.
- [x] Run focused baseline processor, ordinary-bus and persistence tests. Identify pre-existing failures separately.

Verification: a concrete contract/test map for A1–A9, executable reference cases, and a baseline against which later failures can be attributed. No capability is enabled in this stage.

# Stage 2 Extend protocols and prepared ownership

Depends on Stage 1.

- [x] Extend `shoop_rt_protocol` commands, state/resync records and codecs for bus processor ownership, selection and controls; extend app projection and persistence image schemas as needed. Update protocol versions coherently.
- [x] Generalize prepared slot ownership and lifecycle metadata in `shoop_rt_engine` and resource reporting in `shoop_rt_backend`. Include bus/processor generations and reservations for replacement candidates.
- [x] Budget additional bus graph/control/MIDI/telemetry storage and aggregate track/bus usage. Keep unavailable configurations and older incompatible wire versions explicitly refused.

Verification: codec round trips and malformed/version rejection; mixed track/bus slot exhaustion, stale-generation rejection and reservation rollback. Run changed protocol and preparation packages before broader integration.

# Stage 3 Implement the RT bus processor graph

Depends on Stage 2.

- [x] Extend `shoop_rt_engine::{model,graph,node,pool,processors,topology}` and relevant activation/replacement paths so bus summing and output stages can surround a hosted processor without a signal cycle.
- [x] Implement creation, insertion, replacement, bypass and removal with coherent activation and bounded retirement/reset. Bus owners must use their own activity policy and avoid track capture bindings.
- [x] Preserve bus controls, meters, route switching, dynamic host output publication and preview injection through the defined signal order.

Verification: exact mono/stereo/wide sample oracles, multiple source tracks, concurrent track/bus FX, insertion activation, bypass/removal, preview and routing during playback. Cover failed preparation/activation, repeated reuse, callback heap/destructor guards, and debug/release execution.

# Stage 4 Connect the adapter service and recovery

Depends on Stage 3.

- [x] Implement bus FX in `shoop_rt_backend::{backend,mirror,requests,incoming,processor_controls,processors}`, including processor snapshots, resources and catalogs.
- [x] Replace bus FX refusals in `shoop_app/service/commands.rs`; support creation with FX and all applicable `BusAction` variants. Extend pending effects, publication and projection so admission is distinct from confirmed execution.
- [x] Restore service-owned embedded editor visibility, state restoration and control errors. Advertise the applicable Built-in features and enable `BusFx` only when the negotiated host implements them.
- [x] Reconcile accepted bus operations across UI loss, reconnect and session replacement without reissuing them.

Verification: public service/adapter tests over typed and serialized/socket routes; refusal and stale-result cases; disconnect after admission and around activation; bus removal and session replacement with outstanding controls. Assert actual audio and projected state agree.

# Stage 5 Restore global MIDI and public controls

Depends on Stage 4.

- [x] Feed the existing global-control input directly to each bus processor. Preserve sample/lane ordering, filtering, pending latest values, bypass/reactivation behavior and bounded pressure diagnostics.
- [x] Ensure combined track/bus audio routes never forward the global control lane a second time. Retain legitimate additive ordinary/global events.
- [x] Wire `shoop_egui/bus_controls`, Built-in editor/dialog routing, MIDI Learn and applicable Lua controls to confirmed service commands. Show host limits, unavailable processors and actionable failures.

Verification: one controller with several tracks and buses; multiple audio routes into one bus; distinct intended MIDI lanes; mappings edited/restored; disabled stages and bypass; queue saturation and owner removal. Exercise production UI/Lua dispatch and assert resulting DSP/state, rather than only widget-generated actions.

# Stage 6 Complete persistence and atomic replacement

Depends on Stages 4–5.

- [x] Carry bus type, serialized processor state and MIDI mappings through `shoop_app_protocol/persistence_image`, `shoop_persistence_codec/document`, service capture/mapping and adapter session candidates.
- [x] Reserve and prepare all bus processors before committing a load; restore state/mappings coherently and map saved IDs to fresh live identities. Extend capability diagnostics for unsupported processor types.
- [x] Preserve ordinary controls, routes, global-port migration, preview bus identity, zero buses, missing-host-port intent and existing capture/load fencing.

Verification: save/load/save with simultaneous track and bus FX; mono/wide buses; bypass and CC mappings; structure-only and media-bearing loads; legacy missing-global-port migration. Malformed/duplicate mappings, unavailable types, candidate-bank exhaustion, cancellation and disconnect must leave the active session intact. Compare exact state and unchanged loop media.

# Stage 7 Validate end to end and complete delivery

Depends on Stages 1–6 for P08 closure within the user-approved Built-in FX scope.

- [x] Run the packaged production workflow: create/route tracks and buses, insert and edit FX during playback, send global controls, bypass/replace/remove FX, use Master preview, save/reload, and disconnect/reattach the UI. Verify samples, controls, routes, mappings and resource ownership.
- [x] Run native typed/socket tests and actual browser AudioWorklet Built-in tests, including non-isolated supported hosting, no render-time memory growth and native/browser sample/control comparison with declared buffering. Exercise enabled and disabled host policies and applicable firmware build/capability checks.
- [x] Run focused ownership/lifecycle Miri cases and release measurements for mixed track/bus load, MIDI pressure and repeated replacement. Report capacities, callback tails and failures without claiming unmeasured hardware deadlines.
- [x] Run applicable broader native checks from `VALIDATION.md`: formatting, warning-denying workspace build, full native test suite, feature-parity and dependency-boundary checks, test-usage checker and Python script tests. Use the branch’s current consolidated `check_dependency_boundaries.py`, not retired scripts.
- [ ] Update the feature backlog, processor/persistence contracts and usage guidance to record the user-approved Built-in FX scope and Carla exclusion. Record reproducible evidence and review disposition in PR/CI artifacts. Mark P08 COMPLETE when A1–A9 hold; preserve remaining P07/P09/P10 and other backlog scopes.
- [ ] Push milestone commits, create/update the PR against `feat/backend-replacement`, wait for applicable final-head native/browser/platform CI, fix failures, and rerun affected checks. Recheck CI after every fix.
- [ ] Check for automated review and address feedback repeatedly until approval. Report absent or unavailable review explicitly. Leave the PR unmerged and the worktree clean.

Verification: a criterion-to-test/evidence map on the final PR head, applicable green CI, actual native and browser Built-in execution, documented limits, and review disposition.

# Verification strategy

During iteration, run only the changed layer and its nearest integration boundary: codecs/preparation in Stage 2, deterministic RT and lifecycle tests in Stage 3, service/carrier/recovery in Stage 4, MIDI/public actions in Stage 5, and persistence candidates in Stage 6. Reuse existing reference and processor fixtures; add tests for externally visible behavior and ownership failures rather than mirroring implementation details.

At major integration milestones and final delivery, broaden to the native suite and production workflows. Use CI for the cross-platform matrix, with manual actual-browser and selected Miri checks where required by the current test policy. Preserve workloads and watchdogs; timeouts and skipped required hosts remain incomplete evidence. After a fix, rerun affected tests and final-head CI as needed, without repeating unrelated expensive suites in every edit cycle.

Read `.agents/info/build.md` and the current `VALIDATION.md` before executing commands. Run `cargo fmt --all` and a warning-denying build before Rust commits; run `python3 scripts/check_shoop_test_usage.py` when Rust tests change. Remove the missing-backend allowance for tests requiring actual host facilities.

# Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Push milestones and complete applicable CI and automated review work. Do not merge autonomously.

