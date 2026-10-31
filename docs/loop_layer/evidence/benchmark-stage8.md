# Stage 8 — guarded native loop measurements

## Implementation and verification scope

Base commit: `320053f0` on `feat/loop-execution-layer`, with the benchmark/example/
helper tests and affected guides added during the recorded gates. This is a dirty-
implementation measurement, not a claim that the base commit contains these new
files. The archive supplies their reproducible source alongside the base patch;
`accepted-measurements/result.json` records scoped source hashes before/after the
run and both actual binary hashes. Production Rust, manifests, locks and library
dependencies are unchanged from the measured-cost implementation in `802833f4`.
Unrelated `.pi/` and public-book work are excluded from this milestone.

Deliverables:

- `scripts/benchmark_loop_engine.py`: fresh debug/release builds, six specific
  Rust heap canaries, guarded release trials, raw/summary agreement, source/binary
  freshness checks, owning-credit/work/storage checks and partial-timeout logs.
  Existing report directories are refused; only a complete trial set writes a
  passing aggregate. Deadline misses are never a success threshold.
- `src/rust/shoop_loop_engine/examples/benchmark_loop.rs`: actual facade/resolver/
  indexed media, separate stereo/MIDI FX/capture consumers, optional NoPlanner
  baseline and preallocated/pre-touched timing records. Full scalar sample/packet/
  immutable-read checks and exact seek/media cut checks remain outside timing.
- Eleven synthetic Python verifier tests, including missing/duplicate/truncated/
  corrupted data, false heap witnesses, absent progress/cascades, credit overflow/
  unreturned owners and timeout-output preservation. These are not native timing
  executions or interpreter evidence.
- The new [performance guide](../../../src/rust/shoop_loop_engine/docs/performance.md),
  README navigation/current absence statements, generic host-boundaries reference
  and historical-runtime crosslink are maintained with the feature. Historical
  measurements are not relabeled.

## Actual measured campaign

```sh
# Local Cargo/Python payloads use nix develop --command on this machine.
RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings" \
python3 scripts/benchmark_loop_engine.py --iterations 1024 --trials 3 \
  --report-dir target/loop-layer-benchmark-stage8/accepted-measurements
```

All14 cases pass across three separate release processes: **43,008 measured
callbacks**,128 frames/callback at48kHz, plus128 full checked warmup callbacks per
case/trial. Both debug/release builds' allocation/reallocation/deallocation canaries
fail through the specific Rust guard (six deliberate failures, not successful
render sessions). The14-case debug smoke additionally passes at eight measured
callbacks/case, including full stopped page/version/snapshot/owning-buffer recovery.

The continuous one-frame case retains all128 playing frames. A separate burst
case schedules four Stops at127 and only a root Play at the next callback's0;
independent stereo/MIDI checks and explicit semantic counts verify four same-frame
starts in every measured callback (**12,288 starts** total),127 playing frames and
exact final-sample silence/cleanup. This is chosen test input, not an engine
interaction quantum or a reduced continuous workload.

Host: Linux7.0.3/glibc2.42, AMD Ryzen AI7 350/Radeon860M,16 logical CPUs. No affinity,
frequency or real-time scheduler control was applied. Rustc1.97.1/cargo1.97.1,
warning-denied flags and Cargo's unoverridden release settings are recorded in the
raw report. Shared-machine scheduling/cache conditions apply. There is no old/new
speedup comparison, cold-cache claim, browser/device throughput or deadline proof.
The 2,666,666ns nominal deadline is descriptive.

| Case | p50 range, ns | worst p99, ns | maximum, ns | misses / 3072 |
| --- | ---: | ---: | ---: | ---: |
| `cascade-bursts` | 2,731,297–2,744,913 | 5,325,657 | 6,402,052 | 3,072 / 3072 |
| `no-planner-idle` | 70–70 | 81 | 111 | 0 / 3072 |
| `loop-idle` | 240–240 | 261 | 5,400 | 0 / 3072 |
| `audio-1-257` | 2,545–3,226 | 3,887 | 12,864 | 0 / 3072 |
| `audio-4-257` | 9,137–11,061 | 14,527 | 29,756 | 0 / 3072 |
| `audio-16-257` | 37,421–40,927 | 50,856 | 60,594 | 0 / 3072 |
| `audio-4-4` | 67,908–72,757 | 84,909 | 103,334 | 0 / 3072 |
| `midi-1-4` | 177,814–180,409 | 268,054 | 279,165 | 0 / 3072 |
| `midi-4-4` | 683,736–698,474 | 896,757 | 1,080,382 | 0 / 3072 |
| `cascade-4-1` | 2,744,282–2,768,737 | 5,774,140 | 6,267,268 | 3,072 / 3072 |
| `nested-regular` | 140,494–154,311 | 165,431 | 184,406 | 0 / 3072 |
| `seek-replace` | 3,106–3,296 | 5,751 | 6,402 | 0 / 3072 |
| `fx-capture` | 3,527–4,198 | 5,570 | 11,922 | 0 / 3072 |
| `upload-export` | 2,976–3,667 | 5,530 | 7,524 | 0 / 3072 |

