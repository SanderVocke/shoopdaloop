# Replace Carla LV2 indirection with direct Carla Native hosting

## Goal and scope

Host Carla Rack, Patchbay, and Patchbay 16x through the C ABI in `CarlaNative.h`, obtained from a dynamically loaded `libcarla_native-plugin` runtime, instead of discovering and instantiating Carla's LV2 wrappers through Lilv. Preserve the existing frontend-independent processor contract, in-process and supervised-subprocess modes, realtime bridge, user-visible controls, session behavior, and worker diagnostics. Native application archives must carry a pinned, self-contained Carla runtime and no longer require a system Carla/LV2 bundle; browser builds remain free of native FX dependencies.

Direct Carla hosting removes LV2 only from the Shoop-to-Carla boundary. Plugins loaded by users inside Carla may still use any plugin format supported by the bundled Carla build.

### In scope

- Checked Rust FFI definitions for the required Carla Native ABI and safe ownership wrappers around dynamic library symbols, descriptors, host callbacks, plugin instances, state, UI, audio, and MIDI.
- Direct implementations of all three existing Carla chain types behind `CarlaProcessor`.
- Existing in-process bridging and one-worker-per-chain subprocess isolation, recovery, state checkpointing, lifecycle publication, and bounded logs.
- Compatibility with Carla processor state already stored by the LV2-backed implementation.
- Removal of Shoop's Carla-specific Lilv/LV2 discovery, atom, state, and external-UI hosting code and native build dependencies.
- Reproducible acquisition/build, licensing, staging, dependency-closure checks, and archive verification for Carla on Linux x86_64, Windows x86_64, and macOS arm64.
- Tests, benchmarks, CI, packaging, installation guidance, user documentation, and tracing inventory affected by the migration.

### Out of scope

- Replacing Carla's own support for LV2, VST, or other user-loaded plugin formats.
- Adding new Carla chain layouts, parameter automation, embedded Carla UI, transport synchronization, or plugin-management UI outside Carla's existing external UI.
- Changing session track topology, public processor IDs, the `carla.hosting_mode` setting, worker protocol semantics, or browser processor capabilities.
- Introducing installers or changing the existing unsigned archive artifact types.

## Immutable acceptance criteria

1. Carla Rack, Carla Patchbay, and Carla Patchbay 16x are instantiated from `carla_get_native_rack_plugin`, `carla_get_native_patchbay_plugin`, and `carla_get_native_patchbay16_plugin` in a dynamically loaded Carla native-plugin library. Shoop does not host Carla through LV2 or another plugin standard.
2. Native FX builds have no link-time dependency on Carla, Lilv, or LV2. They build without a Carla SDK/runtime present, locate the runtime by an explicit deterministic policy at execution time, and report actionable unavailability without crashing when loading, symbol validation, descriptor validation, or resource validation fails.
3. Existing processor behavior remains available: 2x2 Rack, 2x2 Patchbay, and 16x16 Patchbay audio; one bounded MIDI input and output; variable blocks up to the existing maximum; active/bypass behavior; dry/wet routing and recording; UI show, hide, close detection, and reopen; and state save/restore.
4. Existing Carla state from `.shoop` sessions and recorded wet takes remains loadable. Tests prove migration from a real legacy LV2 state fixture into the direct native host, equivalent Carla project restoration, and subsequent save/load stability in both hosting modes.
5. The `in_process` and `subprocess` setting values and restart-required behavior are unchanged. In-process mode retains the non-realtime processor bridge. Subprocess mode retains authenticated startup, bounded shared-memory audio/MIDI transport, deadlines, independent failure containment, lifecycle/generation/exit reporting, bounded per-generation stdout/stderr, checkpoint restore, desired active state, and explicit UI recovery semantics.
6. No Carla Native call, dynamic loading, allocation, UI servicing, state operation, library unload, or blocking synchronization is introduced on Shoop's audio callback. Existing no-allocation/no-mutex realtime tests and deadline behavior continue to pass.
7. Carla external UI works from both development and extracted packaged artifacts, including adding/removing supported plugins, patchbay editing, file dialogs supplied by Carla, helper-process shutdown, user-close notification, and repeated show/hide cycles without leaked processes or use-after-free.
8. Each Linux x86_64, Windows x86_64, and macOS arm64 native archive contains one reviewed and pinned Carla runtime component: the native-plugin library, Rack/Patchbay UI helpers, required discovery/bridge helpers, runtime resources/styles, transitive native libraries not provided by the platform contract, license notices, and corresponding-source information. An extracted archive works on a clean machine without separately installing Carla, Lilv, or an LV2 bundle.
9. Carla's runtime component is reproducible and integrity checked: the upstream revision, source URL/archive, build configuration, target architecture, expected payload, checksums, dependency allowlist, and licenses are recorded. Linux respects the supported glibc baseline; Windows DLL lookup is confined to packaged directories; macOS install names, architectures, nested signing order, and bundle layout are valid.
10. The retained native FX Cargo feature and public processor catalog remain stable, while internal `lv2` feature/module names, Lilv and `lv2_raw` dependencies, Carla LV2 URIs/port discovery/UI descriptor code, `LV2_PATH` requirements, and native CI package installation for Lilv are removed. Historical constants used only to decode legacy state may remain clearly isolated in the compatibility codec.
11. Browser/Wasm builds and artifacts do not contain Carla FFI, loader, binaries, helpers, or native dependencies and continue to reject Carla processors transactionally as unavailable.
12. Unit, ABI, real-runtime, in-process, subprocess, crash/recovery, UI smoke, state compatibility, artifact, dependency-isolation, formatting, warning-denying build, tracing-inventory, and complete workspace gates pass on their authoritative platforms.

