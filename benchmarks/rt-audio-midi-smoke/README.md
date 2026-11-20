# Shared RT benchmark: audio, MIDI and combined workloads

[Open the five-page PDF](report.pdf) · [Download raw results](results.tar.gz)

Measured source: `73cf1d7581f039654c841a2d20b3ab7360b7db59`; upstream baseline `cc410f27`.
Fixture revision 4 uses 64 loops, Q=callback=32 and 48 kHz. Audio is stereo with
32-frame chunks and 32-chunk blocks (1,024 frames/block). MIDI uses 256-record
blocks. All three RTL media profiles use the same connected hardware capacities,
with audio and MIDI AXI sharing one DRAMsim3 controller. The memory profile is
upstream's provisional Z-turn V2 DDR3-1066 x32 configuration.

A subsequent cleanup reuses upstream's passive plan decoder for snapshots. An
audio lifecycle replay preserved all 298 fabric/traffic/stage measurements,
fixture hash, command events and coverage. Snapshot observation advances no
clocks; this bundle retains the measured source and binary fingerprint above.

This is the **smoke preset**, with two lifecycle iterations and one repetition
per media/pattern/backend. The standard entry point retains eight lifecycle
iterations and three independent CPU/reference repetitions. All 27 requested run
slots are accounted for: 15 completed, 12 explicitly unsupported, none failed.
The bundle retains 8,863 callbacks and validates 851,968 MIDI output
messages exactly, including bytes, timestamps, counts and ordering. Stereo
samples, same-frame command groups, block growth, wrapping, retakes and planned
transitions also passed. Fixture hashes match across available backends.

Capture took about 54 minutes with three RTL workers, a prebuilt binary and a warm
model cache on the recorded host (four CPU cores of quota). Regenerating the
five-page report from saved data took 2.4 seconds.

RTL times cover the complete callback and use **fabric cycles / 150** for µs.
Simulator wall time is separate. The 666.67 µs quantum is a comparison budget;
execution is unpaced and has no driver deadline. Observed maxima describe this
fixture. Overlapping SP/JP/stall counters must not be added.
AXI reads/writes count transactions; response backpressure measures ready responses
waiting for RTL, rather than JP time waiting for memory.

| RTL media / pattern | N | Median µs | p95 µs | p99 µs | Maximum cycles | Maximum µs | Over 666.67 µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| audio / lifecycle | 298 | 341.33 | 412.83 | 480.63 | 113,424 | 756.16 | 1 |
| audio / mixed | 232 | 341.73 | 411.55 | 556.06 | 92,688 | 617.92 | 0 |
| audio-midi / lifecycle | 298 | 628.45 | 13,275.41 | 13,348.68 | 2,031,651 | 13,544.34 | 136 |
| audio-midi / mixed | 232 | 769.53 | 6,380.38 | 13,276.88 | 2,031,651 | 13,544.34 | 117 |
| midi / lifecycle | 298 | 244.43 | 12,986.42 | 13,056.48 | 1,989,492 | 13,263.28 | 136 |
| midi / mixed | 232 | 436.33 | 6,044.44 | 12,987.95 | 1,989,492 | 13,263.28 | 113 |

| Maximum callback | Callback / frame | Trigger | MIDI input / output events |
| --- | ---: | --- | ---: |
| audio / lifecycle | 133 / 4,256 | reuse-record-all | 0 / 0 |
| audio / mixed | 0 / 0 | cold-record-all | 0 / 0 |
| audio-midi / lifecycle | 33 / 1,056 | play-all | 16 / 2560 |
| audio-midi / mixed | 33 / 1,056 | play-all | 16 / 2560 |
| midi / lifecycle | 33 / 1,056 | play-all | 16 / 2560 |
| midi / mixed | 33 / 1,056 | play-all | 16 / 2560 |

MIDI input has 40-message bursts, seven 16-message quanta and eight silent quanta
per cycle. Synchronized playback can produce 2,560 messages on one merged output;
63 balanced merges copy intermediate lists in DDR. The PDF and summary distinguish
quiet playback from dense playback (>256 output events), both excluding recording.
Upstream's [sparse playback measurements](../../docs/fpga/midi-data-jobs-benchmarks.md)
normally emit four or five messages per quantum, assume 100 MHz and use independent
ideal/fixed-delay ports. These workloads and memory assumptions differ.

At the MIDI-only peak, JP is active for 13,176.76 µs and the MIDI port performs
84,704 reads and 72,768 writes. Quiet MIDI-only playback takes 79.43 µs; the
corresponding combined lifecycle population has a 371.53 µs median. Those
baselines and the dense tails describe different amounts of event work.

The native CPU immediate endpoint currently rejects MIDI loop transitions;
MIDI-only and combined CPU cases are reported unsupported. Its authored transport
uses another control path. FPGA history/grabs and MIDI history/grabs remain
unsupported. Native audio history grabs run after the one-second ring wraps.
CPU/reference timing describes host execution and includes host variability.

The PDF devotes its first three pages to FPGA timing, stress populations and peak
diagnostics. The remaining pages cover host results and correctness/provenance.
Full callback/event data and all per-repeat statistics are retained in the archive.

Regenerate without building a backend:

```sh
python3 -m pip install 'matplotlib>=3.9,<4'
mkdir -p work/rt-midi-published
tar -xzf benchmarks/rt-audio-midi-smoke/results.tar.gz -C work/rt-midi-published
python3 scripts/benchmark_rt.py --report-only --output work/rt-midi-published
```

Reproduce this shorter campaign after preparing the simulation prerequisites in
[the benchmark documentation](../../src/rust/shoop_rt_benchmark/README.md):

```sh
python3 scripts/benchmark_rt.py --preset smoke --rtl-jobs 3 --output work/rt-midi-smoke
```

Omit `--preset smoke` for the longer standard campaign. Host timing runs finish
serially before RTL processes start. The historical
[audio-only report](../rt-standard-32x32/README.md) retains its original source,
memory profile and larger repetition/iteration counts.

Archive SHA256: `ec0aff6f6551eace38e7f45143e874e2384d749e64af6f21ad2ef05752a6da2b`.
