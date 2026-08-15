# WebAssembly test migration baseline

This report freezes the starting point for `wasm_test_architecture_plan.md`. Commands use the repository's CI feature and backend policy unless noted. Generated JSON and logs live below `target/wasm-tests` and are not committed; commands and checked-in classification metadata reproduce them.

## Reproduction environment

- Base production revision: `adb0eeed` (squashed PR #749).
- Rust channel: repository `stable`, target `wasm32-unknown-unknown`.
- Native inventory: cargo-nextest 0.9.116, `--workspace --features shoop_engine/app_backend --profile ci`.
- Wasm pilot: wasm-pack 0.15.0, wasm-bindgen 0.2.127, wasm-bindgen-test 0.3.77, Node 22.22.2, Chromium/ChromeDriver 147.0.7727.137.
- Protocol bootstrap: `PROTOCOL_VERSION = 12`, `COMMAND_MAX_BYTES = 65,536`, read by the fixture from `shoop_audio_protocol` rather than copied into fixture JavaScript.

The local native inventory used the same prebuilt Tracy 0.6.0 Linux bundle and development libraries as CI. `SHOOP_ALLOW_MISSING_BACKENDS=1` skips unavailable runtime facilities; it does not remove native test binaries from discovery.

## Exact inventory commands

```sh
# Native canonical IDs.
SHOOP_ALLOW_MISSING_BACKENDS=1 \
TRACY_CLIENT_SYS_PREBUILT_DIR=/path/to/tracy-embedded-native \
  cargo nextest list --locked --workspace \
  --features shoop_engine/app_backend --profile ci \
  --message-format json > target/wasm-tests/native-nextest.json

# Actual Wasm pilot binaries and reports.
python3 scripts/run_wasm_tests.py --runtime node --profile dev
python3 scripts/run_wasm_tests.py --runtime chrome --profile dev

# Cross-runtime and source-level accounting.
python3 scripts/wasm_test_inventory.py \
  --native-json target/wasm-tests/native-nextest.json \
  --node-summary target/wasm-tests/dev/reports/node/summary.json \
  --chrome-summary target/wasm-tests/dev/reports/chrome/summary.json \
  --output target/wasm-tests/dev/inventory.json
```

At the production base there were 1,420 native nextest IDs and 1,419 source test declarations. The Stage 1 harness adds five cross-target support tests, so the first migration inventory is 1,425 native IDs and 1,424 source declarations. The one-count differences are expected and visible rather than silently discarded: `shoop_common` macros generate two native IDs beyond its five declarations, while one `shoop_engine` source declaration is excluded from this native configuration.

| Package | Native IDs after Stage 1 | Source declarations after Stage 1 |
| --- | ---: | ---: |
| `shoop_app` | 93 | 93 |
| `shoop_app_api` | 20 | 20 |
| `shoop_audio_protocol` | 6 | 6 |
| `shoop_audio_worklet` | 13 | 13 |
| `shoop_backend` | 59 | 59 |
| `shoop_common` | 7 | 5 |
| `shoop_egui` | 160 | 160 |
| `shoop_engine` | 933 | 934 |
| `shoop_plugin_protocol` | 5 | 5 |
| `shoop_scripting` | 27 | 27 |
| `shoop_session` | 25 | 25 |
| `shoop_settings` | 14 | 14 |
| `shoop_tracing` | 5 | 5 |
| `shoop_wasm_test_support` | 5 | 5 |
| `shoop_worklet_client` | 18 | 18 |
| `shoopdaloop` | 35 | 35 |
| **Total** | **1,425** | **1,424** |

The first generated overlap report has 11 shared tests (six protocol and five harness tests), identically listed in Node and Chromium, with 1,414 native IDs explicitly marked `pending` in `tests/wasm_test_classification.toml`. Pending is a migration state, not a final exclusion; final `--require-closed` rejects it.

## Previous web CI selection

Before this project, web debug/release jobs ran native Linux nextest for five package trees, not Wasm test binaries:

| Package | Native IDs at baseline |
| --- | ---: |
| `shoop_audio_protocol` | 6 |
| `shoop_audio_worklet` | 13 |
| `shoop_worklet_client` | 18 |
| `shoop_egui` | 160 |
| `shoopdaloop` | 35 |
| **Total** | **232** |

The jobs separately compiled `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`, checked forbidden dependencies and worklet imports, and ran the raw-host Node contract. None of those steps executed Rust tests inside Wasm.

## CI timing snapshot

GitHub run 31897828126 at the production base supplied this cache-dependent snapshot:

| Web cell | Build | Host-native selected tests | Packaged browser workflows | Total job |
| --- | ---: | ---: | ---: | ---: |
| debug | 74 s | 54 s | 490 s | 11 m 44 s |
| release | 120 s | 100 s | 406 s | 12 m 03 s |

Debug ran seven hosted and six self-contained Chrome invocations. Release repeated those 13, then ran five extended Chrome invocations and one Firefox invocation. Stage 5 must map every assertion before replacing this 32-invocation two-profile total with the three-invocation irreducible boundary.

## Production prerequisite evidence

- `shoop_worklet_client` dependency isolation passes and contains no browser/native-driver ownership.
- `BrowserAudioDriver` and `BrowserWorkerDriver` attach through `RemoteBackendControl`/`MessageEndpoint`.
- `audio_worklet.js` and `audio_worker.js` import the same `raw_wasm_host.js`.
- The release worklet artifact is import-free. The Stage 0 staged debug artifact hash was `fc75c1c3fd0bff6f39e8d56dca5764ed63aceb4cc81f582301a2196611caa9f6`; profile manifests record current hashes on every run.
- `tests/wasm/node_worker_probe.mjs` loaded the exact staged production Worker/host modules through `node:worker_threads`, structured-cloned one compiled worklet module into two Workers, transferred independent production/fixture ports, processed one exact quantum, proved isolated callback counts, acknowledged both shutdowns, and observed clean Worker exits.
- Native, Node Wasm, and Chromium Wasm Stage 1 pilots execute the same test bodies. Their intentional failure canaries return nonzero and retain the panic text.
- Wasm dependency-tree checks reject CPAL, JACK, ALSA, midir, rodio, libloading, Tracy client, and Tracy nextest dependencies.

The browser production-Worker asset/Blob feasibility probe is completed by the first `shoop_wasm_runtime_tests` fixture in Stage 3 and is a blocker before broad package migration.

## Provisional classification

The baseline uses package-level pending rules only to ensure nothing disappears before test-by-test migration:

- protocol/value wave: app API, plugin protocol, settings, and session;
- portable runtime wave: scripting, engine, backend, worklet client, and application;
- UI/composition wave: egui, audio worklet, and browser composition;
- explicit native/platform review: common utilities, tracing, engine/backend physical drivers, global allocator/lock tests, filesystem guarantees, subprocesses, environment control, native UI, and Carla.

`wasm_test_inventory.py` supplements native/Node/Chromium runner IDs with source declarations, rejects unmatched or overlapping rules, rejects stale rules, and requires identical shared Node/Chromium sets. The final gate additionally rejects every remaining `pending` classification.

## Completed migration inventory

After the package waves, `--require-closed` reports:

| Category | Logical native tests |
| --- | ---: |
| shared | 1,170 |
| native-platform | 136 |
| native-driver | 119 |
| **Native total** | **1,425** |

The final source scanner records 1,428 declarations: the 1,424 native/cross-target declarations from Stage 1 plus four Wasm-runtime-only production Worker declarations. Node and Chromium each execute the identical 1,170 shared IDs plus four `wasm-runtime` production Worker contracts, for **1,174 actual Wasm tests per runtime**. No classification remains pending. The resulting shared overlap is about 82% of the native inventory; the 255 exclusions are dominated by native app-backend/JACK/CPAL/midir/Carla features, global allocation and lock gates, OS threads/deadlines/filesystems, and native Tracy. Every exclusion has a checked-in pattern and reason.

On the local warm debug-profile reference environment, the canonical package commands took about 156 seconds in Node and 192 seconds in headless Chromium, excluding the one-time staged worklet build. These values are diagnostic rather than acceptance thresholds and are recorded in each generated summary.

## Post-migration CI measurement

PR #751 run `31905533513` executed all 1,174 tests in each debug runtime with warnings denied and identical inventory hashes. Summed per-package runner times were about 359 seconds in Node and 361 seconds in Chromium; the owning CI steps, including orchestration and staging, each took about 6 minutes 13 seconds. The retained packaged smokes took 3.3 seconds for hosted Chromium, 13.4 seconds for self-contained Chromium, and 44.9 seconds for Firefox. Their combined runner time was about 62 seconds, well below the five-minute boundary and down from 32 packaged-browser invocations to three. Artifact build and Firefox installation remain outside that smoke measurement as required.