## Design rules and constraints

- Treat the checked-in FFI as a pinned C ABI, not a Rust API. Generate or audit it against the exact shipped `CarlaNative.h`, cover `sizeof`/alignment/offsets with a C probe, use `extern "C"` callbacks that cannot unwind, and validate every required symbol, descriptor field, hint, port count, and function pointer before instantiation.
- Resolve absolute paths only. Use a documented developer/test override first and platform-specific paths relative to the current executable second; do not search the working directory or accept an unversioned library from `PATH`. Packaged execution must select its bundled runtime.
- Keep the dynamic library and all callback-owned strings/state alive longer than descriptors and instances. Hide UI, deactivate, clean up instances, and stop helper activity before releasing resources. Prefer one process-wide loaded runtime and keep it loaded through process shutdown if Carla cannot prove safe unload.
- Put unsafe ABI operations in a small native-host module. Keep `CarlaProcessor`, the processor bridge, shared-memory protocol, backend/public processor IDs, session topology, and frontend controls plugin-standard-neutral.
- Implement host callbacks with stable heap ownership and bounded/preallocated MIDI output storage. Native MIDI events must obey the engine's existing nonempty, at-most-four-byte payload and frame-offset limits; invalid or overflowing events are rejected/counted rather than truncated.
- Return the configured sample rate/buffer size and an explicit invalid/default time record through host callbacks, matching today's lack of Carla transport input. Service Carla UI idle work on the host/control owner at a bounded cadence, never from the audio callback.
- Preserve the opaque public state string contract. Write a direct-native canonical state representation and accept the historical LV2 JSON chunk representation by strictly decoding its Carla chunk/type/base64/NUL contract before calling `set_state`; reject malformed, oversized, interior-NUL, or wrong-chain data transactionally.
- Make runtime availability a capability probe shared by catalog publication and chain creation. A missing/corrupt runtime disables only Carla descriptors with a useful reason; External and Tiny Synth/FX remain available.
- Curate a Carla runtime component rather than copying an entire developer installation. Include every helper and dependency needed for the plugin formats currently exposed by Carla's UI, but exclude standalone Carla applications and Carla's LV2/VST wrappers used to host Carla itself.
- Keep third-party binaries out of browser artifacts and source builds that disable native FX. Do not restore a compile-time system dependency merely to simplify local development.
- Preserve the existing archive forms and cross-platform targets. Package verification must inspect exact manifests, binary architectures, loader paths/imports, executable bits, and licenses rather than only checking that files exist.

## Staged implementation

### Stage 1 — Freeze the ABI, runtime payload, and compatibility fixtures

