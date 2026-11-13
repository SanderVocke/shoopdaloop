# P16 acceptance evidence

Implementation is published through `4c23274b` (2026-11-13), integrating upstream
`60d8f21c`. Latest local integration gates pass as recorded below.
PR [#977](https://github.com/SanderVocke/shoopdaloop/pull/977) targets
`feat/backend-replacement`. All implementation commits are published; final CI and review
are in progress. P16 remains OPEN.

Implementation milestones: `d6bd336e`, `7e16fa40`, `0c7a0b4b`, `f8925062`,
`8039b33f`, `5b4bcfce`. The final two correct an existing native loop-readiness
race and host test fixture expectations/time allowances discovered during the
complete qualification. Product deadlines and defaults are unchanged.

## Ownership and observable behavior

The service/RT schemas are unchanged. `SessionScripts` and the codec worker's script parts are client-only values. `Work::Prepare` separates resources before producing the musical image. Only that image is staged at the host. Resource provider namespaces include a client-manager identity, so independent clients cannot unregister each other's resources.

| Criterion | Implementation and evidence |
| --- | --- |
| A1 | `shoop_persistence_codec::scripts::tests` independently compares source, Markdown and PNG bytes, checks script-free musical images and streams resources separately from them. Dependency audits pass for native, Windows, macOS and Wasm production feature sets. |
| A2 | Production `persistence_io::tests::scripts` ports the exact export and filesystem conversion scenarios. A source directory is deleted before conversion back, explicit start, provider reads and resave. Admission failures preserve ownership. |
| A3 | Production save tests mutate/remove client records after requesting a save and inspect the frozen output. Project-epoch checks refuse stale captures. Another client's replacement quarantines retained scripts instead of pairing them with new music. |
| A4 | Archive and compile-only Lua validation run in codec workers. Invalid syntax and declined rate conversion preserve music and records. Prepared records are built incrementally before staging, with reserved runtime identities. |
| A5 | Production load tests preserve User/Ephemeral record identities, retain enabled preferences without starting scripts, and clear only Session records on successful empty load/New Session. Explicit starts alone cause observable script control changes. |
| A6 | A future script API source survives accepted musical resampling, exact export and resave. It stays inactive when explicit execution is refused. Catalog reconciliation cannot restart or reclaim converted Session/Ephemeral records. |
| A7 | Public runtime/provider tests cover accepted-load reconnect, ambiguous delivery, evicted results and replacement-client save. Candidates remain inactive on uncertainty; accepted operations are never replayed. The packaged socket probe drops file-transfer clients and observes host-owned accepted load completion exactly once from a fresh connection. |
| A8 | Resource/metadata/count/chunk limits, alloc-only resource tests, native/Wasm builds and cross-target dependency audits pass. Native driver tests preserve the running Lua VM and application IDs. Complete native and real-browser workflows pass as recorded below. |

## Commands and results

Environment: Linux x86_64, Rust 1.100.0, nextest 0.9.116, pinned Node 22.22.2,
Chromium 154. Activate the installed toolchain/native libraries with
`. /workspace/.setup/activate.sh`. Rust builds use `RUSTFLAGS='-D warnings'`.

| Gate | Command or route | Pre-merge result (`5b4bcfce`) |
| --- | --- | --- |
| Complete native suite | `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci` | **3,223 passed, 11 skipped**, 672.437 seconds |
| Real backend facilities | `env -u SHOOP_ALLOW_MISSING_BACKENDS cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci -E 'test(native_builtin_fx_track_has_matching_audio_midi_and_transactional_state)'` | Passed without the missing-backend override |
| Portable Node suites | `python3 scripts/run_wasm_tests.py --runtime node --profile dev --package rt_audio_math --package shoop_persistence_codec --package shoop_script_resources --package shoop_scripting --package shoop_session` with pinned Node on PATH | **120 passed**: arithmetic 3, codec 29, resources 6, scripting 36, session 46 |
| Native changed areas | Native scripting and production `persistence_io` test binaries | **43 scripting / 32 persistence passed**; also included in the complete suite |
| Alloc-only resources | `cargo test -p shoop_script_resources --no-default-features --lib` | Five passed |
| Host integration | Final LAN/recovery suites | 25 passed, two skipped; also included in the complete suite |
| Timing regressions | Four legacy native cases repeated ten times each; LAN startup repeated ten times | 40/40 legacy and 10/10 LAN passed |
| Warning-denying builds | `RUSTFLAGS='-D warnings' cargo build --workspace`; native application/host and affected package builds | Passed |
| Portable application build | `cargo check -p shoopdaloop -p shoop_persistence_codec --target wasm32-unknown-unknown --no-default-features` | Passed; worklet and browser tests also compile |
| Release web application | `NO_COLOR=true trunk build --release` from `src/rust/shoopdaloop` | Passed, including optimized worklet and client codec |
| Dependency boundaries | `python3 scripts/check_dependency_boundaries.py --package shoop_persistence_codec --package shoop_app_client --package shoopdaloop` | Linux, Windows, macOS and Wasm production dependency audits passed; these audits are not cross-platform execution tests |
| Repository policies | Formatting, `scripts/check_shoop_test_usage.py`, `scripts/check_feature_parity.py`, `git diff --check` | Passed |

Packaged persistence acceptance passed over both carriers:

```sh
target/debug/shoopdaloop --probe-persistence --session-transport socket
target/debug/shoopdaloop --probe-persistence --session-transport shared_memory
```

These probes cover >32 MiB audio, exact MIDI, native file I/O, corrupt-file
refusal, source/Markdown/PNG preservation after deleting originals, stopped load,
explicit resource access, exact resave and host-owned completion after client loss.

The unmodified full Chromium smoke passed twice from `src/rust/shoopdaloop`:

```sh
NEW_ENGINE=1 BROWSER_SIZE=640,480 CHROME_BIN=chromium \
  node --experimental-websocket browser_smoke.mjs
WEB_MIDI=1 WEB_MIDI_DENY_FIRST=1 NEW_ENGINE=1 BROWSER_SIZE=640,480 \
  CHROME_BIN=chromium node --experimental-websocket browser_smoke.mjs
```

Both runs exercised the added stopped-load, exact script/resource round-trip,
explicit-start/dialog image and repeated-conversion steps, input, all three
hosted processors and complete musical persistence. Both reported zero xruns,
render memory growth and overflows. Software-rendered runs at larger viewports
reached startup or hosted-restore deadlines; this environment does not qualify
all viewport/device combinations. Product deadlines were not changed.

Local raw logs are retained under `/tmp/p16-*`: notably
`p16-native-final-qualified.log`, `p16-backend-required.log`,
`p16-node-final.log`, `p16-socket-probe-final.log`,
`p16-shared-probe-final.log`, `p16-browser-compact-smoke.log` and
`p16-browser-midi-smoke.log`. Node machine-readable results are under
`target/wasm-tests/dev/reports/node/`. These generated artifacts are workspace
records, not committed evidence or substitutes for final CI.

## Qualification findings and delivery limits

Earlier native failures exposed a real readiness race in the retained native
adapter: waiting for a callback count did not prove that a newly created loop
was ready for routing reads. It now waits for that loop's creation command.
Transport fixtures also allocated unnecessary default host capacity and one
asserted default settings after its configuration was reduced. Their capacity,
assertions and test-helper time allowances now match their actual scenarios.
The complete final suite and repeated cases above passed after these changes.

Initial documentation-only CI (`6ed9ee10`) failed native builds because Rust
1.100 changed Cargo's OUT_DIR layout. The implementation fixes that failure.
Its web debug Node run independently failed `rt_audio_math` before executing
tests due to a missing Wasm test declaration; the local 120-test harness now
passes after declaring it. A concurrent normal native build can overwrite the
test-enabled host companion, so the final full native gate ran without another
native build modifying its executables.

After GitHub access was refreshed, earlier Linux/Windows/macOS native and
coverage logs confirmed the failing LAN fixture assertion already corrected by
`5b4bcfce`. Firmware logs showed its separate Cargo.lock lacked `rt_audio_math`
and the two existing dependency edges from `rt_runtime`/`shoop_rt_engine`.
Refreshing only those entries fixes locked metadata resolution without updating
any registry package version or adding scripting dependencies to firmware.
Final supported-platform CI and automated review are in progress. No PR has
been merged.

Additional qualification after the lock audit:

- Both `cargo test --locked --manifest-path tests/loops_host/Cargo.toml` and
  `cargo test --locked --manifest-path tests/rt_runtime_host/Cargo.toml` passed
  after adding only their existing `rt_audio_math` dependency entries.
- `python3 scripts/run_rt_runtime_hosts.py --native-only --profile debug` and
  `python3 scripts/run_loops_hosts.py --native-only --profile debug` passed,
  producing the independent native replay evidence used by CI.
- With Rust 1.99.1 and the existing Cortex-A9/NEON compiler flags,
  `cargo +1.99.1 build --locked --release --target armv7a-none-eabihf
  --manifest-path platforms/zynq7000/firmware/Cargo.toml` passed both with and
  without `--all-features`. Firmware dependency-boundary audits passed on all
  four configured targets. No service or RT wire schema carries P16 script data.

## Windows core-workflow fixture qualification

The matrix at `ae2ddfbc` passed the P16 tests. Windows release passed
2,139/2,140 application tests; its existing core-workflow scenario timed out
waiting for exactly two recorded MIDI events. Twenty local baseline repetitions passed.
The previous test sent note-on/note-off with fixed 30 ms sleeps before switching
to playback, without observing engine consumption of either input.

The failure capture is artifact `14014542815`,
`perfetto-nextest-windows-x86_64-release-43373017234`, from
[the qualification run](https://github.com/SanderVocke/shoopdaloop/actions/runs/43373017234).
Its 23.564-second trace has no reported drop/loss/error statistics. Inventory
shows client/service ticks and backend request logs, without audio-callback
execution slices. Using the pinned Perfetto processor, query
`slice JOIN args ON args.arg_set_id=slice.arg_set_id`, filter
`args.key='debug.request'` and the 2,400–5,000 ms range relative to
`trace_bounds.start_ts`, then sort by `slice.ts`:

| Backend request | Relative timestamp |
| --- | --- |
| Note-on, strip 2, note 60 | 2,842.056 ms |
| Note-off, strip 2, note 60 | 2,882.800 ms |
| Playback, loop 9 | 2,911.669 ms |

These are queue timestamps, not proof of audio-engine ingestion before the
recording ended. The test now observes the existing
`latest_input_midi_message` after each input before ending recording. Its exact
two-event assertion remains unchanged. All twenty corrected repetitions pass,
as do the warning-denying workspace build, formatting and test-attribute checks.
No runtime code, wire schema or product deadline changed. Final Windows CI
qualification remains pending on the follow-up head.

## Upstream integration and CI policy

The user requested upstream integration on 2026-11-13. Merge `e638cc27` includes
`feat/backend-replacement` at `563a6b0d`, preserving the client-owned P16 contract
and upstream's P11 copying semantics and tested Cargo layout handling. The merged
workspace build, formatting, test policy, parity, three source-marker tests and
four-target production dependency audits pass. The merged native suite ran
3,230 tests: 3,227 passed, three startup/driver-switch cases failed, and 11 were
skipped. Serial repetition is in progress to distinguish timing sensitivity from
deterministic failures. This run is not claimed green; final-head CI is pending.

Pre-merge CI at `ae2ddfbc` also completed these broader checks:

- **2,672 Node tests passed, two ignored**, across 43 packages.
- **682 Chrome library tests passed**, across four packages.
- All release browser workflows passed, including Firefox.
- The complete firmware workflow passed, including USB, both board profiles,
  networking and recovery; coverage and all native platform jobs except the
  Windows core-workflow case described above passed.

The machine-readable Wasm results are artifact `14016897293`,
`wasm-test-reports-debug-43373017234`. The debug application workflow passed all
P16 script/resource steps, then timed out at `HostedMidi` during a later hosted
processor import awaiting host commit. It reported zero xruns, discontinuities,
overflows and memory growth. Its log is artifact `14017441071`,
`browser-workflow-debug-43373017234-1`. This debug workflow is not claimed green.

Upstream commit `563a6b0d` disables debug web test execution through
`SHOOP_DEBUG_WEB_TESTS=false`, retaining debug builds/audits and release browser
execution. The imported CI policy governs subsequent qualification. P16 adds no
viewport or product-deadline exception. The previously collected Node/Chrome
results identify their exact pre-merge revision rather than claiming execution
on the merged head.

Upstream subsequently advanced to `1c7609a8`. Its responsive shutdown management,
serialized firmware diagnostics and independent MIDI fixture fix are also
integrated. The overlapping MIDI test retains upstream's implementation, including
its active-recording assertion; P16's reconnect behavior is preserved. The new
firmware diagnostic test, formatting and repository policy checks pass. Merged
native and final-head CI qualification remain in progress.

The three timing cases from `e638cc27` passed **30/30** executions with
`--test-threads 1 --stress-count 10`, in 674.047 seconds. The unchanged tests are
`invalid_startup_is_rolled_back_and_a_ready_host_survives_ui_spawn_failure`,
`embedded_driver_switch_keeps_the_live_lua_vm_and_application_ids` and
`driver_preference_retry_serializes_all_native_settings_writers`. The raw log is
`/tmp/p16-upstream-timing-baseline-ready.log`. The latest merged full suite uses
two concurrent tests on this four-CPU machine, without additional load; the
normal CI configuration remains unchanged. The earlier four-test run is still
recorded as a failure, not replaced by the serial results.

The latest-upstream warning-denying workspace build passes. Focused nextest
qualification passes **13/13** in 21.999 seconds: two native-owner cleanup tests,
the complete core application workflow and ten P16 persistence/recovery cases.
Logs: `/tmp/p16-upstream-latest-build.log` and
`/tmp/p16-upstream-latest-focused.log`.

Latest merge `ab1e004c` is published and PR #977 is mergeable. Its
[firmware workflow](https://github.com/SanderVocke/shoopdaloop/actions/runs/43392408674)
has a failure in the existing four-device USB emulator scenario. Evidence
artifact `14022215169` contains matching 448-byte captures for the third
background device and its QEMU character log, instead of 512 bytes: the reset
packets for channels 8/9 are absent on all four cables. The earlier and final
packets are present, so this is not merely an unfinished final socket read.
The same unchanged scenario passed in upstream's
[firmware run](https://github.com/SanderVocke/shoopdaloop/actions/runs/43387851707).
The current firmware gate remains failed pending the next qualification matrix; no P16
firmware source change or test exception is introduced.

The complete latest native run (`ab1e004c`, `--test-threads 2`) finished in
1301.833 seconds: **3,230 passed, one failed, 11 skipped**. All ten P16 application
cases, all 25 LAN/recovery cases and the three earlier startup/driver failures
passed. `both_carriers_drive_mock_cpal_and_midi_callbacks_without_heap_operations`
failed at its existing engine-setup response timeout. It subsequently passed
**10/10** isolated repetitions in 125.840 seconds without changes (log:
`/tmp/p16-upstream-process-baseline.log`). The full log is
`/tmp/p16-upstream-latest-full-native.log`; this full
local run is not claimed green. Final CI qualifies the unchanged normal settings.

Latest-merge native backend qualification without `SHOOP_ALLOW_MISSING_BACKENDS`
also passes (one test, 15.681 seconds; `/tmp/p16-upstream-real-backend.log`).

## Dummy-driver pacing fixture

Linux debug [run `43392409121`](https://github.com/SanderVocke/shoopdaloop/actions/runs/43392409121)
passed 3,230 native tests and failed one existing pacing assertion (11 skipped).
The driver starts rendering before `start()` returns, but the fixture starts its
stopwatch afterward and compares all frames since zero with that later interval.
The correction compares frame progress after the measurement begins and waits
for at least 2,400 additional frames. The sample-rate bound and two-buffer
tolerance remain unchanged, as does the absolute callback-count assertion.

Failure artifact `14023390142`,
`perfetto-nextest-linux-x86_64-debug-43392409121`, has a 946.633 ms trace with a
768.926 ms native-start span and a stopped-host report of 109 callbacks and
18 xruns. The pinned processor reports no drop/loss/error statistics. There are
no sampled frame counters or fixture-clock values; the trace cannot measure the
assertion's exact overrun. The clock-order defect is visible directly in the
fixture. Local baseline passes **20/20** (21.587 seconds); corrected passes
**20/20** (24.694 seconds). Warning-denying workspace build, formatting and test
policy pass. Logs are `/tmp/p16-dummy-pacing-{baseline,corrected}.log` and
`/tmp/p16-dummy-fixture-build.log`. No driver implementation or deadline changed.
The follow-up CI matrix also reruns the unresolved USB scenario.

## Integration with upstream memory reporting

Upstream `60d8f21c` includes memory-use reporting and an acyclic loop dependency
order. The `67be8b71` CI checkout was its synthetic merge `abb5b19b`, exposing a
firmware `--locked` resolution failure from the new application API dependency
on `shoop_rt_protocol`. Local audits also rejected that dependency through
client, service and portable-codec paths.

The integration places the unchanged `MemoryReport`, `MemoryResource` and
`MemoryUnit` definitions in the existing `profiling_protocol` diagnostics crate.
RT module/type paths re-export them, and the application API uses the same
neutral values. Their bounded names already use that diagnostic crate's `Name`.
No fields, enum order, report codec or wire versions are changed by this fix;
upstream's memory telemetry and topology semantics remain intact. Removing the
application-to-RT dependency also preserves the existing firmware lock file.
Warning-denying native workspace and locked ARM production/all-feature builds
pass. The complete four-target dependency audit passes all 148 configurations.
**248/248** combined tests pass in 15.260 seconds: P16 persistence/recovery, loop
topology, memory reports, core application workflow and corrected pacing. A direct
source comparison confirms byte-for-byte identical report definitions, and codec
and schema files have no difference from upstream. Portable Node qualification
is running alongside final CI after publication.

Logs: `/tmp/p16-memory-neutral-{native-build,firmware-build,focused,node}.log`
and `/tmp/p16-memory-boundary-audit.log`.

Integration is published as `4c23274b`. Additional pinned Node checks pass
**48/48**: `profiling_protocol` 6, `shoop_persistence_codec` 29 and
`shoop_rt_protocol` 13. The first two remaining package builds exhausted disk
space before executing tests. After clearing inactive native incremental caches,
both pass with `CARGO_INCREMENTAL=0` and debug symbols disabled. The initial
summary is `/tmp/p16-memory-node-first-summary.json`; successful retry results
are in `/tmp/p16-memory-neutral-node-retry.log` and
`target/wasm-tests/dev/reports/node/summary.json`. No test failure is attributed
to the disk-space errors. Final CI and review remain pending.

Both actual native persistence probes pass again on this integration, including
>32 MiB audio, exact MIDI and script/resource round trips. Socket connection-loss
cases confirm accepted host-owned load completion once from a fresh connection.
Logs: `/tmp/p16-memory-socket-probe.log` and
`/tmp/p16-memory-shared-probe.log`.

The [firmware run for `4c23274b`](https://github.com/SanderVocke/shoopdaloop/actions/runs/43400879971)
has a different USB failure, in the nested-hub scenario. Exact channel/system/
realtime/SysEx input and output pass, then the output-completion drain times out:
all sixteen client queues are empty but one HCD output owner is still pending.
Artifact `14025661909` retains serial/client logs and configuration; local copy:
`/tmp/p16-usb-4c-evidence.zip`. This run is not claimed green. An isolated job
retry is pending completion of the remaining workflow jobs.

Hosted-processor [run `43400880038`](https://github.com/SanderVocke/shoopdaloop/actions/runs/43400880038)
passed all four Rust feature variants, Miri and 35 Node DSP tests. Its actual
AudioWorklet scenario refused to welcome a hand-written protocol-25 client after
upstream advanced the RT protocol to 26. Updating the fixture's explicit version
pin to 26 passes the matching locked worklet build and the complete Chromium
scenario: direct, processed, bypassed, reactivated and removed-bus output all
match their expected signals; render-memory growth is false. Logs:
`/tmp/p16-busfx-version-worklet-build.log` and
`/tmp/p16-busfx-version-browser.log`. No product protocol or timeout changes.

Hosted-processor [run `43405024376`](https://github.com/SanderVocke/shoopdaloop/actions/runs/43405024376)
passes on `4c41fdba`. The release web job found a second stale version pin in
`rt_worklet_contract.mjs`, missed by the earlier case-sensitive search. It now
uses 26. Scanning the current browser fixtures finds no remaining protocol-25
pins; protocol 28 belongs to the unchanged legacy reference. The complete
client-codec/worklet clock/timestamp/xrun/runtime-toggle contract and trace-chunk
transport test pass locally. Logs: `/tmp/p16-rt-contract-v26.log` and
`/tmp/p16-trace-chunk-contract.log`.

Linux full-workspace, Windows debug and coverage CI at `76de30aa` identify the
same three upstream fixture failures. Two independent application-protocol byte
vectors still pin 34 after upstream's bump to 35. The memory-label GUI test
discards a font-texture update without clearing it, which egui rejects in debug
builds. The vectors now pin 35; the headless fixture clears texture updates while
preserving its unavailable-versus-zero text assertions. Product behavior and
wire versions are unchanged by these corrections.

After these fixes, the normal CI-profile local native command passes
**3,241/3,241 tests, 11 skipped, 799.661 seconds**, without a test-thread override.
Logs: `/tmp/p16-upstream-fixtures-native-build.log` and
`/tmp/p16-final-fixtures-full-native.log`. The full firmware USB job
[`131826752100`](https://github.com/SanderVocke/shoopdaloop/actions/runs/43407897378/job/131826752100)
passes on `76de30aa` without firmware changes, including both previously failing
scenarios. Earlier failed runs remain failed in the historical record. Final
cross-platform CI and parity closure remain pending.

## Deliberate limits

No VM migration, automatic start, server script catalog, attachment transfer of
script data, durable client crash journal, general Lua module discovery or
directory-bundle export is included. A process that disappears loses its unsaved
client additions. Reconnect retains local context only for the same verified
host instance. Browser device evidence uses fake microphone/MIDI endpoints; it
cannot qualify an arbitrary physical controller or grant another user's permissions.

## Delivery progress

GitHub access was refreshed on 2026-11-13 and all implementation commits through
`5b4bcfce` plus local acceptance evidence were pushed to PR #977. The initial
review/review-thread inspection found no submissions or inline feedback.
Final CI and any subsequent automated review are being checked before parity
closure. No PR has been merged.
