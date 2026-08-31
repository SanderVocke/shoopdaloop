# Tracy audio tracing baseline

This is a same-process release-mode diagnostic benchmark, not a hardware-independent
performance claim. It exercises a fixed 16-loop dummy-engine graph at 48 kHz with
128-frame quanta. Callback timing is measured around every cycle in every mode; the
engine's own timing is active only when tracing is requested.

- UTC: `2026-08-30T06:59:19.820803+00:00`
- Git: `563d08b835598aa5ae78e673342560e636f76657`
- Platform: `Linux-7.0.3-x86_64-with-glibc2.42`
- CPU: `AMD Ryzen AI 7 350 w/ Radeon 860M`
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Tracy client: `0.18.4 / Tracy 0.13.1`
- Embedded capture/query: `v0.7.0`
- Inspected Perfetto candidate: `48ed779`
- Repetitions: 5
- Measured cycles per repetition: 20000

| Mode | Median cycles/s | Median p50 µs | Median p95 µs | Median p99 µs | Median max µs | Median budget overruns |
|---|---:|---:|---:|---:|---:|---:|
| disabled | 227498 | 3.94 | 7.02 | 8.64 | 25.01 | 0 |
| coarse | 107914 | 8.54 | 12.74 | 13.99 | 65.34 | 0 |
| detailed | 81257 | 10.80 | 18.06 | 21.11 | 285.60 | 0 |

Raw repetitions are in the adjacent JSON file. Scheduler noise and machine load
remain visible in maxima; compare medians and upper percentiles together, and rerun
the identical command for Perfetto on the same machine.

## Capture validation

The application smoke capture was finalized as a non-empty 550-byte
`traces/0001-application.tracy` with no partial file. The matching v0.7.0
`tracy-query-linux-x86_64` accepted `check` and `info`; the trace reported Tracy
0.13.1, two messages, one plot point, and three frame marks. This query returned
the expected structured smoke event:

```sh
cargo run -p shoopdaloop -- --tracing --tracing-smoke-test
tracy-query-linux-x86_64 check traces/0001-application.tracy
tracy-query-linux-x86_64 query --kind message \
  --filter 'message.text=frontend\.egui\.tracing_smoke_test' \
  traces/0001-application.tracy
```

The realtime metadata/type contract for the migration is inventoried in
`docs/perfetto_realtime_metadata.csv`. All nine distinct current plots represent
counts, identifiers, occupancy, or reason codes and therefore map to Perfetto i64
counters; future fractional load/ratio plots use f64.