All **6144 dense/burst misses remain present**. Valid costly music may xrun. The
separate idle engines are not a paired statistical overhead/speedup experiment.
Percentiles use each trial's sorted raw rows; the table retains ranges/maxima
rather than replacing all trials by the fastest observation.

Every CSV row also records intervals, semantic-event counts (including Wrap/Pulse),
node/edge visits, required versus maintenance bytes/steps, used/reserved pages,
free version/snapshot samples and pending domain/resource requests. Actual prepared
limits/63 domain credits/64 command slots are reported. Pending request peaks are5
for repeated bursts,2 for seek/capture, and1 domain/2 resource for upload/export;
every drain must return both counts to0. Reusable read/write owners and all
page/version/snapshot pools are explicitly recovered after rendering stops.

Memory metrics distinguish the fixed1MiB resource arena, callback-boundary pool
samples and Linux's OS-reported process-lifetime `VmHWM`. The latter includes prior
cases/off-render verification; neither it nor sampled pool maxima certifies an
isolated callback's unsampled peak or total prepared-storage accounting. Other
platforms can report RSS unavailable. Actual worklet fixed-memory evidence remains
separate from this native campaign.

## Other fresh gates and preserved failures

`accepted-gates.log` records:

- Warning-denied locked workspace build/formatting, library rustdoc and loop doctests.
- **71 Python helper/parser/verifier tests** (eleven benchmark plus sixty retained).
- Test-macro, runtime/loop dependency boundaries, Wasm test-dependency policy and
  closed tracing policy (207 modules; no new production module).
- **2143 workspace nextest passes/four existing skips**, warm33.834s, with pinned
  nextest0.9.116 and the application-backend feature/allowed missing real backends.
- The actual benchmark builds, six guarded failure canaries and all raw trials.

Initial attempts are retained, not counted as complete passes: an unused import
failed the warning-denied build; undersized prepared MIDI edge capacity refused
before rendering; same-frame initial Play/Seek arbitration correctly suppressed the
start; and a caller incorrectly tried to retire a borrowed, automatically retired
capture ID. The example's preparation/submission/ownership use was corrected,
without changing library behavior, existing musical tests, Miri flags or watchdogs.
Earlier13-case prototype reports remain source-scoped prototypes, not relabeled
as the final14-case campaign.

No new production behavior or interpreted test workload is introduced here, so
there is **no per-edit/native-benchmark Miri invocation**. Fresh package-Wasm,
standalone host/browser and generic-runtime host runs are not claimed by this
milestone; applicable final source-matched gates remain mandatory.

## Measured-cost interpreter recovery failed

Focused unprofiled job `20261028T113106-86abdbc4e8`, frozen at
`802833f492df0b7e8f5a8f1c955c16ea102c2cdd`, ended failed/exit1 at
2026-10-28T11:46:09.194427Z after902.0025350259966s. The same complete exact dense
restart/stall seed0 test exceeded the unchanged900-second command watchdog, with
**zero completed executions** and no passing raw summary. Full assertions, seven
replay runs/partitions, allocator/destructor guards and safety flags were retained.
All866 frozen source entries, copied supervisor and original job hash still match;
both recorded process start-tick identities are dead. Collection refuses
`Cannot collect passing evidence: failed`. `failed-measured-retry.json` and copied
raw job state/logs preserve this terminal proof.

Native correctness/performance does not establish interpreter recovery. Censored
900-second attempts prove neither optimization speedup nor slowdown. A concrete
residual-cost diagnosis/relevant fix is still needed; no routine duplicate run is
launched here. The complete current145-command/298-execution selection and final
CI artifacts/review remain open. Stage7 stays7/8; these two benchmark rows make
Stage8 **3/8**, not complete. Stages9–10 and final independent acceptance stay open.
Nothing is pushed or merged by this milestone.

## Artifacts

Archive: `benchmark-stage8-logs.tar.gz`,1,281,380 bytes, SHA256
`cb9afd2b5652248dd904445666eb9f5a61675ee892ccff77be20d1983f91fff6`.
It contains current and earlier source-labeled reports/raw CSVs, canary/build/gate/
failed-attempt logs, Miri terminal-failure proof/copies and the reproducible new
implementation files/base patch. The large historical diagnostic profile is not
copied into it. This document's final archive hash is not embedded inside its own
archive, avoiding a self-referential source claim.
