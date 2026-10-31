# Carla performance regression: laptop reproduction of #890

## Result

**Partial reproduction of the anecdote, and a confirmed scheduling failure in master.**

A long, unconstrained comparison did **not** show a general increase in heavy-plugin processing time. However, later unconstrained runs intermittently produced whole-buffer stalls and xruns in master's internal Carla paths while 0.2.0 remained healthy. Deliberately co-locating the audio callback and Carla workers on one CPU makes the failure deterministic. Raising only the benchmark workers' scheduling priorities eliminates it without changing the plugin, routing, buffer size, or CPU placement.

The demonstrated failure is a **realtime callback synchronously spin-waiting for lower-priority Carla workers**. This is a strong candidate for the reporter's scheduler-sensitive dropouts, especially on a two-core laptop. It is not proof that all of their higher Carla UI DSP readings have the same cause.

The original measurements below used unmodified application source; priority and affinity changes were confined to temporary benchmark processes. Findings were subsequently [posted on #890](https://github.com/SanderVocke/shoopdaloop/issues/890#issuecomment-6525713902). Implementation and follow-up validation are separate from these historical measurements.

## Report being reproduced

Read using `gh issue view --repo SanderVocke/shoopdaloop`, including comments:

- [#890: Performance regression of Carla plugins since 0.2.0](https://github.com/SanderVocke/shoopdaloop/issues/890)
- [#880: original discussion](https://github.com/SanderVocke/shoopdaloop/issues/880)

The reporter used an i7-7600U Linux laptop, Kubuntu 24.04/PipeWire, 48 kHz / 1024 frames, a monitored live-FX track, a click track, and Voyage Voyage. Their approximate Carla gauge readings were:

| Configuration | Reported DSP |
|---|---:|
| 0.2.0 / external Carla | 7–14%, no xruns |
| 0.2.0 / internal Carla | 8–11%, no xruns |
| New version / external Carla | 10–18% |
| New version / internal, in-process | 10–25%, spikes to 50%, some xruns |
| New version / internal, subprocess | 15–35%, spikes to 60%, more xruns |

They confirmed a release build. #890 also describes a small empty-rack monitoring overhead and sensitivity to opening the FX UI.

## Environment and controls

Investigation date: 2026-10-30.

- Host: AMD Ryzen AI 7 350, 8 cores / 16 threads, 61 GiB RAM; AC power; CPU governor, energy preference, and platform profile already set to `performance`. Frequency still varies. This is substantially newer and more parallel than the reporter's dual-core CPU.
- NixOS, Linux 7.0.3 PREEMPT_RT, PipeWire 1.4.9; host physical playback clock, verified at **48,000 Hz / 1,024 frames** throughout the comparisons. Buffer budget: **21.333 ms**.
- Master: `bcf928e165eaf73481727bef43912c6fdca68cc1`, also the remote master at investigation start. Built with `nix develop --command cargo build --release -p shoopdaloop`, with a separate cached target directory. No custom Rust flags.
- Baseline: actual `v0.2.0` Linux x64 **release** portable archive, downloaded with `gh release download`; tag commit `d08aafcb049f6c7a848c97a18b5c3a1456046696`.
- Release archive SHA-256: `8345a107d73326daf78c38a55c2b200412d562a1031f1fd84f1f10e413a40332`.
- To launch that Ubuntu-oriented archive on NixOS, its ELF interpreter was changed to the Nix loader and missing runtime libraries supplied through `LD_LIBRARY_PATH`. The application code was not rebuilt. A preliminary `--quit-after` run crashed during teardown; comparative recording runs stayed alive through capture and were terminated afterwards.
- Same **Carla 2.5.10 Nix package** for legacy LV2 hosting, current native hosting, and the external standalone engine. This deliberately controls for Carla build differences rather than comparing different packaged Carla builds.
- Same **Dragonfly Hall Reverb 3.2.10 LV2** binaries and default settings. Voyage Voyage's download returned a Cloudflare challenge/HTTP 403, so this is an explicitly substituted DSP workload, not an exact plugin/preset reproduction.
- Stereo dry/wet track, monitoring enabled, eight empty loop slots, audio and MIDI ports; empty, one-reverb, and eight-reverb racks. Eight instances run serially in one rack, approximating one heavier plugin without adding more Shoop worker chains.
- Repeatable synthetic stereo signal with a click component. Nonzero output was verified at a separate sink. The sink sends **silence** to the physical playback device: no test tones were sent to speakers. This does not reproduce a separately playing click loop or the reporter's performance input.
- External Carla used the same standalone backend library in continuous-rack/JACK mode; its GUI was not opened.
- No system-wide PipeWire metadata, CPU governor, or persistent scheduling settings were changed.

### Rendering confound removed

The harness sandbox exposes PipeWire sockets but neither `/dev/snd` nor `/dev/dri`. The initial visible master UI under Xvfb/llvmpipe consumed approximately **700% process CPU**, unlike the legacy Qt UI. Those runs showed larger jitter but are **not** a clean normal-desktop comparison.

All comparisons below use Xvfb plus Openbox, with both application windows **minimized**. Master then uses single-digit process CPU for the light workloads. Rendering startup was also limited with `LP_NUM_THREADS=1`. Audio remained active. A GPU-backed visible-UI comparison and repeated FX-window opening remain untested.

### Actual scheduling

The audio callback is a PipeWire `data-loop.0` thread running **SCHED_FIFO priority 83** in both versions. The `pw-Shoop…` threads are ordinary-scheduling control threads, not the DSP callback. An initial inspection of those names alone incorrectly suggested that audio was not realtime; the complete thread and per-thread CPU-time audit corrected this.

Master's `carla-processor-bridge`, and subprocess `carla-worker-realtime`, run as **SCHED_OTHER** by default. Their names do not imply realtime scheduling.

## Measurement method

Use `pw-profiler -J -n N` to collect per-cycle follower and hardware-driver records, independently of either application's gauge:

- Normal completed callback time: `finish - awake`.
- Budget percentage: `100 * callback_time / 21.333 ms`.
- Xruns: differences in cumulative counters during the measured window, recording **both client and hardware-driver counters**.
- Five-second warmup after routing/minimizing. Record the graph, thread policies, application logs, and output RMS; later checks also record per-thread `schedstat` deltas.

These are **wall-clock callback budget percentages, not Carla UI meter samples or whole-process CPU utilization**. External rows below sum the Shoop and Carla node mean budget percentages; they are not an external round-trip latency measurement.

Under an overrun, profiler records can describe a still-awake callback or timestamps from different graph cycles. The CSV preserves the number of non-finished records. Bad-run means/maxima are therefore diagnostic approximations over finished records, not precise all-cycle latency distributions. Xrun counter increments and the separate application trace provide the stronger failure evidence.

## 1. Long, unpinned comparison

Two repetitions per combination, randomized/interleaved with seed 890; 1,200 cycles per run (approximately 25.6 seconds), 30 runs total. Every measured callback finished. **No client or hardware-driver xrun counters increased.**

Mean processing budget percentages, averaging the two runs:

| Configuration | Empty rack | One reverb | Eight reverbs |
|---|---:|---:|---:|
| 0.2.0 / internal | 0.50% | 3.58% | 13.33% |
| 0.2.0 / external, summed nodes | 0.38% | 3.57% | 11.09% |
| Master / internal, in-process | 0.96% | 3.17% | 10.70% |
| Master / internal, subprocess | 1.09% | 3.30% | 11.40% |
| Master / external, summed nodes | 0.87% | 3.05% | 11.34% |

This supports a small empty-track/rack overhead increase, but **not** the reported general heavy-plugin slowdown. Do not interpret the lower master heavy-rack means as proof of an improvement: CPU frequency and scheduling vary, and there are only two independent runs per combination.

The primary legacy fixture retained its default idle mono sync track; the primary master fixture initially did not. The following control aligns that topology. No primary run used a playing click loop.

## 2. Unpinned, matched-sync topology: intermittent stalls

Add an idle mono sync track to master too, without changing the rack or main track. Measure 500 cycles (approximately 10.7 seconds) per run. In the first sequence:

| Configuration | Reverbs | Approx. mean finished-callback budget | Client xruns | Hardware-driver xruns |
|---|---:|---:|---:|---:|
| 0.2.0 / internal | 0 | 0.45% | 0 | 0 |
| Master / in-process | 0 | 0.99% | 0 | 0 |
| Master / subprocess | 0 | 3.26% | 4 | 4 |
| Master / subprocess | 8 | 25.68% | 13 | 11 |
| Master / in-process | 8 | 18.82% | 6 | 6 |
| 0.2.0 / internal | 8 | 17.25% | 0 | 0 |

The three failing master runs had callback completion times around **21.5 ms**, close to one whole buffer deadline, with some cycles still marked awake. The subprocess-heavy capture had only 474 finished records out of 500.

**Adding the sync track is not established as the cause.** Subsequent runs with that same topology were mostly healthy: approximately 10.7–10.9% in both master internal modes, with no xruns. Another in-process repeat did produce one client xrun and a 21.5 ms completion. Both later external controls were healthy.

For the one-xrun in-process repeat, the Carla bridge worker accumulated **23.9 ms of runnable runqueue delay over the measurement**, versus **4.0 ms** in a healthy in-process repeat. The JACK callback itself remained FIFO and accumulated essentially no runqueue wait. This is correlation over the whole interval, not a scheduler trace locating that exact xrun.

## 3. Controlled priority-inversion reproduction

Keep the matched-sync session, minimize the UI, and pin the application audio callback and Carla DSP worker(s) to **logical CPU 8**. The source, silent sink, application actor, and GUI are not pinned. No other CPU or host-process affinity is changed. Measure 200 PipeWire cycles (approximately 4.27 seconds) after warmup.

| Configuration | Worker scheduling | Client xruns | Observed result |
|---|---|---:|---|
| 0.2.0 / internal | Inline on FIFO-83 callback | 0 | Normal output; 9.73% mean budget |
| Master / in-process | Default SCHED_OTHER | 49 | Almost silent output; repeated whole-buffer timeouts |
| Master / subprocess | Default SCHED_OTHER, both workers | 50 | Almost silent output; repeated whole-buffer timeouts |
| Master / in-process, same CPU placement | Bridge FIFO 84 | 0 | Normal output restored; 10.32% mean budget |
| Master / subprocess, same CPU placement | Bridge FIFO 84; child DSP FIFO 85 | 0 | Normal output restored; 10.02% mean budget |

The hardware-driver counters did not increase in this pinned experiment: the client misses processing while the device graph keeps running. Client xrun counts include non-finished profiler records; counting only finished records would report 49 rather than 50 for the failing subprocess case.

The failure is not expensive plugin computation exhausting the buffer. 0.2.0 runs the same DSP comfortably on that CPU, and the same master binary works once the worker can preempt the waiting callback. It is the realtime caller preventing its lower-priority dependency from running. Equal-priority FIFO workers would also require careful consideration when the caller spins; the diagnostic deliberately used strictly increasing priorities along the dependency chain.

**This affinity restriction is a diagnostic stress condition, not the default laptop configuration.** It explains a concrete vulnerability and its sensitivity to placement; it does not turn the 30 healthy default-affinity runs into failures.

## 4. Perfetto confirmation

Use the repository's pinned `scripts/trace_processor` wrapper. Traces and SQL are retained in the artifact directory below. No nonzero error/drop/loss statistics were reported by the inspected trace statistics; the pinned trace's nonzero statistics were informational.

Healthy, unpinned in-process eight-reverb trace, steady window from trace start +7 seconds to trace end −1 second (435 callbacks):

| Zone | Mean |
|---|---:|
| `engine.rt.driver.jack` | 2,373.6 us |
| `engine.rt.fx.bridge_wait` | 2,061.2 us |
| `engine.rt.fx.bridge_worker` | 2,049.4 us |
| `engine.rt.fx.plugin_process` | 2,043.3 us |

That is mostly actual plugin work, with only a small successful handoff overhead. A healthy empty-rack trace measured approximately 3.5 us in native Carla plugin processing versus 375 us for the traced whole callback; its bridge-process zone averaged 28.9 us. Detailed tracing itself adds overhead.

Failing, same-CPU in-process trace, steady window from start +3 seconds to end −1 second (99 callbacks):

- Callback mean: **21,567 us**.
- `engine.rt.fx.bridge_wait` mean: **21,334 us**.
- **No bridge-worker or plugin-process slices in that steady window.**
- Across the trace, `engine.fx.bridge.deadline_misses` reaches **157**; fallback reason reaches **2**, the completion-wait failure path.

This directly observes the callback waiting out its deadline while the worker does not execute, rather than merely attributing a high meter reading to plugin cost.

Relevant implementation at the tested master:

- `src/rust/shoop_engine/src/carla_processor.rs:653`: realtime endpoint submits work, unparks the bridge, and synchronously waits for completion.
- `src/rust/shoop_engine/src/carla_processor.rs:952`: bridge creation spawns an ordinary thread; no audio scheduling negotiation/promotion occurs there.
- `src/rust/shoop_engine/src/carla_shared_memory.rs:354`: completion wait uses a spin loop until completion or deadline.
- `src/rust/shoop_engine/src/carla_subprocess.rs:393`: the subprocess DSP worker is also spawned as an ordinary thread.
- Legacy `v0.2.0:src/backend/internal/lv2/CarlaLV2ProcessingChain.cpp:556`: plugin processing calls `lilv_instance_run` inline.

A production fix should address the synchronous realtime dependency and scheduling hierarchy, not blindly hard-code priorities 84/85. Any promotion must also account for UI/state/control operations currently serviced by these workers and cross-platform audio scheduling. The temporary priority change here is a diagnostic, not an implemented fix.

## Artifacts and rerunning on this laptop

Checked-in per-run/node measurements: [carla-890-results.csv](carla-890-results.csv). It separates the long comparison, intermittent matched-sync controls, later repeats, and pinned/priority controls. Traced measurements and rejected visible-software-rendering measurements are not mixed into that CSV.

Prepared binaries, fixtures, scripts, graphs, logs, raw profiler JSON, thread snapshots, SQL, and traces remain at:

```text
/home/sander/.cache/shoop-issue-890/
```

Important files/directories:

- `env.sh`, `make_fixtures.py`, `audio_fixture.c`, `audio_fixture`, `external_carla.py`, `run_matrix.py`, `analyze.py`, `export_results.py`.
- `fixtures-primary/`: the original master sessions used in the long comparison. Top-level master fixtures now include the matching sync track.
- `matrix-minimized.log`, `runs-minimized/`: 30-run comparison; repetition 0 directories are setup probes and excluded from the CSV.
- `control.log`, `runs-minimized-synced/`: intermittent failing matched-sync sequence.
- `repeat.log`, `repeat-8.log`, `runs-minimized-synced-repeat/`: subsequent controls. An external repeat initially captured 501 rather than 500 cycles and was rejected by an overly strict harness assertion; the later completed repeat is used in the CSV.
- `pinned.log`, `runs-minimized-pinned/`, `runs-minimized-pinned-rt/`: controlled priority/placement test.
- `runs-minimized-trace/5-master-inprocess-{0,8}/traces/0001-application.pftrace`: healthy diagnostic traces.
- `runs-minimized-pinned-trace/10-master-inprocess-8/traces/0001-application.pftrace`: priority-inversion trace.
- `trace-query.sql`, `trace-pinned-query.sql`, `trace-{0,8}-query.txt`, `trace-pinned-final-query.txt`: queries, filters, and outputs.

The GUI close method in the first tracing attempt destroyed the X window instead of sending a normal close request; it panicked during teardown and did not save a trace. The later captures above used the window manager's Alt-F4 path and saved normally.

To rerun using those **already prepared** artifacts (not a portable fresh-machine setup):

```sh
nix develop
source "$HOME/.cache/shoop-issue-890/env.sh"
Xvfb :98 -screen 0 1280x800x24 -nolisten tcp > "$BENCH/xvfb.log" 2>&1 &
xvfb_pid=$!
openbox > "$BENCH/openbox.log" 2>&1 &
wm_pid=$!
MATCH_SYNC=1 python3 "$BENCH/make_fixtures.py"

python3 - <<'PY'
import os, sys
sys.path.insert(0, os.environ['BENCH'])
from run_matrix import run

os.environ['PIN_AUDIO_CPU'] = '8'
os.environ['VARIANT'] = 'minimized-review-pinned'
run('legacy-internal', 8, 11, 200)
for mode in ('master-inprocess', 'master-subprocess'):
    os.environ.pop('RT_WORKERS', None)
    os.environ['VARIANT'] = 'minimized-review-pinned'
    run(mode, 8, 11, 200)
    os.environ['RT_WORKERS'] = '1'
    os.environ['VARIANT'] = 'minimized-review-pinned-rt'
    run(mode, 8, 11, 200)
PY

kill "$wm_pid" "$xvfb_pid"
```

Omit `PIN_AUDIO_CPU` and `RT_WORKERS` for the unconstrained comparison; use a `VARIANT` starting with `minimized` to retain the rendering control. The test helper creates and removes its own JACK clients and connections. The benchmark X server/window manager were stopped after this investigation.

Example trace query in the Nix shell (the temporary HOME puts the wrapper's prebuilt cache in a writable directory):

```sh
HOME="$BENCH/tool-home" scripts/trace_processor \
  --query-file "$BENCH/trace-pinned-query.sql" \
  "$BENCH/runs-minimized-pinned-trace/10-master-inprocess-8/traces/0001-application.pftrace"
```

## Remaining limits

- Exact Voyage Voyage binary, preset, and input performance were not available.
- No GPU-backed visible-UI or repeated FX-editor-opening reproduction was performed. The software-rendering artifact must not be confused with normal laptop behavior.
- The intermittent default-affinity failure does not have a reliable natural trigger yet; sync-track presence alone is insufficient. Normal scheduling/placement is a strong candidate, supported by the controlled experiment, but it is not an exact attribution of every unpinned xrun.
- The investigation did not bisect the regression or establish that Carla's own DSP gauge measures this handoff delay. The report's main heavy-plugin regression remains workload/scheduler-dependent rather than a universal increase in DSP computation.

## Narrow implementation

The production change replaces the two shared-memory completion spin loops with
sleeping notifications. Linux uses process-shared futexes, without the private
flag. macOS and Windows use preopened loopback UDP sockets. Notifications are
only wakeup hints: slot state, sequence, and generation remain authoritative.
The private protocol version is incremented for the completion endpoint stored
in the reserved file header; the 64-byte slot header is unchanged.

The observed failure involves both priorities and synchronization: a FIFO audio
callback monopolizes a CPU while waiting for an ordinary-priority worker. The
higher-priority-worker benchmark demonstrated that dependency, but promoting
workers is not required to break it. Sleeping the caller lets the lower-priority
worker execute. The fix therefore leaves worker priorities alone and introduces
no Linux FIFO assignments, Windows MMCSS registration, or Mach policy changes.
This avoids a Linux-specific priority assumption and does not create elevated
plugin threads requiring new overrun enforcement.

Existing per-bridge/per-subprocess timeouts, completed-result acceptance,
failure output, and recovery semantics are preserved. There is no new graph-wide
deadline, plugin budget, watchdog thread, startup scheduling probe, or control/DSP
exclusion framework. Ordinary workers can still encounter CPU contention;
platform-specific realtime QoS is a separate concern. OS timer granularity and
scheduling still affect timeout precision. This is not plugin sandboxing or a
universal DSP-computation speedup.

## Narrow-version validation

- A same-CPU regression uses a real FIFO caller and an ordinary bridge worker.
  It fails with silent output under the original spin loop and passes with the
  sleeping wait, without opting out of missing realtime facilities. The test
  restores the caller's original scheduling and affinity.
- Delayed audio/MIDI completions exercise both copy paths, including allocation-
  free MIDI reuse. Portable UDP tests also run on Linux and exercise stale
  wakeups and invalid endpoints. A futex race test covers completion before wait.
- Existing worker-entry tests pass separately without the optional-backend flag.
  Timeout fixtures use 500 ms fake work against a 50 ms timeout to avoid assuming
  sub-millisecond timer precision on Windows. These are test settings, not new
  production limits.
- The warning-denying workspace build, four native feature-isolation checks,
  formatting/test policy, and closed tracing inventory pass. The full native
  suite passes **1,814 tests (4 skipped)** with the documented optional-backend
  policy. The LLVM-instrumented suite passes the same tests; local changed-line
  coverage is approximately 88.8%, without changing the 80% CI threshold or
  granting new CI privileges.
- The actual completion helper cross-compiles with warnings denied for macOS
  ARM64 and Windows x64. This is not a full application cross-build or native
  runtime validation. Remote checks must validate the latest narrow version.

The matched live workload was repeated with ordinary-priority workers and no
external priority override. Thread snapshots verify every bridge/child DSP
worker remains `SCHED_OTHER`, priority 0. Carla, Dragonfly, 48 kHz/1,024 frames,
and minimized Xvfb/Openbox controls match the original investigation.

| Placement | Hosting | Reverbs | Mean callback period used | Client/device xruns |
|---|---|---:|---:|---:|
| CPU 8 | In-process | 0 | 1.02% | 0/0 |
| CPU 8 | In-process | 8 | 10.45% | 0/0 |
| CPU 8 | Subprocess | 0 | 1.13% | 0/0 |
| CPU 8 | Subprocess | 8 | 10.53% | 0/0 |
| Default affinity | In-process | 0 | 1.09% | 0/0 |
| Default affinity | In-process | 8 | 11.15% | 0/0 |
| Default affinity | Subprocess | 0 | 1.27% | 0/0 |
| Default affinity | Subprocess | 8 | 11.60% | 0/0 |

Pinned captures contain 200 cycles each; default-affinity captures contain 1,200.
Every capture has normal output (RMS 0.043–0.046) and zero all-state client,
device, source, and sink xrun deltas. These single follow-ups establish removal
of the deterministic starvation failure, not a universal DSP speedup or a new
statistical baseline. Results are in `carla-890-narrow-results.csv`; raw captures
are retained under `runs-minimized-narrow-{pinned,unpinned}/` in the artifact
directory. The original pinned runs had near-silence and 49/50 client xruns.

## Superseded prototype evidence

An earlier, broader prototype added scheduling promotion, overrun monitors,
callback-wide deadlines, and control/DSP exclusion. Those changes were removed
on review to keep this a targeted performance fix. Its eight live captures
remain in `carla-890-fix-results.csv` and
`runs-minimized-fix-v3-{pinned,unpinned}/` as historical evidence, not measurements
of the narrowed implementation. Its scheduling/overrun tests and 1,827-test
validation counts do not describe the current patch. The original investigation
CSV and traces are likewise retained unchanged.
