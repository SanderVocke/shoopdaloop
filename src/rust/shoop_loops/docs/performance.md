# Measure native loop callbacks

The native [`benchmark_loop` example](../examples/benchmark_loop.rs) runs the actual
resolver, media ledger, playback/capture consumers and a stereo half-gain FX node.
It is not a device adapter, browser benchmark or deadline guarantee.

## 1. Run the checked measurement driver

From the repository root, in the project's build environment:

```sh
python3 scripts/benchmark_loops.py --iterations 4096 --trials 3
```

The driver builds both profiles and requires allocation, reallocation and
deallocation canaries to fail through the specific Rust heap guard in **each**
profile. It then measures release callbacks. Use `--report-dir PATH` for separate
reports. Linux `--cpu N` optionally pins each trial process to an available CPU;
without it, affinity is not changed. There is no frequency or scheduler-priority
control. The report records that limitation and the available CPUs/hardware.
Choose a fresh report directory: the driver refuses to overwrite one. Commands
have a600-second watchdog; timeout preserves partial output and never writes a
passing aggregate. This is separate from the unchanged900-second Miri watchdog.

The Linux release CI cell runs the same driver with `--iterations 1024 --trials 3`.
Each run/attempt uses a fresh directory under the runner's temporary directory.
The `loops-bench-<run-id>-<attempt>` artifact retains the complete report,
raw CSVs and logs even after failure; a missing report directory fails artifact
upload. Inspect the actual run's artifact and source identity before attributing
results to a PR head. A local invocation is not remote CI acceptance.

For a short functionality smoke, not representative timing:

```sh
cargo run --locked -p shoop_loops --example benchmark_loop -- \
  --iterations 8 --raw-dir target/loop-benchmark-smoke
```

The measurement driver requires at least 1024 measured callbacks per case. Each
case also runs 128 full checked warmup callbacks. Iteration limits bound the
**experiment duration**, never internal musical progression or required transitions.

## 2. Understand the workloads

Every measured callback is 128 frames at 48 kHz. The fourteen cases include:

- Separate empty NoPlanner and empty loop-planner engines.
- One/four/sixteen stereo loops of257 frames, plus four loops of four frames.
- One/four four-frame stereo/MIDI loops. Indexed initial projections hold a note;
  every wrap performs full cleanup/restoration, including all sixteen channels'
  neutral pitch/hold state. These clips deliberately contain no later original
  MIDI records; this measures indexed-state restoration, not arbitrary packet mixes.
- Four one-frame stereo/MIDI loops, started by a same-frame trigger cascade. All
  continue wrapping after startup; no subdivision/propagation quota truncates them.
- A separate repeated-cascade experiment. Four timestamped Stops at offset127
  leave every loop stopped; the next callback supplies only a root Play at offset0.
  The declared trigger chain must produce **four same-frame starts in every
  measured callback**, then all127 playing frames and the final silent sample/
  exact MIDI cleanup are checked. The continuous workload above is not shortened.
  These Stops are experimental inputs, not a runtime interaction quantum.
- Two nested regular composites clocked by a separate one-frame primitive. This
  is actual nested clock progression, not an unsynchronized synthetic fallback.
- Alternating indexed seeks and prepared selection between two immutable stereo
  revisions. They occur on separate callback cuts so same-frame direct-request
  precedence does not suppress the intended edit.
- Alternating 128-frame recording/playback through the separate half-gain FX and
  capture consumers, with exact expected live/captured/playback samples. The
  facade retires completed capture metadata; callers do not retire borrowed IDs.
- Independent 64-byte uploads and immutable loop-export reads alongside playback.
  Uploads use their own raw resource, not an unvalidated live media replacement.

Preparation uses finite, checked capacities and writes all storage before timing.
The edge/port capacities include the MIDI routes. MIDI capacity is derived from
these known projections/workloads, not a promise of unbounded MIDI output.

## 3. Read results without hiding tails

Default reports are under `target/loops-bench/`:

- `result.json`: source commit/dirty state and scoped source hashes, toolchain,
  release settings, actual prepared capacities/credits, platform, canary results,
  binary hashes and every trial's summaries. Relevant source/binary changes during
  measurement refuse the aggregate.
- `trial-N-raw/CASE.csv`: **every** measured callback's elapsed nanoseconds,
  intervals, graph node/edge visits, semantic event count, render versus maintenance
  bytes/validation steps, used/reserved pages, free version/snapshot samples,
  pending domain/resource requests and repeated-cascade start counts.
- Build, deliberate-failure and per-trial logs, including native JSON summaries.

The Python verifier recomputes percentiles/maxima, deadline misses and work totals
from raw rows. Missing/duplicate cases, truncated/reordered callbacks, changed
summaries, broken guards, maintenance/storage/credit violations, absent upload/
export progress and unmeasured cascades fail. All submitted request counters must
return to zero after every drain. Slow callbacks do **not** fail. The theoretical
128/48000-second deadline is 2,666,666ns; every miss remains visible.

Timing brackets the guarded core processing, including the observer, output-buffer
reset and guard overhead. It includes OS preemption. Source preparation, submission,
full scalar sample/packet/read comparisons, reliable completion destruction and
file I/O occur outside that bracket. MIDI expectations are constructed independently
from the known initial projection/order, not by calling restoration as an oracle.
Seek/replacement state queries additionally check the exact requested position,
selected media and cut frame. All cases explicitly clean up retained leases/
resources and recover every page/version/snapshot and reusable owning buffer after
measurement. The semantic-event count includes Wrap/Pulse as well as transitions.

Component memory figures are the fixed 1MiB **resource arena** and pool usage
sampled at callback boundaries. They exclude other prepared buffers, metadata,
measurement storage and unsampled pool peaks. Linux additionally reports the
OS's process-lifetime `VmHWM` peak RSS, including prior cases and off-render work;
this is not a validated isolated-case/callback memory peak and is unavailable on
other platforms. Credit peaks count submitted domain/resource requests awaiting
checked returns just before rendering, against actual prepared limits; they are
not all internal lease counts. Frames/intervals/semantic events and graph visits
distinguish expensive music from idle overhead; these are not work caps.
The separate idle engines are not a paired statistical speedup experiment.

These native measurements do not certify cold-cache behavior, xrun-free CI/device
operation, ordinary-browser scheduling or legacy-engine performance. Keep actual
worklet correctness/memory/failure evidence distinct from native timing reports.
