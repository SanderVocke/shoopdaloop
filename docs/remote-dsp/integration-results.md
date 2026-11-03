# Graph integration validation

A clean detached checkout of `13ff3c0d166be8359fe03b38f360edf0a9f8ad22`
built the native graph demo with Carla and completed all six reproduction cases.
The source was clean when captured. The cases verified **55,002 received samples
with zero mismatches or unverified samples**. They exercise an actual
`rt_runtime::GraphRuntime` using the same portable link node.

This is correctness evidence. Stepped mode waits for the service between graph
calls, outside rendering; its elapsed time does not establish a real-time latency.
The paced case uses a software clock on localhost in the managed Linux environment.
It retained 96 fallback samples and one consumed concealed output block.

| Case | Q / B | Maximum callback | L (frames) | Verified samples | Fallback samples |
| --- | --- | --- | --- | --- | --- |
| Non-network reference | 3 / 5 | 7 | 27 | 1,946 | 0 |
| UDP, smaller callback | 16 / 64 | 7 | 1,136 | 5,920 | 0 |
| UDP, larger callback | 16 / 64 | 96 | 1,312 | 5,568 | 0 |
| Carla gain 0.5 | 16 / 64 | 64 | 1,248 | 5,696 | 0 |
| Carla saved-state restoration | 16 / 64 | 64 | 1,248 | 5,696 | 0 |
| Software-paced UDP | 16 / 64 | 64 | 1,248 | 30,176 | 96 |

All cases used 48 kHz stereo. Reference delay-copy used A=17, stepped UDP used
A=67, and paced UDP used A=35. Delay-copy was checked bit-exactly against its
source and known input concealment. Missing outputs were checked as silence,
not counted as verified DSP responses. The conservative graph latency includes
callback/assembly guards and is distinct from the original network benchmark N.

Each Carla phase explicitly reports 4,096 warm-up source frames (8,192 stereo
samples), followed by positive verified gain output. The fixture smooths gain
from zero, and the gain comparison uses the existing 0.0001 tolerance. Plugin
latency metadata remained unknown in both phases; successful fixture verification
does not imply automatic latency discovery. The two phases account for 16,384
warm-up samples in total.

## Evidence and reproduction

The [manifest](integration-evidence/manifest.json) records source/binary identity,
platform, commands, process outcomes and every case report. The
[audit](integration-evidence/audit.json) checks active-output sample accounting,
positive processed output, observation completeness and file hashes, and records
the pinned Carla source and actual library hashes. Individual case JSON files
are alongside them. Credentials, raw process logs and plugin state files are
excluded from the published evidence.

From the recorded revision or a later compatible checkout, prepare the pinned
Carla runtime and its development overrides as described in the host documentation:

```sh
cargo build --locked -p shoop_dsp_link_demo --features carla
python3 scripts/smoke_dsp_link.py --binary target/debug/shoop_dsp_link_demo \
  --carla --paced --output work/dsp-link-reproduction
```

Use a fresh output directory; append `.exe` on Windows. Without Carla, omit its
feature and the `--carla` flag. Omit `--paced` for the correctness-focused CI subset.
The [demo guide](../../src/rust/shoop_dsp_link_demo/README.md) also covers manual
connections and platform binding sketches.

After review corrections, the complete local native suite passed 2,576 tests
with four configured skips; all 23 focused DSP/link/node/demo tests passed in
release. Render heap guards, peer death/stall/repeated-reply cases,
graph activation/retirement/replacement and
bounded MIDI recovery are covered by tests. Cortex-A32/A64, Cortex-M, WebAssembly
library and affected application/worklet builds passed.

Review added explicit bounds before externally selected MIDI/queue allocations,
including a heap-guarded rejection test. The UDP correctness tests permit counted
deadline misses while still requiring positive processed output, zero incorrect
or unverified samples, and no lost observations. Their original zero-fallback
assertion failed under both CI and local scheduling contention. Stepped graph
time does not pause the UDP server's wall-clock gap handling; zero misses are
therefore not a portable CI requirement. The synchronous reference still requires
zero fallback. These corrections do not change the captured six-case results.

Delivery source `5d2b891c` passed the complete
[build/test matrix](https://github.com/SanderVocke/shoopdaloop/actions/runs/41608700624),
including native Linux/Windows/macOS debug and release, both browser profiles,
bare-metal builds and required-Carla coverage. Patch coverage was 90.26% against
the unchanged 80% requirement. CodeQL and documentation passed, and all four
allocation review threads are resolved. Delivery is in
[PR #922](https://github.com/SanderVocke/shoopdaloop/pull/922).

The original network and scheduling campaigns are preserved unchanged. This
milestone supplies no production IPC adapter, FPGA driver or physical-LAN timing
guarantee, and does not add automatic graph latency compensation.
