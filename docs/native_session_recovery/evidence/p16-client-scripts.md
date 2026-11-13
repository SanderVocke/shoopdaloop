# P16 acceptance evidence

Implementation and local validation are complete at `5b4bcfce` (2026-11-13).
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

| Gate | Command or route | Final result |
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
  four configured targets. No service or RT source/wire file changed for P16.

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
