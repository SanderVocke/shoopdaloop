# Native JACK restoration plan

Restore direct JACK audio and MIDI in the replacement backend. Selecting JACK must run the new engine through a native JACK client, with callback-local audio and MIDI, instead of translating the selection to CPAL.

Status: implementation complete; final validation and PR delivery in progress. Baseline `3aabf43789708a5640a8f5b368f737651ef1d29d`.

Target branch and final PR base: `feat/backend-replacement`.
Proposed implementation branch: `feat/backend-replacement-native-jack`.

## Goals and scope

- Restore startup selection of native JACK through the in-process application and independent session host, including the native host-process adapter where supported.
- Use a fixed, configurable bank of named JACK audio and MIDI ports for each host lifetime. Project those ports through the existing engine host-port and track-connection model, and expose them to JACK patchbay clients.
- Preserve MIDI event offsets in the JACK process callback, including output and recovery messages, without CPAL audio bridging or midir MIDI queues.
- Deliver configuration compatibility, truthful runtime status, bounded failure handling, platform feature isolation, regression tests, documentation, and an implementation PR targeting `feat/backend-replacement`.

Dynamic per-track JACK port registration, live audio-driver switching, JACK transport/timebase synchronization, and JACK session management are separate workstreams. Changes to the fixed port layout or sample rate require a host restart. This work does not close the broader P14 driver-switching parity item.

## Immutable acceptance criteria

1. Selecting JACK starts a direct JACK client for both audio and MIDI. The JACK path does not instantiate CPAL streams or midir connections. CPAL's own JACK host remains a separate CPAL option.
2. Audio input and output are processed within the same JACK cycle, without an added duplex-bridge buffer. MIDI offsets, ordering, and port identity survive input, engine processing, and output, including engine sub-block boundaries.
3. Configured named audio and MIDI ports are visible in JACK and the application's host-port projection. Track connections, configured startup routing, and default Master output connections work. Existing client-name-only JACK profiles still load with documented defaults.
4. The engine uses the server's actual sample rate and buffer size. Startup failures and unsupported server changes have explicit diagnostics. Xruns and server loss produce correct status; cleanup is bounded and releases partially created clients and ports.
5. Normal shutdown sends the engine's MIDI recovery before JACK deactivation and returns the render owner for cleanup off the real-time thread. Server loss cannot hang cleanup; unavailable recovery delivery is reported truthfully.
6. Native JACK works through the in-process application and independent session host. Supported native host-process conversion also preserves JACK selection. UI disconnect/reconnect retains running audio, MIDI, and session state without restarting the driver.
7. JACK can be enabled independently of CPAL. JACK-only, CPAL-only, combined, and driver-free native configurations build as supported; browser builds retain their existing dependency boundary. Missing JACK libraries or a missing server are handled at runtime in distributable native builds.
8. Deterministic tests and a required real-JACK production-path test prove actual audio and MIDI behavior. The real-JACK acceptance job fails if its required JACK facilities are unavailable. Required formatting, build, test, dependency-boundary, and CI checks pass.
9. The final deliverable is an open implementation PR with base `feat/backend-replacement`, passing required CI and automated review feedback addressed until approval. The PR describes behavior, validation evidence, and the remaining scope boundaries.

## Design rules and constraints

- Keep the new host independent of application semantics and the legacy engine. Use the legacy JACK implementation and tests as behavioral references, not as production dependencies or a callback implementation to copy unchanged.
- Preserve the current `#![forbid(unsafe_code)]` host policy by using safe JACK crate APIs. Keep JACK dependencies native-target-only and feature gated; enable JACK in intended native distribution feature sets with runtime library loading.
- Prepare and register ports, buffers, MIDI storage, and routing before activation. The callback exclusively owns the render side and registered process ports; it must not allocate, free, lock, wait, log through allocating paths, register ports, or manage external connections.
- Respect the engine's existing limits, including at most 64 host ports per kind and direction. Stable port identifiers and documented naming/default rules must survive session save/load; handle resolved client names and port-name collisions explicitly.
- Keep JACK audio and MIDI synchronous. Introduce a direct MIDI path in `Cycle` while retaining existing CPAL/midir behavior. Do not route JACK MIDI through the wall-clock input bridge or output pump.
- Split callbacks larger than the prepared maximum using preallocated storage. Rebase MIDI input offsets into each sub-block and output offsets into the original JACK cycle. Clear each JACK output MIDI buffer once per cycle; merge ordinary and recovery events in valid timestamp order without duplication.
- Bound MIDI capacity and count refused/dropped input and failed output writes. Do not broaden the engine's supported MIDI formats as part of driver restoration.
- Read the active server configuration before engine preparation. Support buffer-size changes within the documented splitting/capacity policy. A sample-rate change invalidates the prepared host: publish failure, silence safely, and require restart rather than attempting live resampling.
- Open with `NO_START_SERVER`, preserving the existing native JACK behavior. Do not start a server as a side effect of probing or selection. On genuine JACK startup failure, retain the application's explicit diagnostic and dummy fallback policy; never present fallback as an active JACK driver.
- Add a distinct protocol identity for JACK without renumbering existing wire tags. Check peer version/capability handling and reject unsupported peers clearly; change protocol versions only where compatibility requires it.
- Keep the process/lifecycle owner responsible for device changes and shutdown. UI loss must not stop the independent host. Preserve shutdown ordering, render-owner recovery, and off-thread destruction on success and failure.
- Keep each milestone buildable and tested. Include cross-crate exhaustive-match changes in the milestone that introduces the corresponding enum variant; do not expose incomplete production paths.

