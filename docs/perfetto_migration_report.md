# Perfetto migration report

## Result

ShoopDaLoop tracing is implemented through `shoop_tracing` with standard
`.pftrace` output on native, browser Window, Engine Worker, AudioWorklet, native
nextest, Node Wasm tests, and Chromium Wasm tests. Tracy dependencies, patches,
prebuilt setup, workflows, active documentation, and artifacts were removed.

The reviewed upstream is `SanderVocke/perfetto-everywhere` through merged PR 2
at commit `caaa26c067cc91a2b3f6dc9ca2a87da22071e5a4`. Shoop pins that exact
revision. The upstream additions include an import-free bounded raw-Wasm
producer, recyclable transferable browser chunks, configurable Worker assets,
wasm-bindgen 0.2.127 alignment, native static-name caching, and preservation of
equal-timestamp producer ordering.

## Architecture and semantics

- `shoop_tracing` is the only crate with direct `perfetto-everywhere*`
  dependencies. It owns gates, static realtime metadata, subscriber bridges,
  native/browser capture lifecycle, browser collection, and test retention.
- Native capture uses an in-process 64 MiB Perfetto session. Save writes a
  `.pftrace.partial` and atomically renames it; discard writes nothing; sessions
  can restart in one process.
- Browser Window records use `performance.now()`. The active Worker or
  AudioWorklet writes fixed 48-byte records into preallocated raw-Wasm storage,
  drains into a preallocated transferable ArrayBuffer pool, and publishes exact
  sample-frame timestamps, calibrations, metadata, loss, and high-water health.
  The Window continuously consumes each chunk and returns its detached buffer to
  the originating realm, so captures are not limited to one pool rotation or a
  fixed record count. A 512 MiB per-realm collector safety quota, allocation
  failure, or storage failure makes the capture incomplete and prevents save
  rather than silently truncating it.
- Browser final protobuf collection currently runs during explicit finalization
  on the Window thread rather than in a separate collector Worker. This avoids a
  second generated Wasm artifact while preserving bounded realtime producers and
  application-owned standard bytes. Finalization may briefly occupy the UI;
  callback continuity is unaffected.
- AudioWorklet/Worker internal slices at one sample-frame tick are logical
  ordering/stage records and commonly have zero duration. They are not callback
  CPU measurements. Native callback slices retain CPU-clock duration semantics.
- Integer counts, identifiers, occupancy, generations, and reason codes are i64
  counters. Fractional values use f64 counters.

Hosted browser multirealm capture uses recyclable transferable buffers and does not require COOP/COEP isolation headers. Unsupported/direct-file deployments keep running and report tracing as
unavailable.

## Test capture

`#[shoop_test]` wraps each eligible native attempt and Wasm testcase. Native uses
`SHOOP_TEST_TRACE=off|failure|always` plus `SHOOP_TEST_TRACE_DIR`; the Wasm runner
uses `--trace off|failure|always`. Names/report mappings include package or
binary, testcase, attempt where applicable, and a digest.

A default `wasm32-unknown-unknown` panic abort cannot execute Rust finalizers.
The external harness therefore persists a valid bootstrap trace before entering
each testcase and replaces it with the full trace after normal completion. On a
trap, the supervisor retains the bootstrap trace with testcase identity, span,
structured event/log, and counter plot. The next testcase discards abandoned
in-memory producer state before starting.

Intentional native, Node, and Chromium failure canaries each produced exactly
one queryable failure trace. Failure-only successful runs retained none; `always`
retained one full trace per eligible passing testcase.

## Validation evidence

Local verification on Linux 7.0.3, Rust 1.97.1, Node 22.22.2, and Chromium /
ChromeDriver 147.0.7727.137 included:

- `RUSTFLAGS="-D warnings" cargo build --workspace` — passed.
- Complete native nextest suite with failure tracing — 1,520 tests passed and
  three were skipped; no failure traces remained.
