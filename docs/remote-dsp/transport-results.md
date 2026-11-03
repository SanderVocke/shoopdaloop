# Matched UDP/TCP remote-DSP results

## Summary

**TCP works with the client-driven pipeline at the selected 0.833–3.000 ms modeled
audio latencies on this machine.** All 34 TCP runs, including eight 60-second
runs and two deliberate source-pause runs, had **zero output misses, zero input
concealment, zero retirement, and zero sample-verification errors**.

This is not evidence that TCP is universally faster or better than UDP. The
matched complete-input UDP runs were also healthy. TCP had a few microseconds
more typical post-submission turnaround here. The important semantic distinction
was **waiting for original input versus expiring an input gap**, not simply the
transport label.

The capture completed on **2026-11-03, 14:20:55–14:48:24 UTC**, sequentially:

- **102/102 records:** 72 short runs, 24 fixed 60-second runs, six separately
  labeled source-pause runs.
- **7,146,000 measured output slots**, **152,640,000 verified samples**.
- **Zero late outputs, missing outputs, incorrect samples, unverified samples,
  stream retirements, application send errors/skips or deliberately omitted input.**
- **Seven zero-substituted UDP input packets:** one in an ordinary long run,
  three in each of the two deliberately paused UDP-gap runs. No TCP or
  complete-input UDP substitution.

Verification is policy-aware: correctly returned zeros after recorded input
concealment are not proof that the original sound was preserved.

This is a **localhost** comparison. The
[two-machine cable guide](cable-benchmarks.md) and runner are provided, but no
physical two-host cable measurement or real network-loss injection is claimed.
The [cloud evidence](localhost-results.md), [cloud tuning](tuning-results.md) and
[earlier local reproduction](local-results.md) remain separate and unchanged.

## Implementation and matched conditions

Audio can now use UDP or a separate authenticated TCP connection. TCP carries a
four-byte little-endian length followed by the existing audio packet, preserves
partial reads/writes, uses `TCP_NODELAY` at both ends, and has bounded preallocated
transmit storage. Queue exhaustion and connection/framing errors are explicit;
they do not silently discard bytes and continue a broken stream.

The server remains driven by the client's source sequence/frame timeline. Its
clock measures elapsed time, not an independent audio cadence. The new
complete-input operation will not expire, zero-fill or advance an incomplete DSP
block. Existing inactivity, bounded-window and lifecycle limits still apply.

Three variants separate transport from input policy:

| Variant | Transport | Input policy |
| --- | --- | --- |
| `udp-gap` | UDP | Existing `(k-1)*P/fs + 200 us` gap expiry and zero substitution |
| `udp-complete` | UDP | Process only complete blocks; no substitution |
| `tcp-complete` | TCP | Process only complete blocks; no substitution |

All used stereo 48 kHz delay-copy DSP with `A=0`, yielding polling, 64 reorder
packets, 1500-byte logical MTU admission, 0.5-second warm-up, fixed output deadlines,
and catch-up without intentional overdue-source skipping. The UDP controls were
fresh runs of the **same new executable**, not reused historical UDP counts.
TCP always catches up; the runner explicitly enables the same behavior for UDP.

There were three shuffled 2-second repetitions per condition, followed by a
fresh, independently shuffled 60-second run of **every** condition. Failed timing
outcomes were not replaced with reruns, nor were only the best settings selected.

The host was the same Ryzen AI 7 350 / NixOS Linux 7.0.3 PREEMPT_RT laptop as in the
local reproduction. Ordinary scheduling used the allowed CPU mask 0–15; FIFO
priority 1 pinned the requester to **CPU 2** and server to **CPU 4**, separate
physical cores. Niceness was **0 throughout**. Actual per-thread policy, priority
and affinity were audited. Pinning is not isolation, and SMT siblings were not
reserved. This compares transports **within** each scheduling row; ordinary versus
FIFO also changes affinity and does not isolate priority's causal effect.

No builds/tests or automatic system/network tuning ran during the capture. Both
endpoint inventories, CPU topology, power/governor observations, permission
probes and before/after host-wide network counters are retained. The visible
cgroup information does not establish the absence of all ancestor limits.

