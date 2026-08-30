---
name: perfetto
description: Capture and investigate ShoopDaLoop native, browser, AudioWorklet, Worker, and per-test Perfetto .pftrace files.
compatibility: ShoopDaLoop uses the pinned perfetto-everywhere revision and standard Perfetto Trace Processor files.
---

# Debug ShoopDaLoop with Perfetto

Use the repository-pinned `scripts/trace_processor` wrapper. It downloads the
matching official binary with its embedded platform/checksum manifest; do not
commit the downloaded binary or generated traces.

## Native application capture

```sh
cargo run -p shoopdaloop -- --tracing
cargo run -p shoopdaloop -- --tracing --tracing-engine-detail
scripts/trace_processor traces/0001-application.pftrace
```

Tracing can also be started in **Settings > Developer**. Save atomically
publishes a numbered `.pftrace`; Discard emits no file; another capture may then
start in the same process. Detailed engine recording increases callback overhead.

## Browser capture

Hosted Chromium exposes the same developer controls. Multirealm engine tracing
requires `SharedArrayBuffer` and therefore COOP `same-origin` plus COEP
`require-corp`. Save downloads one `.pftrace` containing Window and the active
Engine Worker or AudioWorklet realm. Direct-file or unsupported browsers report
tracing unavailable rather than silently omitting an active realm.

AudioWorklet timestamps are exact logical sample frames. They do not measure
callback CPU-entry/exit duration. Inspect clock snapshots, calibration events,
discontinuities, dropped records, and high-water health before comparing realms.

## Test failure traces

Native nextest and Node/Chromium Wasm tests use `off | failure | always` capture.
CI defaults to failure-only and uploads finalized `.pftrace` files. Native names
include binary, testcase, attempt, and attempt digest. Wasm trace paths are also
recorded in report JSON/JUnit artifacts.

```sh
SHOOP_TEST_TRACE=always \
SHOOP_TEST_TRACE_DIR="$PWD/target/test-traces" \
cargo nextest run -p shoop_engine --profile ci

python3 scripts/run_wasm_tests.py --runtime node --profile dev --trace always
python3 scripts/run_wasm_tests.py --runtime chrome --profile dev --trace always
```

A default Wasm panic abort cannot run Rust finalizers. The harness therefore
publishes an externally owned bootstrap trace before the testcase and replaces
it with the full trace on normal completion. A failing trace remains valid and
identified but may end at the last externally published boundary.

## Query workflow

Validate structure and inventory before narrowing:

```sh
TRACE=traces/0001-application.pftrace
scripts/trace_processor --query-file /dev/stdin "$TRACE" <<'SQL'
SELECT start_ts, end_ts FROM trace_bounds;
SELECT name, COUNT(*) AS count, ROUND(SUM(dur) / 1e6, 3) AS total_ms
FROM slice GROUP BY name ORDER BY total_ms DESC LIMIT 50;
SELECT t.name, COUNT(*) AS samples, MIN(c.value), MAX(c.value)
FROM counter c JOIN counter_track t ON c.track_id = t.id
GROUP BY t.name ORDER BY t.name;
SQL
```

Shoop's stable families remain `frontend.egui.*`, `frontend.app.*`,
`engine.control.*`, `engine.graph.*`, `engine.rt.*`, `engine.fx.*`, and
`worker.*`. Correlate app dispatch/handle using `intent_id`; follow topology
changes into the next callback; compare callback duration with the frame budget
only on native CPU-clock tracks.

Structured logs are instant slices with typed debug arguments. Query `args`
through each slice's `arg_set_id`; do not infer severity from color or payload
words. Distinguish observed records from interpretation and report the trace,
query, filters, range, and relevant output behind each conclusion.

Tracing is diagnostic and can change timing. Compare equivalent workloads and
coarse/detail modes, inspect drops, and do not treat one capture as a transparent
performance measurement.
