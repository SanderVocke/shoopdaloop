# Perfetto audio tracing benchmark

This is a same-process release-mode diagnostic benchmark, not a hardware-independent
performance claim. It exercises a fixed 16-loop dummy-engine graph at 48 kHz with
128-frame quanta. Callback timing is measured around every cycle in every mode; the
engine's own timing is active only when tracing is requested.

- UTC: `2026-08-30T08:18:53.608316+00:00`
- Git: `548f4e6d310af8b4de07cb785152c490517e4026`
- Platform: `Linux-7.0.3-x86_64-with-glibc2.42`
- CPU: `AMD Ryzen AI 7 350 w/ Radeon 860M`
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Repetitions: 5
- Measured cycles per repetition: 20000

| Mode | Median cycles/s | Median p50 µs | Median p95 µs | Median p99 µs | Median max µs | Median budget overruns |
|---|---:|---:|---:|---:|---:|---:|
| disabled | 213950 | 3.92 | 7.96 | 10.85 | 164.07 | 0 |
| coarse | 83771 | 10.65 | 16.90 | 20.99 | 374.69 | 0 |
| detailed | 13702 | 65.07 | 113.80 | 137.91 | 1573.19 | 0 |

Raw repetitions are in the adjacent JSON file. Scheduler noise and machine load
remain visible in maxima; compare medians and upper percentiles together, and rerun
the identical command for Perfetto on the same machine.