## Fixed 60-second comparisons

Each cell is **output misses / zero-substituted input packets**. Output misses
are late plus missing; concealed-input counts include warm-up. All rows have
zero retirement and verification errors.

| P/k/N | Modeled audio ms | Scheduling | UDP-gap | UDP-complete | TCP-complete |
| --- | ---: | --- | ---: | ---: | ---: |
| 8/1/4 | 0.833 | Ordinary, unpinned | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/4 | 0.833 | FIFO 1, pinned | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/8 | 1.500 | Ordinary, unpinned | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/8 | 1.500 | FIFO 1, pinned | 0 / 0 | 0 / 0 | 0 / 0 |
| 16/4/4 | 1.667 | Ordinary, unpinned | 0 / 0 | 0 / 0 | 0 / 0 |
| 16/4/4 | 1.667 | FIFO 1, pinned | 0 / 0 | 0 / 0 | 0 / 0 |
| 16/4/8 | 3.000 | Ordinary, unpinned | **0 / 1** | 0 / 0 | 0 / 0 |
| 16/4/8 | 3.000 | FIFO 1, pinned | 0 / 0 | 0 / 0 | 0 / 0 |

Modeled latency is `((N+1)*P+A)/fs`: packetization plus the allowance and algorithmic
delay. Aggregation/turnaround must fit inside the allowance, not be added again.
It assumes sample-paced playback at the fixed consumption deadlines and excludes
audio devices, ADC/DAC, callback/driver buffering and clock-domain adaptation.

The isolated ordinary UDP-gap concealment was mask `8` in the block beginning at
sequence **33976**: its final input packet was substituted, yet its output was
available on time. The observations do not prove whether that packet was sent or
read late, genuinely dropped, or bypassed by timeout-versus-socket-draining order.
Do not label it a measured network drop.

All **72 short runs** had zero output misses, concealment and retirement. The
complete per-condition/repetition counts are in the
[generated tables](transport-evidence/report.md).

### Transport overhead with the same complete-input policy

These are individual 60-second-run **p99 batch turnaround times in microseconds**:
from application submission of the batch's final input packet to observation of
all its output packets by the requester. They are not one-way network latency,
not the modeled audio latency, and exclude waiting for the earlier input packets
to accumulate. Sender lateness, RTT and the full histograms are retained separately.

| P/k/N | Scheduling | UDP-complete p99 us | TCP-complete p99 us |
| --- | --- | ---: | ---: |
| 8/1/4 | Ordinary, unpinned | 3.166 | 4.969 |
| 8/1/4 | FIFO 1, pinned | 3.306 | 5.851 |
| 8/1/8 | Ordinary, unpinned | 3.607 | 5.030 |
| 8/1/8 | FIFO 1, pinned | 3.356 | 5.671 |
| 16/4/4 | Ordinary, unpinned | 6.933 | 14.277 |
| 16/4/4 | FIFO 1, pinned | 7.514 | 13.135 |
| 16/4/8 | Ordinary, unpinned | 9.749 | 13.204 |
| 16/4/8 | FIFO 1, pinned | 8.036 | 12.743 |

TCP's largest observed long-run batch turnaround was **976.907 us**, in ordinary
`16/4/8`; every output still met its deadline. One run per long condition is not
a statistically robust estimate of a worst-case bound or scheduler superiority.
The two-process polling pair consumed approximately **two CPU cores**.

Both transports reported **2,097,152-byte socket receive/send buffers** at both
endpoints. TCP recorded **zero write-would-block events, no transport errors and
zero remaining application-queue bytes**. Queue high-water marks were only
**132–196 bytes**. Consequently this is not a sustained TCP backpressure,
congestion or retransmission experiment.

## Controlled 2 ms source pause

These six 1-second runs were separate from the ordinary measurements. At
`16/4/8`, dispatch slept for 2 ms immediately after submitting the first measured
packet, then caught up. Source readiness and output deadlines did not move.

