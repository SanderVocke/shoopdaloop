# 64-loop MIDI data-job execution evidence

See [the report](../../midi-data-jobs-benchmarks.md) for interpretation. Each run
loads 64 loops with 30-second content, then checks one startup quantum, sixteen
warmup quanta, and sixteen steady quanta. Audio is stereo per loop; MIDI is 100
controller changes/second per loop, staggered across eight destinations. Audio
chunks and processing quanta are both 32 frames at 48 kHz.

- `mixed-0.json`, `mixed-7.json`: audio and MIDI, ideal/delayed memory.
- `audio-0.json`, `audio-7.json`: audio only, ideal/delayed memory.
- `midi-0.json`, `midi-7.json`: MIDI only, ideal/delayed memory.
- `quanta.csv`: all measured startup, warmup, and steady quanta, including AXI
  read/write and native request counts for each memory port.
- `summary.json`: steady median/range, startup, message/job counts, program sizes.
- `provenance.json`, `sources.sha256`, `artifacts.sha256`: source/tool identity and
  integrity of the selected artifacts.
- `driver-equivalence.json`: identical complete MIDI results from debug and
  optimized host drivers; clock counts do not depend on host execution speed.
- `previous-stateless.json`: the prior state-free service's recorded quantum
  timings and original file hashes, with instruction-profile counters omitted.
- `summarize.py`, `plot.py`, `quantum-times.svg`: aggregation and figure source.

Run `python3 summarize.py`, then `python3 plot.py` with Matplotlib installed to
regenerate the summary, CSV, and chart. To recreate the raw runs, use the example
commands in the report. Verilator 5.032 is required. Simulation compilation and
the large audio-content load can take much longer than timed playback itself.

The seven-clock service model is the same controlled delayed-memory model used
previously. Both independent 64-bit AXI ports retain their protocol overhead even
with zero service delay. Physical DDR arbitration, row/refresh behavior, and board
clock timing are not established by these measurements. Times are microseconds
at an **assumed** 100 MHz. The existing physical timing failures remain relevant.
