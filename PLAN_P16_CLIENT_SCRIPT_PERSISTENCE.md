# P16: client-owned script persistence

Status: implementation and local validation complete; final CI and review
in progress. PR [#977](https://github.com/SanderVocke/shoopdaloop/pull/977). The ownership contract
was revised with user approval on 2026-11-12. P16 remains OPEN.

Base: `feat/backend-replacement` at `9aee7496`; production behavior was compared
with `master` at `9134eb20`. Follow
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
