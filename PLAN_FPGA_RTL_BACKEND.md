# FPGA RTL simulation as a regular audio backend

## Goal and scope

Make the Marlin/Verilator RTL simulation of the FPGA pipeline a regular audio
backend. It is enabled by a cargo feature and selected through the ordinary
audio-backend setting. It starts through `shoop_host` and through the normal
`shoopdaloop` startup (in-process or with a companion/independent host), with no
dedicated binary and no RTL-specific window or UI elements.

Today the only application-level entry is `rt_fpga_sim`'s `fpga-app-demo`
(#983). It has its own window, a synthetic-input checkbox and a hand-built
service/client composition. This plan moves that composition onto the regular
host and app integration paths and retires the demo.

The supported processing profile stays the adapter's current one. That is one
mono Sync strip and loop, stereo Master, Q=32, and audio only (no MIDI, FX or
extra tracks). The default fresh session fits within it. Widening the profile is
separate work.

Builds on #987, merged as `5b7dd0a5`. It added the Simulated driver kind, FPGA
memory reporting, the overlay's local-runtime description and the regenerated
audio demo ROM.

Out of scope:
- Connecting the RTL backend to a real-time audio driver (CPAL/JACK). The
  simulation is 10–100× slower than real time, and the model is bound to its
  creating thread. The setting model below keeps this possible later.
- Widening the adapter profile (more tracks or loops, MIDI, FX, buses).
- Shipping RTL sources or a prebuilt model in release packages.
- Simulation on macOS or Windows. This needs Linux, as Marlin is validated there.
- Supporting newer Verilator releases (Marlin's bridge breaks on 5.050). Any
  physical-board host transport is out of scope too.

## Naming

The concept is the **audio backend**: what processes the engine's audio graph.
The existing `--audio-backend` CLI flag, `audio-backend` web query and setting
(`shoop_egui::AudioBackendKind`) gain the value `fpga_rtl_sim` next to
`new_engine`. Since the legacy engine was removed, `new_engine` has been the only
value. The audio backend is orthogonal to the **audio driver** (dummy, CPAL,
JACK, Web Audio, Simulated), which paces processing and moves device samples.

The plan does not use "engine" for this. In this codebase the engine is
`shoop_rt_engine`: its control endpoint, protocol, graph preparation and
observations are shared by both backends. Only the consumer that renders the
prepared graph differs. "Engine" also already names the engine protocol,
`EngineDescription`, `new_engine`, the engine transport setting and the legacy
`shoop_engine`.

The status overlay's "Backend type" line currently shows the driver kind. It is
renamed "Audio driver", and an "Audio backend" line is added.

## Current implementation and concrete gaps

- `shoop_rt_host_native::Prepared`/`Cycle` hard-wire `prepare_with_processors`
  and the native `Render`. `shoop_host`'s `serve.rs`, the app's
  `engine_backend.rs` and both driver/host-graph controllers assume a
  `NativeHost`. Only `rt_fpga_sim::app_worker` uses `prepare_with_consumer`.
- `rt_fpga_sim`'s `app` feature drags in `shoop_app_client`, `shoop_scripting` and
  `shoop_rt_engine/testing`. `scripts/check_dependency_boundaries.py` forbids
  these in `shoop_host` (audited with `--all-features`).
- `Adapter::prepare` requires Q=32, 2 audio in and 2 audio out, no MIDI, and one
  loop slot. The host's default description has larger pools, and the dummy
  driver has 1 MIDI in and 1 MIDI out.
- A cold Verilator model build takes about 20 s. That is longer than
  `shoop_host`'s 10 s session deadline (`serve.rs`) and the app's 12 s launch
  timeout (`launch.rs`).
- RTL source and model-cache paths are compile-time `CARGO_MANIFEST_DIR` paths.
  `verilator-wrapper` is a shell script.
- Marlin 0.21.1's generated bridge does not compile with Verilator 5.050
  (`VlWide<N>` passed to `memcpy`). CI builds 5.032 from source; the flake has no
  Verilator.
- `build_and_test.yml` does not run for PRs into `proto/fpga-rust-reference`.

## Immutable acceptance criteria

1. **`shoop_host` starts with the RTL backend.** With the `fpga-sim` feature on
   Linux and Verilator 5.032 installed, `shoop_host` started with the RTL audio
   backend accepts the normal app through its usual connection paths. In the
   default fresh session (Sync track and loop, Master), the app can connect
   input, record, play, stop and clear on RTL, with audio and loop state
   observed through the ordinary UI.
2. **The normal app runs it in-process.** The normal `shoopdaloop` binary built
   with the feature can run the RTL backend in-process, selected by the regular
   audio-backend setting or `--audio-backend`. No dedicated binary or
   RTL-specific window or controls are needed.
3. **The overlay identifies the RTL backend.** The status overlay reports the
   RTL audio backend and the Simulated clock driver through generic fields, and
   MEM shows RTL storage pressure.
4. **Other builds are unchanged.** Without the feature, dependency trees and
   builds on all platforms are unchanged: no Marlin, and boundary rules hold.
   Enabling the feature on an unsupported platform yields a clear "unsupported
   on this platform" error at startup, not a build break.
5. **Unsupported setups fail clearly.** This covers missing or unsupported
   Verilator, missing RTL sources, a real-time driver selected with the RTL
   backend, and actions beyond the profile. Each produces a specific error or
   refusal. Nothing crashes or faults the engine, and nothing silently falls
   back to the native backend.
6. **The RTL-versus-native comparison still passes.** The common Sync-loop
   comparison passes within the bounds documented in `rt_fpga_sim/APP_CONTRACT.md`,
   through the new composition.
7. **The demo is retired.** `fpga-app-demo`, the `app-ui` feature and the
   demo-only UI are removed, and the documentation describes the regular
   startup.
8. **CI covers it and passes.** CI builds and tests the host-level RTL
   integration on Linux. PRs into `proto/fpga-rust-reference` run the full
   build-and-test workflow, and the PR's CI is green.

## Design rules and constraints

- **Feature gating.** The feature is off by default. `fpga-sim` on `shoop_host`
  and `shoopdaloop` forwards to one new host crate. Marlin is a dependency only
  under `cfg(target_os = "linux")`; elsewhere the crate compiles to a stub that
  reports unsupported.
- **Backend and driver are separate settings.** `HostSettings` gains `backend`
  next to `driver`. Valid pairs are checked in one place. For now the RTL backend
  pairs only with the Simulated driver, and adding device drivers later must not
  require a setting change.
- **Thread affinity.** The Marlin runtime, model and adapter are created, used
  and dropped on a single worker thread. Nothing else touches them.
- **Single source of truth for the profile.** The adapter derives the profile
  limits used for the host description, pool clamps and host ports. They are not
  duplicated as constants elsewhere.
- **Regular paths only.** Reuse `RtBackend`, the service, the client and the
  normal UI. The only UI changes are generic: the audio-backend value and the
  overlay's "Audio backend"/"Audio driver" lines. Remove
  `set_local_runtime_description` if nothing else needs it.
- **Configurable runtime paths.** The RTL source directory, model cache directory
  and Verilator executable are configurable. Defaults point at the source tree
  and `work/` for development builds.
- **Verilator.** Keep the validated 5.032. One script builds it from source
  (the CI recipe) for both CI and local use. A runtime version check names the
  supported version when another one is found.
- **Backend-aware readiness.** Startup deadlines account for model build time.
  Progress is logged, and failures carry the Verilator/Marlin diagnostic.
- **Protocol versions.** Wire or schema changes bump the engine protocol and the
  application protocol versions, as in #984 and #987.
- **CPU cap.** Local test runs use at most half the CPU cores
  (`taskset -c 0-6`, 7 threads).

## Staged implementation

### 1. Branch, CI trigger and toolchain

- [x] Branch `feat/fpga-rtl-backend` from `proto/fpga-rust-reference` (`5b7dd0a5`,
      which includes #987) and open a draft PR with this plan.
- [x] Add `proto/fpga-rust-reference` to `build_and_test.yml`'s `pull_request`
      branches.
- [x] Move the Verilator 5.032 source build into `scripts/build_verilator.sh`,
      used by `fpga_simulation.yml` and documented for local use (including
      NixOS).
- [x] Add the runtime version check.
- [ ] Verify: the workflow runs on this PR; the script builds 5.032 locally; the
      check rejects another version with a clear message.

### 2. Host-usable RTL backend crate

- [x] Create `shoop_rt_host_fpga_sim`, which depends on `rt_fpga_adapter`,
      `rt_fpga_sim/sim` and `shoop_rt_engine` and nothing app-level. Move into it
      the reusable parts of `app_worker` and the profile description:
      - the worker thread that owns the runtime, model and adapter;
      - the unpaced Simulated clock;
      - the input mode (silence or deterministic synthetic), with output
        discarded;
      - stop and join.
- [x] Expose `start(config) -> Started`, mirroring `shoop_rt_host_native::start`
      (client, host status, stop handle), with readiness reporting and
      configurable RTL, cache and Verilator paths. Replace the `verilator-wrapper`
      shell script with Marlin options.
- [x] Add a non-Linux stub and a boundary-script rule for the new crate.
- [x] Re-base `rt_fpga_sim`'s app tests on the new crate. The `Fpga` composition
      may remain as a test-only harness.
- [ ] Verify: crate unit tests; the existing `app_rtl`/`app_*` suites unchanged in
      outcome; `cargo tree` shows no Marlin without the feature.

### 3. Settings, protocol and status

- [ ] Add `AudioBackendKind::FpgaRtlSim` (`fpga_rtl_sim`, accepted only when the
      binary has the feature).
- [ ] Add `HostSettings.backend` with serde default `new_engine`, and
      `--audio-backend` for `shoop_host`.
- [ ] Give the Simulated driver an input-mode config, and validate
      backend/driver pairs in one place.
- [ ] Report the active audio backend through engine/host status.
- [ ] Update the overlay ("Audio backend", "Audio driver"), and bump the protocol
      versions.
- [ ] Verify: config parse and validation tests (including refused pairs),
      settings, CLI and web-query tests, and overlay text tests.

### 4. `shoop_host` integration

- [ ] Make `serve.rs` start the selected backend. Construct the native
      driver-switching and host-graph controllers only for the native backend,
      and report switching as unsupported for RTL.
- [ ] Clamp pools and host ports to the adapter profile, so that out-of-profile
      app actions are refused with clear messages.
- [ ] Make the session and launch deadlines backend-aware, so a cold model build
      fits.
- [ ] Let the Host Manager's start dialog offer the RTL backend when the binary
      supports it.
- [ ] Verify: a feature-gated host integration test (Linux with Verilator) starts
      `shoop_host --audio-backend fpga_rtl_sim`, connects a client, and records
      and plays the Sync loop; a refusal test for an extra track and for a CPAL
      driver.

### 5. Normal app integration

- [ ] Forward `fpga-sim` from `shoopdaloop`. Route `start_engine` and
      `launch::pair` through the audio-backend setting, for both in-process and
      companion-host use.
- [ ] Make no RTL-specific UI changes.
- [ ] Verify: a headless app test (existing harness) starts with the RTL backend
      and runs the Sync trace actions; one manual GUI smoke run of the normal app
      in-process and against a separately started `shoop_host`.

### 6. Retire the demo

- [x] Remove the `fpga-app-demo` binary, the `app-ui` feature and the demo-only
      window code.
- [ ] Keep the deterministic replay and comparison as tests, through the new
      composition.
- [ ] Update `rt_fpga_sim/README.md`, `APP_CONTRACT.md`, `VALIDATION.md`,
      `docs/runtime_profiling.md` and the adapter README for the regular
      startup.
- [ ] Verify: the comparison test passes within the documented bounds, and docs
      build.

### 7. CI coverage

- [ ] Extend `fpga_simulation.yml` to build and test `shoop_host`/`shoopdaloop`
      with `fpga-sim` against the cached Verilator, and add the new crate to its
      path filters.
- [ ] Confirm that default builds on all platforms are unaffected.
- [ ] Verify: both workflows are green on the PR.

### 8. End-to-end validation and delivery

- [ ] Manual: from a clean shell, start `shoop_host --audio-backend fpga_rtl_sim`
      and connect the normal app. Separately, run the app in-process with
      `--audio-backend fpga_rtl_sim`. Exercise the default session, check the
      overlay and MEM, and confirm the refusals.
- [ ] Run the full workspace nextest suite once (7 cores) and the `rt_fpga_sim`
      suites, including the long take test.
- [ ] Mark the PR ready for review, and fix CI until green.
- [ ] Check the PR for automated review. Address the feedback repeatedly until
      the reviewer approves. Do not merge without the user's go-ahead.

## Testing cost

- Each stage runs targeted tests on 7 cores.
- Verilator-backed tests reuse the cached model and rebuild only when the RTL
  changes.
- Protocol, settings and UI behavior use device doubles and headless tests
  without Verilator.
- Full workspace nextest, the long take test and GUI smoke runs are reserved for
  the final stage.
- Rely on CI for non-Linux platforms.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution notes

- The environment exposes five CPUs; local builds/tests use affinity `0-1` and
  two jobs/threads, respecting the half-core limit.
- The shared script built the pinned Verilator 5.032 locally. The version-check
  regression accepts 5.032 and refuses 5.030/5.050 and malformed output.
- The reusable worker and app suites now live in `shoop_rt_host_fpga_sim`.
  The application replay harness is under its `tests/support`, with application
  dependencies only in dev-dependencies. Keeping it in `rt_fpga_sim` would create
  a production dependency cycle through the new host crate. The obsolete demo
  entry points were removed during this extraction; documentation retirement
  remains in stage 6.
- Marlin runtime options now select C++ build jobs and remap the statically
  declared bridge source to a cache-local embedded copy. FPGA RTL sources remain
  configurable through `SHOOP_FPGA_RTL`; no shell wrapper is needed.
- Initial extraction validation: host production boundary audit passes; the
  native app trace (3 tests), protocol refusal/ownership tests (5 tests), and a
  cold RTL application bootstrap pass. The cold model build took 14.5 seconds.