## Current implementation anchors

The code investigation used branch commit `3aabf43789708a5640a8f5b368f737651ef1d29d`. Reconcile these anchors against the latest target branch before implementation.

| Area | Files and observed gap |
| --- | --- |
| Native driver | `src/rust/shoop_rt_host_native/src/{lib,cycle,status}.rs`, `Cargo.toml`: only Dummy and CPAL drivers; `Cycle` owns MIDI bridges. Add `jack_driver.rs` and direct callback MIDI support. |
| Wire identity and host setup | `shoop_rt_protocol/src/{schema,codec}.rs`, `src/{host,codec/host}.rs`; `shoop_rt_host_process/src/{convert,serve,process}.rs`: no JACK driver kind; setup/discovery assumes existing drivers. |
| Configuration and launch | `shoop_host/src/{engine,serve}.rs`, `shoopdaloop/src/engine_backend.rs`, native Cargo feature forwarding: JACK currently maps to CPAL, and startup/testing matches require updating. |
| Saved settings and projection | `shoop_app_api/src/lib.rs`, `shoop_settings`, `shoop_egui/src/settings_dialog.rs`, `shoop_rt_backend/src/backend.rs`: JACK profile has only `client_name`; replacement settings omit a JACK choice and runtime projection has no JACK arm. |
| Reference and evidence | `shoop_engine/src/app_backend.rs`, `shoop_engine/tests/jack_app_backend.rs`, native CPAL/dummy tests, `docs/native_session_recovery/feature-parity.md`, `.github/workflows/build_and_test.yml`. |

## Staged implementation

### Stage 0 Prepare the implementation branch

- [x] Read the target branch's `AGENTS.md`, `.agents/index.md`, mandates, build/test guidance, and relevant style instructions; inspect worktree changes and preserve unrelated work.
- [x] Fetch `origin/feat/backend-replacement` and create `feat/backend-replacement-native-jack` from its current tip. Carry this plan onto that branch; record the actual baseline commit here.
- [x] Reconcile the anchors above, current launch paths, dependency-boundary checks, and CI jobs. Read the parity ledger before changing preserved legacy references.

Verification: the implementation branch starts from the intended target; the working diff contains only authorized work. Record the baseline and any implementation-detail adjustments without changing acceptance criteria.

### Stage 1 Define configuration and protocol contracts

Depends on Stage 0.

- [x] Specify and record the startup port bank: counts, names, direction mapping, defaults for old profiles, preferred Master outputs, and optional startup external connections. Keep this layout fixed for the host lifetime; document how JACK patchbay connections relate to engine track connections.
- [x] Add portable saved JACK settings with backward-compatible deserialization defaults and validation, plus native `JackConfig`. Expose new settings through the existing settings registry rather than a parallel configuration path.
- [x] Add `DriverKind::Jack`, append its codec tag, and update runtime projection and affected exhaustive matches. Preserve the configured and actual driver distinction, including fallback.
- [x] Define host setup serialization/conversion for JACK port configuration and resolved runtime metadata. Update version/capability handling if fields or peer expectations require it; keep existing driver payloads compatible where possible.
- [x] Add focused tests for old profiles, new profile round trips, limits and invalid names, wire round trips, stable existing tags, and unsupported peer behavior.