- [x] Select and record one Carla release/commit for all targets; capture its `CarlaNative.h`, exported-symbol lists, build flags, GPL/source obligations, helper/resource layout, and per-platform transitive dependency closure in a machine-readable runtime lock/manifest.
- [x] Build a disposable C/Rust ABI probe for `NativeHostDescriptor`, `NativePluginDescriptor`, `NativeMidiEvent`, time structures, enums, callback signatures, and the three descriptor getters. Confirm descriptor labels/hints, 2/2/16 audio counts, one MIDI input/output, state/UI support, allocation ownership of `get_state`, and required idle/UI-close behavior against a real library.
- [ ] Produce representative legacy LV2 state fixtures from Rack, Patchbay, and Patchbay 16x with an actually loaded test plugin and routing, and document the exact `carla/chunk` + Atom String compatibility mapping to Carla Native `set_state`.
- [x] Define the curated runtime manifest for Linux, Windows, and macOS, including native library, UI helpers, discovery/bridge helpers, resources, dependency allowlists, architecture/baseline rules, and files deliberately excluded.
- [ ] Verify this stage with the ABI probe on all target toolchains, exported-symbol inspection, dependency inspection (`readelf`/`ldd`, `dumpbin` or equivalent, `otool`/`lipo`), fixture decode/encode tests, and a license review. Do not begin replacement of the LV2 host until all three target ABI/runtime contracts are supported.

### Stage 2 — Implement the dynamic Carla Native host

- [x] Add a native-only, checked FFI module and runtime locator/loader that applies the override/bundled precedence, resolves the three getter symbols, validates the selected descriptor, preserves library lifetime, and returns structured availability diagnostics suitable for the processor catalog and worker logs.
- [x] Implement a `CarlaProcessor` host around a stable `NativeHostDescriptor`: sample-rate/buffer/offline/time callbacks, bounded MIDI-output callback, UI-close/unavailable/idle dispatch, persistent C strings, preallocated audio planes/pointer arrays and MIDI input/output storage, and panic-proof callback boundaries.
- [x] Implement instance lifecycle and cleanup ordering: instantiate, activate/deactivate on observable active transitions, process bounded variable blocks, collect MIDI without allocation/truncation, save/free native state, restore validated state, dispatch size changes where required, show/hide external UI, service `ui_idle`, and cleanly terminate helpers before instance/library teardown.
- [x] Add the versioned/direct state codec and strict legacy LV2 chunk reader. Keep compatibility logic independent of Lilv and LV2 headers/crates and test malformed, oversized, NUL, wrong-type, and cross-chain cases.
- [ ] Verify this stage with fake-library loader/ABI/error tests plus real-runtime tests for all three descriptors, audio/MIDI pass-through, a loaded deterministic test plugin, active bypass, state round trips, UI user-close/reopen, repeated create/drop, and clean helper shutdown under sanitizers or platform memory tooling where available.

### Stage 3 — Integrate in-process and supervised-subprocess parity

- [x] Replace `CarlaLv2Host` construction in the application backend and worker with the direct native host while retaining `CarlaProcessor` and the existing realtime endpoint. Rename only LV2-specific internals/examples/tests; do not churn public processor IDs, topology, controls, or worker messages.
- [x] Add a bounded non-realtime idle hook to the processor ownership loops. Ensure the in-process bridge and subprocess control owner service native UI work without running it in the callback or allowing idle work to delay shared-memory block completion.
- [x] Preflight runtime capability before advertising/creating Carla chains, propagate loader/resource/ABI errors as unavailable reasons, and ensure worker startup reports the same diagnostics. Continue to create External and Tiny Synth/FX tracks when Carla is absent.
- [x] Preserve subprocess supervision behavior and prove checkpoints now contain direct-native state: startup handshake, processing deadlines, process-error/abort/hang/malformed-handshake/log-flood/shutdown-hang paths, crash detection, generation logs, clear logs, explicit recovery, checkpoint restore, desired active state, and UI relaunch only under the existing recovery policy.
- [x] Update in-process/subprocess benchmarks to compare the new direct host and retain shared-memory/reference transport measurements.
- [ ] Verify this stage with processor bridge realtime guards, session dry/wet audio and MIDI routing, wet recording/state capture, mode-setting tests, worker integration tests, lifecycle/recovery UI tests, no leaked worker/UI processes, and benchmark smoke runs.

### Stage 4 — Remove the Shoop-to-Carla LV2 layer

