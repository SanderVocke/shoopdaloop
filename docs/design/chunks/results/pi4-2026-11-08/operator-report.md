# ShoopDaLoop Pi 4 benchmark report

## Test setup

- Host: Raspberry Pi 4, Cortex-A72, aarch64 kernel 6.1.21-v8+
- Userland: Debian/Raspberry Pi OS Bookworm armhf; private arm64 glibc 2.36 runtime added for the supplied ARM64 binaries
- Scheduler/kernel: `PREEMPT` (not PREEMPT_RT), render thread `SCHED_FIFO/80`
- CPU governor/frequency: performance, 1.5 GHz
- CPU placement: host CPU 0, render CPU 1, interference CPU 2
- Available cpuset was CPUs 0-2, so the README's render CPU 3 layout could not be used
- No isolated CPUs were configured
- Temperature after testing: 42.3 C; firmware throttle status `0x0`
- Deadlines: 5.333 ms at 256 frames; 0.667 ms at 32 frames
- All three modes were tested: `current`, `chunks`, `chunks-descriptors`
- Total measured callbacks: 1,388,187

Values below are median-of-run p50/p99 and the largest callback across all three repeats, in microseconds.

## Unloaded steady timing

### 256-frame buffer

| Scenario | Mode | p50 | p99 | max | overruns |
|---|---|---:|---:|---:|---:|
| idle | current | 395.1 | 491.1 | 548.9 | 0 |
| idle | chunks | 328.3 | 397.4 | 436.2 | 0 |
| idle | chunks-descriptors | 330.0 | 407.6 | 481.2 | 0 |
| play | current | 885.3 | 1020.6 | 1160.5 | 0 |
| play | chunks | 936.1 | 1105.1 | 1200.4 | 0 |
| play | chunks-descriptors | 939.3 | 1109.6 | 1202.5 | 0 |
| ramps | current | 972.7 | 1096.3 | 1204.8 | 0 |
| ramps | chunks | 1037.1 | 1229.0 | 1342.4 | 0 |
| ramps | chunks-descriptors | 1012.9 | 1188.2 | 1325.7 | 0 |

### 32-frame buffer

| Scenario | Mode | p50 | p99 | max | overruns |
|---|---|---:|---:|---:|---:|
| idle | current | 68.8 | 120.7 | 229.8 | 0 |
| idle | chunks | 61.4 | 112.9 | 203.7 | 0 |
| idle | chunks-descriptors | 60.4 | 111.0 | 229.9 | 0 |
| play | current | 134.2 | 226.4 | 356.6 | 0 |
| play | chunks | 142.1 | 241.9 | 367.1 | 0 |
| play | chunks-descriptors | 143.0 | 241.6 | 433.6 | 0 |
| ramps | current | 157.4 | 299.1 | 448.5 | 0 |
| ramps | chunks | 170.0 | 318.8 | 418.4 | 0 |
| ramps | chunks-descriptors | 160.4 | 304.2 | 443.8 | 0 |

At idle, the experimental modes reduced median callback time by about 11-17%. During playback they regressed by about 6%. During ramps, descriptors were better than plain chunks but remained about 2-4% slower than current.

## Interference

Periodic activity caused no overruns at either buffer size. Its largest callback was 1.624 ms at 256 frames and 0.629 ms at 32 frames.

Continuous 64 MiB copies caused severe shared-memory pressure:

| Buffer/scenario | current | chunks | chunks-descriptors |
|---|---:|---:|---:|
| 256 idle max / overruns | 1553 us / 0 | 1162 us / 0 | 1286 us / 0 |
| 256 play max / overruns | 5040 us / 0 | 5669 us / 113 | 5453 us / 15 |
| 256 ramps max / overruns | 5238 us / 0 | 5720 us / 569 | 5602 us / 49 |
| 32 idle max / overruns | 771 us / 61 | 737 us / 25 | 728 us / 3 |
| 32 play max / overruns | 53291 us / 36089 | 53025 us / 40129 | 53277 us / 42481 |
| 32 ramps max / overruns | 53046 us / 40353 | 53195 us / 44065 | 53202 us / 42868 |

For 32-frame memory-pressure play+ramps, overrun rates were approximately 84.9% current, 93.5% chunks, and 94.8% chunks-descriptors. None is safe at this buffer size under this load.

## Looping operations (256 frames)

| Scenario | Mode | p50 | p99 | max | overruns |
|---|---|---:|---:|---:|---:|
| record | current / chunks / descriptors | 859.6 / 822.8 / 829.9 | 978.3 / 939.5 / 931.6 | 1019.8 / 985.9 / 1006.1 | 0 / 0 / 0 |
| record-play | current / chunks / descriptors | 950.9 / 996.3 / 979.4 | 1088.8 / 1129.3 / 1133.4 | 1146.0 / 1221.2 / 1196.8 | 0 / 0 / 0 |
| shared | current / chunks / descriptors | 950.1 / 939.5 / 932.0 | 6215.3 / 6325.1 / 6354.5 | 6273.7 / 6557.8 / 6462.8 | 54 / 55 / 55 |
| continue | current / chunks / descriptors | 1183.8 / 1070.7 / 1071.4 | 1322.3 / 1224.2 / 1232.2 | 1371.9 / 1320.3 / 1316.7 | 0 / 0 / 0 |
| churn | current / chunks / descriptors | 884.0 / 854.2 / 850.2 | 1226.9 / 1946.3 / 1766.1 | 1359.0 / 6926.6 / 6537.5 | 0 / 3 / 3 |

## Correctness and conclusion

- Smoke playback/grab passed for all modes.
- Zero dropped timing records across all runs.
- Zero incomplete captures in archived evidence.
- No thermal throttling was reported.
- Experimental chunk modes clearly improve idle timing, but do not provide a general timing win.
- `current` is fastest for ordinary playback and generally most robust under memory pressure.
- Descriptors improve ramp behavior versus plain chunks, but not enough to beat current consistently.
- Shared-memory saturation dominates all implementations at 32 frames, producing roughly 53 ms worst callbacks.

Raw evidence archive: `pi4-results.zip`.
