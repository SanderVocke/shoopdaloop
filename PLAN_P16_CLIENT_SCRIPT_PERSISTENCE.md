# P16: client-owned script persistence

Status: implementation complete; upstream integration qualification and final
CI/review in progress. PR [#977](https://github.com/SanderVocke/shoopdaloop/pull/977). The ownership contract
was revised with user approval on 2026-11-12. P16 remains OPEN.

Base: `feat/backend-replacement` at `9aee7496`; production behavior was compared
with `master` at `9134eb20`. Upstream is now integrated through `60d8f21c`.
Follow
[Write Plan](.agents/prompts/write_plan.md), the
[P16 contract](docs/native_session_recovery/p16-client-scripts.md), and the
[code investigation](docs/native_session_recovery/p16-investigation.md).

## Goal and scope

Restore session script conversion and complete file persistence while retaining
client ownership of script data, resources and execution. The client joins its
additions to a backend musical capture when saving, and separates those additions
before sending musical data to the backend when loading.

Include exact source export, supported Markdown/PNG resources, save/store/load,
New Session and native/browser workflows. Exclude backend script storage or
attachment transfer, VM serialization/migration, automatic starts, durable client
crash recovery, arbitrary Lua dependency discovery and directory-bundle export.

## Immutable acceptance criteria

- **A1 — Ownership:** script source, resources, inclusion and saved preferences
  stay client-local. Service snapshots, persistence images/transfers, engine
  commands and RT resources contain no script additions. The client archive
  preserves names, document identities, entrypoints, preferences and exact bytes.
- **A2 — Conversion:** include-in-session captures current source plus supported
  companions before changing ownership. Failure, cancellation or stale completion
  preserves the previous data/ownership. Conversion stops the affected VM;
  Session/Ephemeral/Session retains its bundle after source deletion. Removal
  cleans up providers and script-owned activity. Export preserves exact source
  and avoids a duplicate `.lua` suffix.
- **A3 — Save coherence:** each save/store combines an immutable musical capture
  with a frozen copy of that client's script additions for the same project.
  Subsequent edits cannot change the output. Concurrent session replacement
  cannot mix old additions with new music. Platform write/cancel/error semantics
  remain accurate, including browser download handoff.
- **A4 — Load preflight:** validate all archive resources and supported Lua syntax
  without executing scripts, and prepare local installation before musical commit.
  Invalid data, declined conversion, backend refusal and pre-admission cancellation
  leave existing script data and the active musical session intact. Prevent
  old-session script commands from affecting the replacement.
- **A5 — Local installation:** confirmed successful load replaces only session-owned
  client records, with all loaded scripts stopped even if saved as enabled. Preserve
  machine-local User/Bundled/Ephemeral records. Successful New Session or loading an
  empty script section clears only session-owned records. Report complete load
  success only after prepared local installation completes.
- **A6 — Compatibility and execution:** retain valid supported-format bundles even
  when scripting/API/device support prevents execution; keep them inactive and
  exportable. Malformed bundles and unsupported archive formats refuse explicitly.
  Start only through explicit user action after load/recovery, using this client's
  devices and permissions. No automatic start on refresh or reconnection.
- **A7 — Recovery:** admitted musical work finishes under existing host ownership.
  A surviving client reconciles its exact pending load without replay and installs
  its candidate only for confirmed matching success. Unknown outcomes keep affected
  scripts inactive. A replacement client inherits no previous-client scripts;
  saving there includes only its own additions. No claim of cross-process atomicity
  or recovery of additions after client-process loss.
- **A8 — Bounds and platforms:** native and browser public routes obey the same
  semantics and explicit byte/count/concurrency limits. Archive expansion, hashing,
  syntax checks, worker traffic and cleanup do not block the UI/service pumps or
  add work to the audio callback. Existing musical persistence and client script
  data across driver switching continue to work.

## Design rules and implementation boundaries

1. Keep `shoop_app` and its musical persistence schema unchanged for script data.
   Reuse the existing staged musical load, result tracking, confirmed revision and
   epoch fences. No new service script catalog or resource object kind is needed.
   Do not bump service/RT wire versions solely to carry client additions.
2. Add a client-owned data value for script document records and immutable bundles,
   independent of running VMs. Extend `PersistenceIo` beyond its current
   `session_scripts: bool`: provide frozen save input and explicit prepared-load /
   confirmed-install handoffs to the runtime's `ScriptManager`. Keep the runtime
   as the sole owner of VMs and avoid mutable sharing with codec workers.
3. Extend `shoop_persistence_codec` to compose/split complete file bundles. Do not
   merely remove P16 refusal checks: retain the client section explicitly through
   every decode/convert/prepare/encode path. Musical format conversion must leave
   script/resource bytes unchanged. Reuse the existing `.shoop` script manifest
   layout and ownership/hash validation; do not invent a generic extension system.
4. Browser codec-worker traffic is separate from client/service traffic. The worker
   currently reuses musical image messages, which cannot carry additions. Add
   bounded worker-local script descriptor/resource messages and task handles for
   encode/decode handoff. Retain 64 KiB worker envelopes and current message/work
   windows; never send a full resource bundle in one envelope or through the host.
5. Keep current resource limits (16 MiB/file, 64 MiB/script, 256 MiB aggregate,
   10,000 resources/script, 100,000 scan entries), 8 MiB archive metadata and
   512 MiB encoded-file ceilings. Define explicit archive-wide script/resource/entry
   count limits before implementation. The implementation pins 256 session scripts,
   65,536 total resource files and 4096-byte names/paths, independently of musical
   transfer objects. Account for resources in the expanded-byte
   ceiling and peak old/candidate/codec ownership separately from the 256 MiB audio/
   MIDI budget. Do not apply the service's 4096-object limit to client additions,
   silently lower resource limits, or use the reference codec's 16 GiB defaults.
6. Use bounded native workers and cancellable browser jobs. Reuse immutable bundles
   before scanning a directory; generation/project/task checks reject stale results.
   Browser source-only inputs remain source-only; inaccessible required companions
   fail explicitly. Do not turn the reference's thread-per-conversion mechanism
   into an unbounded production job pool.
7. Reserve and validate local replacement records/providers before backend admission
   so confirmation only requires a prepared local swap. Keep old records until
   that decision. Gate old-session script dispatch while commit is in flight and
   discard stale queued operations. Do not tear down VMs merely to validate a file.
   After a refused operation, resume only activity allowed by the existing live
   connection; connection recovery must not restart scripts.
8. Tie pending additions to the host/session, load operation and local project
   generation. Confirmed musical success remains success even if its refreshed view
   is delayed; retain local pending data until it can be matched. An unknown outcome
   must not report complete project load or reuse old scripts against new music.
   Losing the process loses this local context; no new recovery journal is in scope.
9. Separate persisted enabled preference from permission to start a VM. Existing
   `replace_session_scripts` starts enabled records and needs a prepared, stopped
   installation path. Do not serialize resource-provider URIs or native device
   bindings; create providers for the current client. Preserve existing machine
   records rather than recreating their VMs when replacing session scripts.

## Staged implementation

### Stage 1 — Establish client data contracts and acceptance fixtures

- [x] Create `feat/p16-client-script-persistence` from the current upstream branch,
  carrying these approved documentation changes; do not merge unrelated master work.
- [x] Define the client additions value, frozen-save input, prepared-load/install
  handoffs and unknown-outcome handling in the runtime/provider boundary. Pin the
  concrete archive-wide count limits and peak memory accounting described above.
- [x] Port the three named production reference scenarios to appropriate production
  test fixtures, preserving exact-byte/resource assertions while replacing their
  auto-run expectation with explicit-start behavior. Add the separate-client
  recovery/save scenario so ownership cannot drift back into the service.
- [x] Commit the first verified milestone; open a draft PR against
  `feat/backend-replacement` after the first useful push.

Verification: focused fixtures for A1/A6/A7, compile the changed client interfaces,
and run dependency/capability audits. Do not alter existing P16 guards before the
new paths can preserve all additions. Later stages complete the end-to-end fixtures.

### Stage 2 — Support bounded complete archives on the client

Depends on Stage 1.

- [x] Update `shoop_session/src/archive.rs` bounded encode/decode to charge every
  script record/resource, entry and expanded byte before allocation; reuse existing
  path/hash/owner checks. Keep archive compatibility with existing script-bearing
  sessions and sessions without scripts.
- [x] Update `shoop_persistence_codec/src/{codec,document}.rs` to join additions to
  captured music and return them separately from prepared musical images. Audit
  conversion and all convenience entry points to prevent silent stripping.
- [x] Extend `worker.rs` and browser worker glue with segmented client-only additions
  transfer, cancellation, stale-handle fencing and cleanup. Keep Lua validation and
  VM installation at their client owners.

Verification: A1/A6/A8 codec round trips with independent expected source/Markdown/
PNG bytes; wrong/missing owners and entrypoints, case collisions, path traversal,
hash/size mismatches, undeclared files, truncated ZIPs, zero-byte entry count floods,
and limits at/over boundaries. Test cancellation and worker chunks around envelope
boundaries. Run focused session/codec tests and portable/Node builds before proceeding.

### Stage 3 — Restore local conversion and stopped script preparation

Depends on Stage 1; can be developed independently of Stage 2's archive changes.

- [x] Adapt `ScriptManager` preparation/installation to retain enabled preferences
  while constructing stopped records and reserving providers before commit. Preserve
  machine-local record identities and support inactive incompatible sources.
- [x] Restore bounded off-pump filesystem capture using `conversion_source`,
  `capture_filesystem_bundle` and generation checks. Keep conversion/removal local;
  reuse captured bundles through repeated Session/Ephemeral conversion.
- [x] Fix source export filenames and retain exact bytes for every exportable source.
  Clean up pending conversions, callbacks and provider registrations on removal or
  project change.

Verification: A2/A5/A6 focused resource/scripting/client tests: mutate or remove a
source while scanning, cancel/repeat conversion, delete original directories,
inspect provider-backed Markdown/images, export incompatible sources, and install
enabled scripts without executing callbacks or MIDI side effects.

### Stage 4 — Integrate save/load/New Session with confirmed musical outcomes

Depends on Stages 2 and 3.

- [x] Update `shoop_app_client/src/runtime.rs` and `shoopdaloop/src/persistence_io`
  to pass immutable additions into save/store jobs and retain prepared local load
  candidates. Cover every picker, URL/source, byte-input and empty-session route.
- [x] Freeze additions for each save task and fence project replacement during its
  capture pairing. Keep later edits outside the task. Include scripts only in
  project archives; leave standalone audio/MIDI exports unchanged.
- [x] Integrate syntax/resource preflight, conversion consent and prepared local
  installation with `pump.rs::reconcile_commit` and New Session result handling.
  Stop old-session commands crossing the replacement boundary; distinguish refusal,
  confirmed success and unresolved admitted work. Finalize local records once.
- [x] Replace the P16 boolean refusal guards only after the complete paths work.
  Preserve task/output errors, browser handoff semantics and normal controls.

Verification: A3/A4/A5/A7 against the public production runtime/provider, including
save while editing/playing, conversion decline, backend refusal, cancellation,
provider preparation failure, source changes during capture, empty-session load,
New Session, delayed confirmation, detach before/after admission and stale callbacks.
Assert exact file contents, no script execution, no replay and no wrong-session pairing.

### Stage 5 — Validate complete native/browser workflows and recovery limits

Depends on Stage 4.

- [x] Exercise include script + help/image, delete originals, save/store, load,
  inspect stopped state, export exact source, explicitly start, resolve resources,
  convert to run once and back, and save again. Include musical media and preserve
  unrelated User/Bundled/Ephemeral scripts throughout.
- [x] Run actual native embedded and split host/UI paths over socket/shared memory,
  alongside typed tests. Kill the UI around an admitted musical load; attach a
  replacement client and verify music survives but scripts do not transfer. Save
  from that client and inspect that only its own additions are included.
- [x] Exercise the surviving-client pending-candidate/unknown-result paths and driver
  switching with local script data. Run browser worker/AudioWorklet acceptance for
  archive/resource round trips, explicit starts/dialog images, permission-gated MIDI
  behavior and download completion. Record software/device limitations honestly.

Verification: complete A1–A8 acceptance with independent expected bytes/state and
actual application evidence. Keep deterministic fault tests for admission races;
use real packaged/browser paths to establish integration rather than rerunning the
full platform matrix for each small fix.

### Stage 6 — Complete repository gates, CI and review

Depends on Stage 5.

- [x] Follow `.agents/info/{build,test}.md` and mandatory per-commit/push checks.
  Run `cargo fmt --all`, warning-denying builds and the Rust test-usage checker
  for changed Rust tests. Complete the native suite with nextest 0.9.116:
  `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci`.
  Run backend-dependent qualification without the missing-backend override.
- [x] Run affected portable/Node tests, required Wasm builds and browser smoke tests;
  check that production dependency boundaries and musical service/RT wire schemas
  still exclude script data. Validate the feature-parity ledger.
- [ ] Push the implementation and obtain green required CI on the final head,
  including supported platform builds. Inspect automated PR review after pushes;
  address actionable feedback and repeat until approval, or report an unavailable
  reviewer/remaining blocker explicitly. Rerun checks affected by material fixes.
- [x] Record commands, final implementation SHA and limitations; update
  user/developer/persistence docs and P16 acceptance evidence.
- [ ] Close the parity row after A1–A8 and final delivery gates pass.
  Keep the PR ready for user review; do not merge autonomously.

Testing cadence: run focused changed-package and failure-path tests during each
stage; run full integration gates at the completed feature milestone and after
material cross-layer fixes. Avoid repeating unchanged full suites for documentation
edits. CI supplies the supported cross-platform matrix; local verification targets
the selected platform plus browser/portable paths affected by P16.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution notes

- 2026-11-12: agreed contract committed as `6ed9ee10`; implementation branch
  created and draft PR #977 opened against `feat/backend-replacement`.
- Implementation milestones `d6bd336e` and `7e16fa40` are published. Subsequent
  local commits preserve converted records on catalog refresh (`0c7a0b4b`), repair
  the arithmetic Wasm harness (`f8925062`), confirm legacy native loop creation
  before routing reads (`8039b33f`), and align host fixtures with their configured
  settings and measured construction time (`5b4bcfce`, final implementation SHA).
- Qualification exposed Cargo 1.100's changed OUT_DIR nesting and a missing Wasm
  test dependency; both are corrected. The legacy loop race now waits for its
  exact creation command. Host transport fixtures reserve three tracks/twenty
  loops, disable unused DSP where appropriate, assert their configured settings,
  and allow driver construction in test helpers. Product deadlines/defaults,
  arithmetic behavior and service/RT wire schemas are unchanged.
- 2026-11-13: complete native nextest gate passed: **3,223 passed, 11 skipped**
  in 672.437 seconds. Real native backend qualification also passed without
  `SHOOP_ALLOW_MISSING_BACKENDS`. The four legacy timing cases passed 40 repeated
  executions; the LAN startup case passed ten repetitions.
- All **120** affected pinned Node tests pass (arithmetic 3, codec 29, resources 6,
  scripting 36, session 46). Native scripting has 43 passing tests and production
  persistence has 32. Warning-denying builds, native/Wasm compilation, five
  alloc-only resource tests, dependency audits, formatting and test policy pass.
- Both packaged persistence carriers pass with >32 MiB audio, exact MIDI and
  source/Markdown/PNG persistence after deleting originals. The unmodified release
  browser smoke passes at 640×480 with fake microphone, and again with Web MIDI
  permission denial/retry. Both exercise stopped loads, exact resource round trips,
  explicit starts and repeated conversion, reporting zero xruns. Larger software
  rendering runs reached startup/hosted-restore deadlines; physical devices and
  all viewport combinations are not qualified here.
- See [acceptance evidence](docs/native_session_recovery/evidence/p16-client-scripts.md)
  for commands, recovery scenarios and limits. The implementation remains client
  owned: no backend script catalog, transfer or automatic execution was added.
- GitHub access was refreshed on 2026-11-13. All implementation milestones and
  local acceptance evidence have been pushed to PR #977. Final-head CI and
  automated review are being checked; parity closure remains pending.
- Restored CI log access confirmed the earlier Linux/Windows/macOS native and
  coverage failures were the LAN fixture's default-settings assertion, already
  fixed in `5b4bcfce`. Firmware jobs also exposed a pre-existing nested lock file
  missing `rt_audio_math` and its two dependency edges; the lock refresh adds only
  those entries and passes locked Cargo metadata resolution.
- The same audit found the missing math dependency in the two standalone native
  test projects. Their lock refreshes also change only the existing local math
  package/edge. Both locked project test commands and the independent native
  runtime/loop playback checks passed. The pinned Rust 1.99.1 ARM production
  and all-feature firmware release builds also passed after the firmware refresh.
- The next matrix passed Linux debug/release, Windows debug, coverage, hosted
  processors, security analysis, firmware and browser release. Windows release
  passed 2,139/2,140 application tests; its existing core workflow timed out on
  MIDI detail after two fixed 30 ms sleeps. The failure trace shows note-off
  queued 28.869 ms before playback. The test now waits for the existing engine
  MIDI echo after each message before ending recording, retaining its exact
  two-event assertion. Twenty local baseline and twenty corrected repetitions
  passed; warning-denying workspace build, formatting and test policy passed.
  Runtime code and product deadlines are unchanged; final CI rerun is pending.
- At the user's request, merged `origin/feat/backend-replacement` at `563a6b0d`
  into this branch as `e638cc27`. The merge retains the client-owned P16 contract,
  upstream's P11 duplication changes and its stricter tested Cargo layout check.
  Formatting, test attributes, parity, three source-marker tests, warning-denying
  workspace build and four-target dependency audits pass. The full merged native
  suite ran 3,230 tests: 3,227 passed and three startup/driver-switch tests
  failed on timing. Repeated serial qualification is in progress before claiming
  the merged gate green; no product deadline has been changed.
- Pre-merge CI at `ae2ddfbc` completed 2,672 Node tests (two ignored) across
  43 packages and all 682 Chrome library tests across four packages. Release
  browser workflows, including Firefox, and the entire firmware workflow passed.
  Debug application execution passed the P16 script/resource steps, then timed
  out on a hosted-processor MIDI import. Upstream `563a6b0d` now disables debug
  web execution while retaining debug builds/audits and release browser tests.
  The next CI run follows that imported policy; no viewport or deadline change
  has been added for P16.
- Upstream advanced again to `1c7609a8`, adding responsive host-shutdown
  management, serialized firmware diagnostics and an independent MIDI fixture
  synchronization fix. The overlapping fixture resolution keeps upstream's
  version, which checks both MIDI input consumption and active recording. The
  P16 reconnect changes are retained. Formatting, test policy, parity and the new
  firmware diagnostic test pass; merged build and test qualification continue.
- The three timing cases from merge `e638cc27` all passed ten serial repetitions
  each (**30/30**, 674.047 seconds), without changing their fixtures or deadlines.
  The latest merged native suite will use two concurrent tests locally; CI keeps
  its normal settings. This does not relabel the earlier four-test run as green.
- Latest-upstream warning-denying workspace build passes. Focused combined-branch
  qualification passes **13/13** tests: both native-owner cleanup cases, the core
  application workflow and ten P16 persistence/recovery cases (21.999 seconds).
- Published latest-upstream merge `ab1e004c`; PR #977 is mergeable and ready for
  review against `feat/backend-replacement` at `1c7609a8`. The current matrix's
  firmware USB test failed in its four-device emulator scenario: one device's
  reset capture lacks the 64-byte block for channels 8/9. Both its capture and
  QEMU character-device log show the same omission (artifact `14022215169` in
  run `43392408674`). The unchanged USB scenario passed upstream's run
  `43387851707`. Preserve this failure evidence and rerun the scenario in the
  next qualification matrix; do not claim the firmware gate green while unresolved.
- The full native run on `ab1e004c` with two concurrent tests completed in
  1301.833 seconds: **3,230 passed, one failed, 11 skipped**. All P16 cases and
  all three earlier timing failures passed, including 25 LAN/recovery cases.
  The remaining subprocess fixture timed out waiting for engine setup, then
  passed **10/10** isolated repetitions in 125.840 seconds without changes.
  Preserve the full-run failure in the
  evidence and use final CI to qualify the normal repository configuration.
- Real native backend qualification on the latest merge also passes without
  `SHOOP_ALLOW_MISSING_BACKENDS` (one test, 15.681 seconds).
- Linux debug CI ran the complete native suite: **3,230 passed, one failed,
  11 skipped**. Its failure was an existing dummy-driver pacing fixture whose
  stopwatch began after rendering had already started, while the assertion
  counted frames from zero. The fixture now measures frame progress within the
  observed interval, preserving its minimum render duration, sample-rate bound
  and two-buffer tolerance. Twenty baseline repetitions and twenty corrected
  repetitions pass; warning-denying workspace build, formatting and test policy
  pass. No driver implementation or deadline changes. The next push requalifies
  both this fixture and the unresolved firmware USB scenario.
- Upstream advanced to `60d8f21c` (memory reporting and acyclic loop dependency
  order). CI at `67be8b71` tested that newer synthetic merge and exposed a
  missing firmware dependency-lock edge. Local integration also found the new
  application API's RT-protocol dependency violating the existing client/codec
  boundary audit. Shared memory-report definitions now live in the existing
  `profiling_protocol` diagnostic crate; RT public module/type names are retained
  by re-export. The application API uses the neutral definitions, avoiding the
  dependency and lock drift without changing report fields or encoded bytes.
  Upstream's memory wire version and loop changes are retained. Native/ARM
  builds, boundary audits and combined P16/memory/topology qualification continue.
- Integration gates for `60d8f21c` now pass: warning-denying native workspace
  build, locked ARM production/all-feature builds and the complete 148-entry
  four-target dependency audit. **248/248** focused tests pass (15.260 seconds),
  including P16 persistence/recovery, loop topology, memory reporting, the core
  application workflow and corrected pacing. The moved report definitions are
  byte-for-byte identical to upstream, and report codec/schema files are unchanged.
  Portable Node qualification is running alongside final CI after publication.
- Latest integration is published as `4c23274b`, with a clean merge against
  upstream `60d8f21c`. Additional portable Node checks pass **48/48**: diagnostics
  6, persistence codec 29 and RT protocol 13. Two initial package builds exhausted
  generated-cache disk space before executing tests; both pass after clearing
  inactive native incremental caches and rerunning with incremental builds off.
  Final CI and parity closure remain pending; no automated review is available.
- Both actual native persistence probes pass again against the integrated memory
  protocol: socket and shared memory preserve >32 MiB audio, exact MIDI,
  script/resources, stopped loads and exact resaves. Socket loss also confirms
  host-owned accepted completion once from a fresh connection. In CI, USB now
  reaches a different intermittent failure: nested-hub output completion does
  not drain after exact MIDI input/output already passed (artifact `14025661909`,
  run `43400879971`). Preserve that evidence and retry the isolated job after the
  workflow finishes; the parity/delivery gate remains open until CI passes.
- Hosted-processor CI at `4c23274b` passed all Rust feature variants, Miri and
  35 Node DSP tests, but its hand-written AudioWorklet scenario still sent RT
  protocol 25 after upstream advanced to 26. The fixture now pins 26. A matching
  locked worklet build and the actual Chromium scenario pass all five exact
  audio regions with no render-memory growth. This changes no product protocol
  or timeout; the next matrix includes the fixture correction.
- Hosted-processor CI passes on `4c41fdba`. The release web artifact contract
  exposed a second hand-written protocol-25 pin (`engineProtocolVersion`) that
  the earlier case-sensitive search missed. It now uses 26. A broader scan found
  no remaining current-browser protocol-25 pins; the legacy reference's separate
  protocol remains 28. The complete client-codec/worklet clock/timestamp/xrun/
  runtime-toggle contract and trace-chunk transport check both pass locally.