- [x] Delete Carla LV2 discovery, port metadata, URID/atom buffers, LV2 state callbacks, external-UI descriptor loading, and related tests. Rename the engine feature/module/configuration from internal LV2 terminology to Carla Native terminology while keeping `shoopdaloop`'s `native-fx` feature stable.
- [x] Remove `lilv` and `lv2_raw` from workspace/engine manifests and lockfile; retain `libloading`, state-codec dependencies, and isolated historical URI strings only where directly required. Tighten `cfg` boundaries so Wasm and no-native-FX graphs do not acquire them.
- [x] Update error messages, tracing inventory, examples, comments, test locks, and availability tests to refer to the native runtime rather than `LV2_PATH` or Carla LV2 bundles.
- [x] Verify with `cargo tree` audits for native default, native no-default-feature, and Wasm graphs; targeted `rg` audits for forbidden Lilv/LV2 host references; warning-denying builds; and the real-runtime suite without any Carla LV2 bundle installed or on `LV2_PATH`.

### Stage 5 — Build and distribute the Carla runtime component

- [x] Add reproducible automation that fetches the pinned Carla source with checksum verification, builds the reviewed native-plugin/runtime subset for Linux x86_64, Windows x86_64, and macOS arm64, and emits versioned component archives plus checksums, licenses/notices, build metadata, and corresponding-source information. Keep headers and binaries from the same revision.
- [ ] Normalize each component for relocation: establish the Linux glibc baseline and `$ORIGIN` loader paths/executable bits; place Windows DLLs safely and verify imports; set macOS `@rpath`/install names, arm64 slices, executable locations, entitlements if needed, and inside-out signing order. Exercise Carla discovery, bridges, and UI helpers after relocation.
- [x] Extend `package_artifacts.py` to stage the pinned component beside the executable in Linux/Windows layouts and under the correct `Contents/{MacOS,Frameworks,Resources}` locations on macOS. Pass the resulting resource directory to `NativeHostDescriptor` and make the hidden worker resolve the same bundle from its executable.
- [ ] Extend native archive verification to require the exact Carla manifest and notices, reject wrappers/standalone files and unexpected binaries, verify hashes/architectures/imports/rpaths, and fail if the application or worker resolves a system Carla. Preserve existing application metadata, fonts, icons, click assets, and archive naming.
- [x] Update the native CI matrix to obtain/build the pinned component, remove Lilv installation, package it in debug and release archives, cache only integrity-addressed inputs, and upload Carla build metadata with artifacts. Keep web cells isolated from all component work.
- [ ] Verify by extracting every native archive into a path containing spaces/non-ASCII characters on a clean runner, blocking system Carla/LV2 discovery, then probing all chain descriptors, processing audio/MIDI, opening/closing both Rack and Patchbay UIs, loading a representative supported plugin through Carla discovery/bridge paths, saving/restoring state, and running a subprocess crash/recovery cycle.

### Stage 6 — Documentation and final end-to-end validation

- [x] Rewrite installation, Carla process-isolation, architecture, settings/session-format, application README, and developer documentation: bundled runtime behavior, dynamic lookup/override diagnostics, direct Native API boundary, state migration, supported modes, external UI/helpers, subprocess tradeoffs, licenses/source offer, and no system Carla/Lilv/LV2 prerequisite.
- [x] Add deterministic unit/fixture tests to ordinary workspace CI and gate real Carla/UI/artifact tests on the pinned runtime rather than silently skipping when a developer installation is absent. Keep explicitly headful UI tests opt-in locally but mandatory on suitable authoritative CI runners.
- [ ] Run `cargo fmt --all -- --check`, `RUSTFLAGS="-D warnings" cargo build --workspace`, `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`, and `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run warning-denying default/no-default native builds, all real-runtime in-process/subprocess tests, legacy-state migration and session/wet-take round trips, realtime guards, crash/recovery/log tests, artifact package/verify/extracted-app smokes, and platform dependency/security audits.
- [ ] Build/check both Wasm packages, run browser dependency-isolation scans and hosted/self-contained artifact verification/smokes, and prove no Carla runtime file or loader dependency enters browser outputs.
- [ ] Require the authoritative GitHub matrix to pass Linux x86_64, Windows x86_64, macOS arm64, and WebAssembly debug/release cells. Record any unavailable hardware-only validation with evidence and block release rather than falling back to the LV2 host or an external Carla installation.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