Verification: targeted protocol, configuration, settings, and projection tests pass. No existing JACK profile silently changes to CPAL, and no driver starts before its production implementation is ready. Commit the contract milestone.

### Stage 2 Add the direct callback MIDI interface

Depends on Stage 1.

- [x] Refactor `Cycle` minimally to accept bounded callback-local MIDI input and expose output and recovery events directly. Retain the queued CPAL/midir path and existing recovery ownership.
- [x] Add a deterministic process-block harness that exercises the same buffer/MIDI adapter the JACK callback will call, without requiring a running server.
- [x] Cover MIDI events at frame zero, the last frame, exact sub-block boundaries, and multiple events at one offset. Cover split audio buffers, zero-frame handling, output ordering, input/output overflow, and recovery on disconnect/fault.
- [x] Enforce the real-time heap contract around the exercised processing code, including checks that the guards themselves detect violations. Keep tests focused on samples, events, ownership, and counters rather than implementation structure.

Verification: new direct-I/O tests and existing native CPAL/dummy tests pass; direct MIDI does not acquire bridge timing or buffering. Commit the callback-interface milestone.

### Stage 3 Implement native JACK and its lifecycle

Depends on Stage 2.

- [x] Add the independent native `jack` feature and JACK dependency with runtime loading. Add `DriverConfig::Jack`, process/notification handlers, and all matching ownership/start/stop paths.
- [x] Open the client without server autostart, resolve its actual name/rate/buffer size, register the configured audio/MIDI port bank, and prepare the engine and default Master routes before activation. Apply external startup connections on the control side once ports are available; publish readiness only after required setup succeeds.
- [x] Copy or borrow JACK's planar audio through the prepared process adapter and transfer MIDI directly at cycle-relative offsets. Implement bounded splitting and output-writer ordering, without a CPAL duplex ring or midir connections.
- [x] Publish callbacks, processing load, xruns, MIDI drops/write failures, active driver state, and actual server configuration through existing status facilities.
- [x] Handle partial registration/activation/connection failure transactionally. Implement normal stop, client-name collision, server shutdown, buffer-size changes, and sample-rate invalidation; return or retire the render owner safely off the callback thread.
- [x] Add lifecycle/failure tests and a real-JACK peer-client test proving the driver's actual callback carries audio and timestamped MIDI.

Verification: deterministic callback/lifecycle tests pass, and the direct driver passes real-JACK audio/MIDI loopback. Normal stop delivers recovery before deactivation; unavailable server/library and server loss have bounded, truthful failure behavior. Commit the driver milestone.

### Stage 4 Wire application and host integration

Depends on Stage 3.

- [x] Replace the JACK-to-CPAL mapping in `shoop_host::engine` with native JACK configuration. Update inventory/probing, warnings, test-only driver matches, and both application and independent-host startup paths.
- [x] Carry JACK configuration through supported `shoop_rt_host_process` setup/conversion and diagnostics. Retain the architecture boundary that keeps that legacy process adapter out of the production application's dependency graph.
- [x] Add JACK to the replacement settings UI and remove its obsolete substitution message. Save the full startup profile and show the actual active driver; retain restart-based application of preferences.
- [x] Forward JACK features through the application and host crates and intended native release jobs. Ensure missing runtime JACK facilities do not break ordinary CPAL/dummy startup.
- [x] Exercise engine host-port projection, track routing, Master defaults, startup autoconnections, session port-name restoration/missing-port intent, and reconnect metadata for the new port bank.

Verification: application and independent-host tests launch JACK without CPAL/midir in the path; host-process setup preserves JACK selection. Old saved profiles work, fallbacks are visible, and UI reconnection does not restart the driver. Commit the integration milestone.

### Stage 5 Validate the production workflow and compatibility

Depends on Stage 4. This is the final end-to-end validation stage before PR delivery.