| Variant | Ordinary: misses / concealed | FIFO 1: misses / concealed |
| --- | ---: | ---: |
| UDP-gap | **0 / 3** | **0 / 3** |
| UDP-complete | 0 / 0 | 0 / 0 |
| TCP-complete | 0 / 0 | 0 / 0 |

The default input gap wait here is **1.2 ms**, while the first packet's output
allowance is **2.667 ms** after readiness. The injected pause exceeded the input
wait but left time within the output allowance. UDP-gap processed the first
packet plus three zero-substituted regions (mask `14`, batch sequence `1500`).
Both complete-input variants instead waited for the original remaining packets
and returned their correct results before the deadlines.

**This demonstrates the input-policy distinction.** It does not demonstrate TCP
recovering a lost network packet: no input frame was intentionally dropped, and
complete-input UDP succeeded too. Under genuine UDP loss, complete-input mode has
no retransmission mechanism and can stall until bounded-window/inactivity
retirement. TCP can recover lost bytes while its connection remains viable, but
retransmission and ordered delivery can still miss audio deadlines.

## Reproduce and move to two machines

Build in the repository environment, finish validation, then run the matrix:

```sh
nix develop --command env RUSTFLAGS="-D warnings" \
  cargo build --release -p shoop_dsp_host --no-default-features
nix develop --command python3 scripts/benchmark_remote_dsp_transports.py \
  --client-cpu 2 --server-cpu 4 \
  --output artifacts/new-transport-comparison \
  --publish docs/remote-dsp/new-transport-evidence
```

Use a fresh output directory for a changed source, executable, host or condition.
The actual capture used a frozen copy at
`artifacts/remote-dsp-transports-bin/shoop_dsp_host`; its executable SHA-256 was
`626d7470b1bbb46aa0284ae0be606d3ef2b0fe28a6019d19ade0cf78e7e97988`.
The base revision was `ec4b52ec2de23b60714eadb8ebf1be69834dd3e0`, **plus the
uncommitted implementation captured in the patch**, not that revision alone.

For the later physical link, use the [cable guide](cable-benchmarks.md), including
its complete SSH command, fixed control/audio ports, private shared credential,
route/MTU checks and explicit limitations. Remote-server CPU is not included in
controller child CPU measurements and must not be mistaken for a two-host CPU
comparison. No client/server wall-clock synchronization is needed for these
application-level RTT/deadline observations.

## Evidence and validation

- [Manifest and expected cases](transport-evidence/manifest.json),
  [completion inventory](transport-evidence/finished.json).
- [All 102 records and histograms](transport-evidence/results.jsonl.gz),
  [individual CSV rows](transport-evidence/summary.csv),
  [audit](transport-evidence/audit.json),
  [generated tables](transport-evidence/report.md).
- [Exact capture implementation patch](transport-evidence/implementation.patch):
  applying it to the manifest revision reconstructed all **30** hashed source
  files. A cached resume preserved **109** result/evidence files byte-for-byte,
  including the archive and original completion inventory.
- [Post-capture runner maintenance](transport-evidence/runner-maintenance.patch)
  adds process-group cleanup, absolute SSH-root validation and a regression test.
  No native code or recorded outcome was changed. It reconstructs the current
  runner/tests on top of the exact capture patch; report-only regeneration still
  preserves the archived payload.
- [Validation record](transport-evidence/validation.json): warning-denying workspace
  build, formatting, test-attribute and closed tracing-inventory checks passed;
  **2,560 native tests passed, four skipped**, with required real-Carla validation
  enabled. Targeted real-Carla tests verified UDP gain processing and TCP restored
  state. **163 Python script tests passed**, including nine runner tests.

Raw credentials/logs remain in ignored `artifacts/`; publication excludes secret
content and credential hashes. Original cloud evidence was checked unchanged.
The payload's decompressed SHA-256 is
`19936987a6c8d66f429ad717de306f7154797377a486b5208f51682e50dddf24`.

These results support using TCP for the desired client-driven, complete-input
model. They do **not** establish a universal transport winner, actual packet-loss
rates, physical-LAN behavior or a hard-real-time guarantee.
