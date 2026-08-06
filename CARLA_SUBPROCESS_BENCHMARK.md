# Carla bridge benchmark

## Contract and command

`carla_bridge_benchmark` measures the final session-facing bridge at 48 kHz. Each row uses 100 warm-up blocks and 500 paced blocks. Timing starts immediately before submission and ends after completion or the one-period deadline. Audio uses the fixed three-slot shared-memory layout; MIDI uses fixed 1,024-event pools with four inline bytes per event, matching the engine's supported MIDI storage. Local callback-to-bridge notification uses `Thread::unpark`; subprocess bridge-to-worker notification uses a nonce-derived 16-byte loopback UDP datagram. Audio/MIDI payloads never use the notification channel.

```text
cargo build --release -p shoopdaloop --bin shoopdaloop \
  -p shoop_engine --example carla_bridge_benchmark \
  --features shoop_engine/app_backend

target/release/examples/carla_bridge_benchmark \
  target/release/shoopdaloop direct

target/release/examples/carla_bridge_benchmark \
  target/release/shoopdaloop subprocess

target/release/examples/carla_bridge_benchmark \
  target/release/shoopdaloop reference
```

## Linux result

Measured on the same x86_64 NixOS PREEMPT_RT host as `CARLA_SUBPROCESS_BASELINE.md`. Units are microseconds.

| Mode | Chain | Frames | Mean | p50 | p95 | p99 | Max | Misses |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| direct | Rack 2ch | 32 | 15.851 | 14.497 | 23.284 | 33.653 | 118.684 | 0 |
| subprocess | Rack 2ch | 32 | 36.405 | 34.845 | 54.152 | 76.835 | 150.864 | 0 |
| direct | Rack 2ch | 64 | 18.467 | 17.693 | 22.693 | 31.780 | 38.272 | 0 |
| subprocess | Rack 2ch | 64 | 39.838 | 38.211 | 58.660 | 73.107 | 110.027 | 0 |
| direct | Rack 2ch | 128 | 49.353 | 45.946 | 89.588 | 168.026 | 272.142 | 0 |
| subprocess | Rack 2ch | 128 | 52.178 | 52.969 | 80.571 | 117.121 | 267.884 | 0 |
| direct | Rack 2ch | 256 | 112.870 | 126.067 | 194.005 | 218.732 | 319.591 | 0 |
| subprocess | Rack 2ch | 256 | 87.053 | 69.590 | 226.366 | 356.060 | 447.171 | 0 |
| direct | Rack 2ch | 512 | 104.187 | 104.005 | 174.999 | 209.714 | 261.372 | 0 |
| subprocess | Rack 2ch | 512 | 236.662 | 242.716 | 387.178 | 459.595 | 738.579 | 0 |
| direct | Rack 2ch | 1024 | 112.428 | 111.830 | 176.562 | 220.375 | 363.474 | 0 |
| subprocess | Rack 2ch | 1024 | 271.841 | 282.341 | 402.147 | 477.959 | 1009.368 | 0 |
| direct | Patchbay 16ch | 32 | 26.541 | 24.586 | 43.512 | 62.248 | 96.572 | 0 |
| subprocess | Patchbay 16ch | 32 | 39.805 | 37.611 | 57.478 | 82.154 | 120.185 | 0 |
| direct | Patchbay 16ch | 64 | 32.188 | 28.904 | 51.417 | 70.974 | 648.620 | 0 |
| subprocess | Patchbay 16ch | 64 | 43.400 | 40.096 | 63.980 | 100.219 | 179.668 | 0 |
| direct | Patchbay 16ch | 128 | 42.257 | 42.370 | 60.935 | 153.148 | 233.559 | 0 |
| subprocess | Patchbay 16ch | 128 | 65.033 | 58.651 | 104.607 | 125.967 | 141.156 | 0 |
| direct | Patchbay 16ch | 256 | 141.226 | 169.679 | 237.627 | 268.695 | 310.804 | 0 |
| subprocess | Patchbay 16ch | 256 | 118.793 | 98.876 | 258.526 | 415.291 | 549.172 | 0 |
| direct | Patchbay 16ch | 512 | 172.241 | 176.301 | 262.223 | 333.157 | 1666.584 | 0 |
| subprocess | Patchbay 16ch | 512 | 277.929 | 249.068 | 478.940 | 543.933 | 804.142 | 0 |
| direct | Patchbay 16ch | 1024 | 214.647 | 217.529 | 287.210 | 330.932 | 3222.581 | 0 |
| subprocess | Patchbay 16ch | 1024 | 389.938 | 422.524 | 567.036 | 607.642 | 1172.925 | 0 |

No deadline misses occurred in 6,000 measured blocks per mode. The worst observed subprocess call was 1.173 ms against a 21.333 ms 1,024-frame budget; at the smallest 32-frame budget (0.667 ms), the worst call was 0.151 ms.

Shell process accounting, which includes benchmark pacing, Carla work, bridge threads, and worker startup/teardown, was:

| Mode | Real s | User s | System s | CPU / one core |
|---|---:|---:|---:|---:|
| direct | 44.411 | 1.879 | 1.616 | 7.9% |
| subprocess | 47.858 | 3.925 | 5.288 | 19.2% |
| serialized reference | 48.182 | 8.691 | 5.288 | 29.0% |

The subprocess CPU delta includes one authenticated UDP wake and two shared-memory copies per block. It avoids the unbounded idle spinning seen in the prototype. Under the same paced bridge interface, the retained framed-JSON reference had 2-channel medians from 82 to 793 microseconds and 16-channel medians from 165 to 2,305 microseconds. The shared-memory subprocess medians were 35–282 and 38–423 microseconds respectively, supporting rejection of per-block serialization for the final path.

## Mechanism and dependency audit

The final bulk mapping uses `memmap2` (MIT/Apache-2.0, maintained, native Windows/Unix mapping support) over a restrictive random `tempfile` (MIT/Apache-2.0). Control and notification use the Rust standard library's cross-platform loopback TCP/UDP APIs. `Thread::unpark` supplies the in-process wake without adding a queue dependency. The mapping validates file length, protocol/layout version, nonce, generation, and capacities before use; atomics own every slot transition. UDP carries only a fixed nonce-derived wake token, so loss causes a bounded wet fallback and spoofed datagrams cannot alter payload or state. TCP has explicit frame and request timeouts. Parent disconnect and generation-specific cleanup cover crash behavior.

An anonymous/in-process ring queue was rejected because it does not cross a process boundary. Named platform-specific events were rejected for now because they would add three lifecycle/permission implementations without outperforming the measured portable wake path. Continuous polling was measured and rejected for idle CPU cost. Per-block framed JSON is retained only as a tested reference because the comparison above shows materially worse channel/frame scaling.

## Interpretation and remaining platform evidence

The selected mechanism is the shared-memory transport plus bounded native notification described above. Framed TCP remains control-only and the old serialized block message remains only as a protocol reference/test surface. Linux measurements support the capacities and one-period deadline. Equivalent release measurements still need to be produced by Windows and macOS package hosts; Linux numbers must not be presented as evidence for those schedulers.