- Complete Node Wasm suite — all 16 packages and 1,261 tests passed.
- Complete Chromium Wasm suite — all 16 packages and 1,262 tests passed.
- `python3 scripts/check_shoop_test_usage.py` and
  `python3 scripts/check_tracing_coverage.py --require-closed` — passed; the
  inventory covers 139 production modules.
- Import-free raw worklet build and `raw_wasm_host_contract.mjs` — zero Wasm
  imports; metadata, exact source frame, bounded drain, and memory-growth view
  recovery passed.
- `python3 scripts/validate_perfetto_traces.py` — native application and
  synthetic Window/Worker/AudioWorklet traces passed pinned Trace Processor SQL.
- Hosted Chromium startup/save smokes produced
  `target/perfetto-validation/browser-worker.pftrace` and
  `browser-audio.pftrace`. Each contained Window plus the active engine realm,
  over 1,000 `engine.rt.callback` slices, producer health, two-point clock
  calibration, and zero Trace Processor import/clock errors. Each producer
  emitted more than the 8,192-record ring capacity with zero drops. The audio
  smoke also rebuilt the AudioWorklet graph during capture and retained both
  synchronized realm segments. The captures were produced from the packaged
  application through the transferable-buffer/raw-Wasm bridges.
- The retained hosted output-only AudioWorklet workflow passed after the tracing
  changes. Trunk debug packaging and the raw worklet artifact contract passed.

Upstream PR 1 passed browser, collector, MSRV, native, quality,
security/licenses, and Wasm checks before merge. The Shoop GitHub matrix remains
the authoritative evidence for Windows, macOS, release packaging, Firefox,
artifact upload, and clean CI. Final implementation run
[33319623816](https://github.com/SanderVocke/shoopdaloop/actions/runs/33319623816)
passed every PR check and retained ``perfetto-validation-linux-debug-33319623816``,
``perfetto-validation-web-debug-33319623816``, and
``wasm-test-reports-debug-33319623816`` alongside all platform packages. Live
branch and PR runs are linked from
[PR 818](https://github.com/SanderVocke/shoopdaloop/pull/818/checks).

## Native audio-domain comparison

The same release binary workload processed a fixed 16-loop dummy graph at 48 kHz
with 128-frame quanta, 2,000 warm-up cycles, 20,000 measured cycles, and five
process repetitions per mode. Full environment and raw repetitions are in
`tracy_audio_baseline.{md,json}` and `perfetto_audio_results.{md,json}`.

| Mode | Tracy median cycles/s | Perfetto median cycles/s | Tracy p99 | Perfetto p99 | Budget overruns |
|---|---:|---:|---:|---:|---:|
| Disabled | 227,498 | 213,950 | 8.64 µs | 10.85 µs | 0 / 0 |
| Coarse | 107,914 | 83,771 | 13.99 µs | 20.99 µs | 0 / 0 |
| Detailed | 81,257 | 13,702 | 21.11 µs | 137.91 µs | 0 / 0 |

No callback exceeded the 2.667 ms audio budget in any repetition. The largest
observed Perfetto callback was 1.993 ms, versus 0.938 ms for Tracy. Therefore no
major deadline/xrun regression was observed in this workload. However, a major
instrumentation-overhead regression **was** observed in detailed mode: throughput
fell about 83% relative to Tracy and p99 rose about 6.5×. Coarse throughput fell
about 22% and p99 rose about 50%. Native static-name caching was tested upstream
but did not materially remove the detailed cost, which is dominated by the
volume and SDK/debug-argument work. This is reported as requested and is not a
completion blocker; coarse mode remains the recommended first diagnostic mode.

## Known limitations and follow-ups

- Browser collector finalization is Window-owned and may cause a short UI pause.
- Browser logical engine slices do not claim CPU duration.
- A fatal process kill, timeout, OOM, or native panic-abort can prevent final
  publication. Ordinary Wasm panics finalize the active testcase trace from the
  panic hook; traps that bypass the hook retain the pre-published bootstrap.
- Firefox/Safari tracing is not claimed by the pinned upstream; existing Firefox
  application behavior remains independently smoke-tested.
- Trace files may contain paths, messages, and application state and must be
  treated as potentially sensitive.