- [x] Add a required Linux CI job that starts a dedicated JACK server with the dummy audio backend and uses peer JACK clients. Include readiness timeouts, teardown, and captured failure diagnostics. Run required JACK tests without `SHOOP_ALLOW_MISSING_BACKENDS=1`.
- [x] Drive production application/service paths: launch with JACK; create audio and MIDI tracks; connect ports; record known samples/events; play them back; verify samples, channel identity, MIDI offsets, and Master output through peer clients. Test both in-process and independent-host launches.
- [x] Save/load a supported session and verify routing intent and media survive. Disconnect/reconnect the independent host's UI during playback and prove continuous audio/MIDI and retained state.
- [x] Validate missing server/library, startup rollback, port collisions, server loss, buffer changes, sample-rate invalidation, and normal shutdown recovery. Keep server-dependent assertions strict in the dedicated job and distinguish unavailable recovery delivery from a successful reset.
- [x] Build/test JACK-only, CPAL-only, combined, and driver-free configurations of affected native packages. Check default native distribution features on Linux, macOS, and Windows according to existing CI targets; run available JACK-provider smoke checks without claiming untested providers are validated.
- [x] Run browser application/worklet builds and the browser smoke and dependency-boundary checks. Confirm no JACK/CPAL/midir dependency reaches browser production paths and no legacy engine dependency reaches the new host.
- [x] Update the native-host README, configuration examples, and parity ledger with the restored direct driver, fixed port model, restart requirements, and evidence. Keep live switching and other outstanding P14 work open.

Verification gates, using the target branch's prescribed development environment and current equivalent commands:

```sh
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_tracing_coverage.py --require-closed
python3 scripts/check_rt_engine_boundary.py
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

The permissive workspace command supplements the strict real-JACK job; it does not replace it. Read `.agents/info/build.md` before running commands, use the repository development shell on Nix/NixOS, and use the provisioned environment in CI. Run focused tests while iterating; run required gates before behavior changes are committed/pushed as instructed by repository mandates. Record exact commands, supported configurations, and evidence for each acceptance criterion. Commit the validation/documentation milestone.


### Stage 6 Deliver the implementation PR

Depends on all preceding stages.

- [x] Review the final diff for unrelated changes, real-time contract violations, feature leakage, incorrect status, and acceptance gaps. Reconcile changes on the target branch and rerun affected checks if the baseline moves.
- [x] Push `feat/backend-replacement-native-jack` and open a PR with base **`feat/backend-replacement`**. Lead the description with restored native JACK behavior; include configuration compatibility, fixed-port scope, test evidence, and restart/failure semantics.
- [ ] Monitor every required CI job, fix failures in follow-up commits, and rerun affected checks until required CI is green.
- [ ] Check for automated PR reviews. Address feedback, push fixes, and repeat review/check cycles until the automated reviewer approves where present. Document any remaining material limitations accurately.
- [ ] Update this plan's checkboxes and evidence, verify the PR base/head and final passing commit, and return the PR URL with validation results.

Completion: an open implementation PR targeting `feat/backend-replacement`, with all immutable acceptance criteria evidenced, required CI passing, and automated review approval where provided.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution evidence

- Stage 0: implementation branch created from `3aabf43789708a5640a8f5b368f737651ef1d29d`; approved plan committed as `b0c4515b`.
- Initial Stage 1 milestone: saved port defaults are two audio inputs/outputs and one MIDI input/output, named `audio_in_1`, `audio_out_1`, `midi_in_1` and so on. Port lists are configurable; engine names use the stable `jack:` prefix, independent of JACK's resolved client-name suffix. Startup connection pairs use local short names or external `client:port` names; absent required endpoints fail startup with diagnostics. Port capacity remains 64 per kind/direction.
- Protocol identity uses appended tag 3; engine protocol version increases from 2 to 3 so older peers refuse clearly. Existing driver tags are unchanged. Host-setup conversion will be finalized with the native driver rather than exposing a partial JACK path.
- Stage 1 validation: warning-denying builds of protocol, API, adapter, and settings UI passed. Protocol/host configuration and recovery tests passed, including old JACK-profile deserialization and codec tag stability. Test-attribute policy passed.
- The JACK crate requires an unsafe shutdown callback declaration. A small native-only notification adapter isolates that declaration with an atomic-store-only body; `shoop_rt_host_native` retains `forbid(unsafe_code)` and safe JACK APIs. No unsafe pointer/FFI operations are added to the host.

- Stages 1–4 are implemented together as a coherent driver/integration milestone, with cross-crate enum matches and feature forwarding updated atomically. Host setup version 2 carries named JACK ports, startup connections and resolved instance names; application protocol version 12 rejects peers using the previous JACK profile layout.
- Local strict JACK acceptance passed for direct callback samples/timestamps, MIDI recovery, invalid ports, failed connection rollback, name collisions, buffer-size changes, server loss, missing library/server, application startup, process adapter, and in-process/independent service recording/replay/reconnect plus `.shoop` save/load.
- Warning-denying complete native workspace builds passed. All four native driver feature combinations passed `cargo check`. The engine/host dependency-boundary audit passed for Linux, Windows, macOS and WebAssembly. All 143 Python script tests passed; tracing, parity, formatting and test-attribute audits passed.
- Browser application and worklet compilation passed. The generated codec and worklet Node artifact contracts passed with engine protocol version 3. Browser smoke and final CI evidence will be recorded before delivery.
- An existing runtime stress test exhausted its iteration bound while native tests overlapped browser compilation. It passed all 10 isolated repetitions and the subsequent full run. Full-suite runs identified and corrected only the expected JACK settings field-count/missing-backend fixtures and application protocol byte fixtures; final clean-suite evidence is pending below.

- Core native driver/integration milestone committed as `93e5e38d`. Final native workspace run: **2,550 passed, four pre-existing skips**, with the suite's established optional-hardware policy. Strict real-JACK acceptance passed separately with the skip flag removed.
- JACK-only, CPAL-only, combined, and driver-free checks and native-host/host-library tests all passed. The strict CI job runs these same combinations.
- Hosted Chromium browser core smoke passed at 900×600 with **zero xruns**, including record/play, waveform/MIDI detail, `.shoop` save/load/resave and restored playback. Node v24.19.0 and Chromium 154.0.8037.57 were used locally; the existing CI web matrix retains its pinned browser tooling.
- Final formatting, warning-denying workspace build, test-attribute, tracing and parity audits passed. Workflow YAML parses. Native protocol byte fixtures and the browser canary are updated to their new versions.
- Remaining Stage 5 platform evidence and Stage 6 PR/CI/review completion will be recorded from the implementation PR.

- PR opened: https://github.com/SanderVocke/shoopdaloop/pull/925, targeting `feat/backend-replacement`. The target advanced to `64a4ad06` (click generation); it was merged without conflicts and the combined warning-denying build and full suite passed: **2,552 passed, four pre-existing skips**.
- Final review corrected preparation for server periods larger than the engine render block: JACK retains the actual server buffer metadata and uses at least that period as control lead while rendering bounded sub-blocks. The regression test failed before the fix and passes afterward; strict JACK acceptance and the warning-denying workspace build passed again on the merged source.
- Full retained workspace suite after that correction: **2,553 passed, four pre-existing skips**. PR CI initially passed strict JACK acceptance, all four driver feature combinations, coverage, bare-metal builds, CodeQL, and documentation; the platform/browser matrix is still being monitored.
- Codecov identified that the real-JACK acceptance job was outside the coverage collector (69.35% patch coverage). The coverage workflow now instruments the same strict production-path runner and merges its profiles with the retained suite, preserving the existing 80% patch gate.
- The instrumented strict runner passed locally. Its LCOV report includes 96.4% of `jack_driver.rs`, 96.1% of the direct callback adapter, and 100% of the notification adapter. Workflow YAML and shell syntax checks passed. The merged-source browser build and hosted core workflow also passed again with zero xruns.
- PR CI on `075f552c` passed strict JACK acceptance, the four driver configurations, all default Linux/macOS/Windows builds, and complete Windows-debug/Linux-release jobs. Real JACK provider acceptance is Linux JACK2 with the dummy backend; macOS/Windows JACK runtime providers are not claimed tested.
- All 2,553 instrumented workspace tests and strict JACK scenarios passed in the coverage job, but LLVM rejected one corrupt pooled child-process profile. The collector now uses separate per-process profiles without merge pools for tests that intentionally terminate hosts, retaining strict profile validation and unchanged coverage thresholds.
- The complete combined coverage flow passed locally with separate profiles: **2,553 tests passed, four pre-existing skips**, strict JACK acceptance passed, and strict LLVM profile merge succeeded. Changed executable lines in the combined LCOV report are **831/886 (93.79%)** against the current target.
- The corrected coverage job passed on `7b76f30c`; Codecov patch and project checks passed, with **93.00% patch coverage** against an unchanged 80% threshold. The target commit has no uploaded baseline coverage report, so the project check does not provide a numerical comparison.
- Review also identified that the engine's fixed startup command lead could be shorter than a subsequently enlarged JACK period. Scheduling now takes the maximum of configured lead and published JACK period. A portable engine regression verifies actual output timing across increasing and decreasing buffer sizes; it fails against the prior scheduling code and passes with the correction. Other driver scheduling policies are unchanged.
- The dynamic-period correction passed the warning-denying workspace build, test-attribute audit, full suite (**2,554 passed, four pre-existing skips**), and strict real-JACK acceptance.
