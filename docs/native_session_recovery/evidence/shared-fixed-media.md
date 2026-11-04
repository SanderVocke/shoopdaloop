# Shared audio/MIDI on fixed execution

The shared foundation uses the sole production graph and domain at
Q=1/16/32/64/128. Application P04/P05 remain open. The
[implementation plan](../../../PLAN_RECONCILE_GRAB.md) tracks final-head delivery;
[PR #927](https://github.com/SanderVocke/shoopdaloop/pull/927) records the delivered
head/base, applicable matrix and latest artifacts. The user waived macOS checks
on 2026-11-04 after runner queues blocked independent validation; queued macOS
jobs are not represented as passes. Other validation requirements remain in force.
Parent delivery is #930; this branch
also includes the audio foundation from #909. Full run 6221 subsequently passed
on `7dad1991`, including macOS. The final consolidation through target `df468352`
is pushed without another CI waiting round at the user's request; the tested
preceding head and the final handoff head remain distinguished in the plan/PR.

## Functional evidence

[Run 6214 foundation](https://github.com/SanderVocke/shoopdaloop/actions/runs/41890301190/job/126785618885)
at feature head `480a77a669b7e1ea1cf3354cddd14fab238da54a` passed the four
warning-denying library suites and formatting/tracing/attribute/parity policies.
The engine unit binary passed 21 tests, with the release measurement explicitly
ignored there and executed separately. Native JACK and bare-metal jobs also pass.
This run includes target `3ca192e1`; the subsequent target refresh through
`820fb239` removes an ignored budget field and fixes physical-port retirement.
The delivered head must receive its own checks after that merge.

The engine proofs use actual allocator-guarded rendering, with independent audio,
raw packet and state expectations across host partitions 1/7/31/128. They cover
stopped capture, overlapping cuts, identical admitted clocks, private wrapping
edits, delayed fair preparation, publication, stale writers/generations, output
recovery, recording conflicts, saturated results, faults and complete teardown.
The persistence sequence retains original, intermediate and reedited exports
through session/exact/WAV round trips, loaded indexed editing, family selection
and whole-configuration replacement. Storage fixtures separately cover density,
page boundaries, long loaded media, u32 crossings, timestamp exhaustion and quotas.

## Reproducible release measurements

[Raw artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/41890301190/artifacts/13353826518)
`shared-fixed-41890301190-1` contains 20 callback CSV files, summary, runner and
Rust toolchain metadata, configuration-source hashes and tested source
`f39c723437ad26d15b9101bd39b073d4eca3e775` (the PR merge checkout for feature head
`480a77a6`). Artifact SHA-256:
`aa3346bf85d38bcaa38a17a93c7c5036df0e64dc818802059ad3007758305e5b`.
GitHub retains it for 14 days; the summary is also printed in the linked job log.

The focused CI job runs `fixed_shared_callback_measurements` in release mode.
It measures the complete guarded `Render::process` call, including all core ticks
and callback-end maintenance. Input generation and controller collection are
outside that interval. The workload uses 48 kHz, maximum host size 128, one stereo
input history, two playing loops, two candidate slots, four rounds of private
audio/MIDI edits and publication, retained exports, and admitted 64-event/refused
65-event MIDI ticks with truthful recovery. This is a finite workload, not an
exhaustive timing bound over every admitted topology or device.

Across 71,965 callbacks, 616 exceeded their individual `frames / 48000` deadline.
The table gives **maximum microseconds / miss count**. Nominal partitions may end
with shorter callbacks; each raw row retains its actual frame count and deadline.
Means and p50/p95/p99 are retained in the artifact and job log.

| Q | Partition 1 | Partition 7 | Partition 31 | Partition 128 |
| --- | --- | --- | --- | --- |
| 1 | 49.844 / 140 | 168.899 / 11 | 535.540 / 0 | 2050.735 / 0 |
| 16 | 36.745 / 102 | 44.255 / 0 | 79.898 / 0 | 160.276 / 0 |
| 32 | 43.885 / 94 | 49.853 / 0 | 73.468 / 0 | 102.812 / 0 |
| 64 | 50.344 / 101 | 48.051 / 0 | 50.024 / 0 | 76.653 / 0 |
| 128 | 56.013 / 165 | 55.402 / 3 | 71.045 / 0 | 61.951 / 0 |

No miss occurred in the 128-frame cases (2.667 ms deadline). Very short callbacks
are not established as deadline-safe. There were 2,844 MIDI overflow diagnostics
in Q=1/partition=128, and zero in the other cases. Two rapidly wrapping MIDI loops
can exhaust the unchanged output event/byte limits. This is reported rather than
hidden by enlarging output capacity. Dedicated functional fixtures check overflow
cleanup/recovery and retained media. None of these observations qualifies a
physical device or guarantees lossless output beyond prepared output limits.

## Prepared capacity and bounded work

For this configuration, coordinator storage ranges from 21,416 to 23,448 bytes;
store metadata/state ranges from 8,019,856 to 8,257,488 bytes across Q. MIDI state
projections account for 5,080,320 bytes of that metadata. Payload reservations are
2,359,296 bytes within the ordinary 3,145,728-byte arena. The figures exclude
allocator overhead and common engine/graph/resource metadata; reservations are
not additional arena allocations. Full MIDI checkpoint projections dominate this
prepared configuration, even before packets arrive.

Observed callback maxima are 128 ticks, 3,617 render operations, 1,167,944
render copied bytes (including state work), and 164,825 render validation steps.
These are observations, not independent render quotas. Required work is bounded
by host size/Q, prepared sources, input/packet limits, tree depth and the ordinary
version-table scan at publication. Optional MIDI progress is separately bounded
at eight steps and at most 16 candidate scans per callback; dedicated multi-tick
fixtures check fairness and cancellation with both slots active. Required history,
playback and ready publication also pass with background copy credits exhausted.

Observed callback-end pool minima are 12,106 free physical pages and 32 free
ordinary version/snapshot slots (40 prepared). Free physical pages include
reserved pages; that count is not unreserved admission capacity. Raw CSV records
reservations and retirement separately. These samples occur after maintenance
and do not claim intra-callback high-water marks. Static prepared reservations,
owner/transaction quotas and the maximum-host multi-tick workload supply the
storage bound; separate exhaustion fixtures verify refusal before publication.
Every measured configuration completes teardown with all ordinary versions,
snapshots, capture/index credits, shared owners/reservations and retirements
recovered, and no work-counter overflow.
