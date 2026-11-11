# AXI sample memory

The implementation follows [PLAN_DDR3.md](../../PLAN_DDR3.md). Baseline:
`8f64c32d` on `proto/fpga-rust-reference`, existing PR #969. The existing cores
retain their word port. Integrated tops put the common synthesizable AXI master
between that port and external memory. No accesses bypass AXI in these tops.

## Endpoint contract

The endpoint accepts a command on `command_valid && command_ready`: byte address,
write flag, AXI SIZE (log2 bytes), and LEN (beats minus one). Supported commands
are naturally aligned INCR transfers of 1–16 beats, at most the configured bus
width, contained in one 4 KiB page. The data bus is 32 or 64 bits; address width
is 32 bits. IDs are fixed at zero. One command remains outstanding until the
last read beat or write completion is consumed.

Write beats use `write_valid/ready`, bus-width data, and byte strobes; the endpoint
generates WLAST. Read beats expose bus-width data, error, and LAST with their own
ready/valid handshake. Write completion has a separate ready/valid/error port.
Invalid command shapes complete locally with an error and no AXI transaction.
Independent AW/W handshakes and response backpressure must be supported.

The word adapter converts one 32-bit request into one SIZE=2, LEN=0 command.
Lane placement and strobes follow the byte address on a wider bus. Logical
addresses are translated as `physical_base + (address - arena_base)` within a
configured arena byte count. Underflow, overflow, and out-of-window requests
fail locally instead of truncating high address bits. Default physical placement
is `0x01000000`; all existing synthetic 64-bit arena bases remain usable.
The cores expose their bound `memory_arena_base` alongside the word port. The
adapter uses this snapshot, since the live run/configuration input may change
after admission while background memory traffic still uses the bound arena.

On hardware the endpoint attaches through an AXI4-to-AXI3 compatibility wrapper:
LEN is at most 15 and WID is fixed zero. Zynq-7000 HP supports 32/64-bit buses.
This work does not supply the PS, DDR controller/PHY or board clock/reset logic.

Word reads complete at R acceptance and writes at B acceptance. Cache fence and
retirement drain earlier AXI transactions and preserve visibility; they do not
claim physical DDR completion. Simulation reset coordinates the master and slave
and discards old responses. Resetting only the master against a live PS port
requires a separate system drain/reset policy.

## DDR3 model assumptions

DRAMsim3 is pinned in `dramsim3.lock` (MIT; retain its upstream license in the
downloaded source). It models timing, not sample contents. A shared Python store
owns the bytes and applies WSTRB. Cocotb serves AXI; ctypes calls a small native
C ABI, polls completions and advances multiple DRAM clocks per fabric cycle.

The supplied profile assumes one 4 Gbit x16 device, eight banks, 32,768 rows,
1,024 columns, one rank, and 512 MiB total. Its 1.875 ns clock gives DDR3-1066
and a 16-byte native BL8 transfer. The example fabric clock is 100 MHz. These
are illustrative timing assumptions, not a specification for every Zynq-7010
board or a specific memory part. Power values are placeholders, not power
estimates. Physical placement, clocks and profile hash belong in every report.
See [profile provenance](configs/README.md) for the assumed values and their
limits; this profile is not sourced from a specific board or memory part.

The profile uses an open-page unified transaction queue. Upstream DRAMsim3 posts
write callbacks after admission, before physical completion. Separate read/write
queues can retain small write batches indefinitely; the unified queue avoids
that threshold behavior. The native bridge must expose a real idle/drain check
including issued writes, rather than treating posted callbacks as DDR drain.

Each word pays native-burst timing. An AXI burst covers each native block once;
independent word requests are never grouped by this bridge. DRAMsim3's internal
write merging and read forwarding are controller behavior, counted separately
from logical AXI traffic. Refresh advances even while AXI is idle or stalled.

The model approximates DDR service, not the exact PS controller, HP bridge or
concurrent CPU traffic. Observed maxima are measurements, not hard WCET bounds.

## Isolated checks

With Verilator, Yosys, a C++ compiler and the Python requirements installed:

```sh
python fpga/axi_memory/simulate.py
python fpga/axi_memory/simulate.py --memory-model fixed --memory-cycles 7 --output work/axi-fixed
bash fpga/axi_memory/check.sh
```

The suite tests both bus widths, reads/writes of 1/2/16 beats, upper word lanes,
byte strobes, native-block crossings, AW-before-W and W-before-AW, stalled
responses, local address/shape rejection, errors and coordinated reset. Both
endpoints pass warning-clean lint and ECP5 synthesis; synthesis is not place and
route or a frequency guarantee.

## Detailed timing setup

The optional native model needs Git, patch, CMake (3.16 or newer), and a C++11
compiler. It is not required for the instant/fixed suites.

```sh
python fpga/axi_memory/build_dramsim3.py
python fpga/axi_memory/tests/test_timing.py
python fpga/axi_memory/simulate.py --memory-model dramsim3 --output work/axi-dram
```

The build fetches the exact lock revision into `work/dramsim3/source`, retains
the MIT license, and patches a separate build copy. `native/drain.patch` only
adds idle/bus observation and tracks the last issued write's data-completion time;
it does not change scheduling or callback behavior. The shared library and its
dependency live together in `work/dramsim3/build` with an origin-relative RPATH.
Override locations with `--dram-library`, `--dram-config`, and `--output`.
Rebuild an older native library to obtain ABI 2 and bus counters; stale libraries
fail with a rebuild instruction.

The C ABI reports read completions and **posted write admissions** separately.
Its physical-drain hook waits for transaction/command queues, read callbacks and
issued write data. It is not a prefetch or pipeline barrier. The unified queue
is required by the binding so a final small write batch makes progress.

Native tests also verify row hits/conflicts, bank overlap, refresh while idle,
the 5 1/3 DRAM ticks per 100 MHz fabric cycle, and generation-tagged reset.

## Integrated pipeline

The chunk, media-store, media-audio and full-loop runners use AXI with instant
memory by default. `--memory-model fixed --memory-cycles 7` selects a deterministic
native-block delay; `--memory-model dramsim3` selects detailed timing. Neither
mode groups individual words into bursts. `--axi-data-width 64` exercises narrow
word lanes on the wider bus; `--physical-base 0x02000000` changes DDR placement.

```sh
python fpga/chunk_memory/simulate.py --config all
python fpga/media_store/simulate.py --pipeline --config all
python fpga/loop_pipeline/simulate.py --media --config all
python fpga/loop_pipeline/simulate.py --media --config small --memory-model dramsim3
python fpga/axi_memory/generate_tops.py --check
bash fpga/axi_memory/check_pipeline.sh
```

`--memory-model legacy` retains the original word-slave fixtures, including the
old `--response-cycles` floor. That option is rejected in AXI mode to avoid
stacking delays. Fixed AXI timing includes the endpoint's handshake overhead;
its delay number is not directly comparable to the old word-response floor.

The checked-in thin wrappers expose only AXI for external transactions and
preserve the cores' public host ports. The generator checks for interface drift.
The full-loop testbench has internal word-port probes for protocol checks, not
an alternate external memory service. Both manual-clock and free-running tests
use the same AXI state machine and functional store.

## Measurements

`memory-timing.json` contains actual quantum cycles, assumed-clock microseconds,
host-operation transaction statistics, chunk transfer intervals, and simulation
execution time. The host-operation statistics include snapshots/fences outside
the measured quantum; quantum totals are the RTL admission-to-completion time.
`prefetch-analysis.json` contains measured fill waits and DMA intervals. The
uniform-service replay remains available only with the legacy word backend.

Latency groups distinguish read/write and single beat/burst, and report count,
mean, median, population standard deviation, min/max and nearest-rank p95/p99.
AXI latency runs from address acceptance to R/B consumption; response-available
and interbeat timing are separate, as are channel and native admission stalls.
Word-port and AXI handshakes add latency outside the native DRAM timing itself.
Write bursts are buffered through WLAST before native submission in this model.

Native reports distinguish submitted burst bytes from actual READ/WRITE command
bytes: controller write merging and forwarding can reduce the latter. Read
callback latency and posted write-admission latency are separate from native
physical service histograms. Finalization drains and prints native statistics
once; DRAMsim3's final-statistics printer is not idempotent. Final drain ticks
are reported separately and do not extend the measured logical quantum.

Reproduce the aligned 64-loop/Q=32 case for each model:

```sh
python fpga/media_store/simulate.py --pipeline --config 8x8 --timing \
  --prefetch-analysis --read-buffers 128 --memory-model instant --output work/ddr3-bench-instant
python fpga/media_store/simulate.py --pipeline --config 8x8 --timing \
  --prefetch-analysis --read-buffers 128 --memory-model fixed --memory-cycles 7 --output work/ddr3-bench-fixed
python fpga/media_store/simulate.py --pipeline --config 8x8 --timing \
  --prefetch-analysis --read-buffers 128 --memory-model dramsim3 --output work/ddr3-bench-dram
python fpga/media_store/simulate.py --pipeline --config default --timing \
  --memory-model dramsim3 --output work/ddr3-record-dram
```

Initial results at assumed 100 MHz, 128 read buffers and four write buffers:

| Model | Quantum cycles (each of four) | Quantum time | Exposed fill wait |
| --- | ---: | ---: | ---: |
| Instant AXI | 28,996 | 289.96 us | 0 |
| Fixed, seven native-delay cycles | 28,996 | 289.96 us | 0 |
| DDR3-1066 x16 DRAMsim3 | 28,996 | 289.96 us | 0 |

Each quantum reads 2,048 words from fresh chunk positions, with no writes. Across
the four detailed quanta, 8,192 read responses average 4.2745 AXI fabric cycles,
median 4, standard deviation 2.3515, p99 26 and maximum 37. The model issues
8,192 native READ commands (131,072 bytes for 32,768 useful bytes). Early prefetch
hides their latency in this workload; the equal totals do not imply zero DDR
cost or prove timing under other traffic. All outputs and logical media match
Rust bitwise. The media is larger than the cache and positions move every
quantum, so these results do not depend on reusing the previous quantum's chunk.

## Validation record

- Instant/fixed/DRAMsim3 endpoint suites pass at both 32 and 64 bits, including
  directed burst tests. Eleven native/statistics tests and 19 Rust/Python contract
  tests pass. The unchanged Rust reference/testbench packages pass 74 tests.
- Instant AXI: all four chunk profiles (20 comparison cases, 1,600 random
  requests, plus protocol tests); all three media profiles (12 seeded cases each);
  all three audio profiles (55 cases, 165 quanta); all three full-loop profiles
  (57 cases, 479 quanta, plus preparation/fault tests).
- Detailed timing: standalone chunks, media audio, full-loop record/playback
  (10 cases, 45 quanta, plus fault tests), and the 64-loop benchmark pass. A
  64-bit, single-read/single-write-buffer audio run passes 13 cases/33 quanta,
  including multi-chunk windows and wrap. The legacy delayed full-loop path
  still passes.
- The small capture benchmark issues 64 logical word writes and 61 physical
  native WRITE commands: the controller merges three buffered writes. Posted
  admissions average two DRAM ticks; physical write service averages 13.623
  ticks, with a maximum of 155. Cache-forwarded playback needs no backing reads.
- Every integrated top passes warning-clean lint at both bus widths. The
  default eight-loop AXI top passes generic checks and ECP5 synthesis with
  24,263 LUT4s, 13,916 flip-flops, 93 DP16KD blocks and four MULT18X18D blocks.
  Its optimizer remapping differs from the word-only top, so isolated endpoint
  counts are a better measure of bridge cost: 303 LUT4s/152 flip-flops at 32
  bits, 406 LUT4s/152 flip-flops at 64 bits. No extra BRAM is needed by the
  endpoint. These profiles are not the 64-loop timing configuration.

Detailed setup was also rebuilt from a fresh pinned dependency checkout. There
are no Rust source changes. The checks above are local; PR CI is deliberately
not inspected. Existing cocotb waveform clocks are normalized to ten nanoseconds;
`--fabric-hz` controls the modeled DRAM clock ratio and report time conversion.
Waveform timestamps represent physical time directly at the default 100 MHz.

## DDR bus utilization

Each detailed timing result has a `ddr_bus` object for the same execution window
as the RTL quantum cycle count. It excludes host loading, snapshot readback and
post-quantum fences. `whole_model_bus` in the final native report additionally
covers those host intervals and final drain, so its denominator is different.

`data_bus_utilization` is the fraction of channel-clock slots occupied by actual
physical read/write data, with separate direction fractions. The native observer
counts CL/CWL-delayed data beats, not callbacks or request latency. BL8 occupies
four DRAM clocks per physical command; merged/forwarded requests add no duplicate
bursts. Sampling counters does not drain or print/reset statistics. Windows can
split a transfer and still count only the cycles actually inside the window.

`command_bus_utilization` separately counts issued READ/WRITE/ACT/PRE/REF/etc.
command slots; it is not a measure of bank availability. Refresh, turnarounds and
row constraints can limit service even when the data bus is idle. Physical bytes
and bytes/second use the full bus width, including overfetched or masked lanes;
theoretical peak is `channels * bus_bytes * 2 / tCK`.

Grouping contiguous words should reduce aggregate data-bus use for this workload:
32 separate four-byte reads cost 32 native 16-byte transfers (512 bytes) for a
128-byte chunk. Burst access needs eight such transfers (128 bytes). That is an
expected fourfold traffic reduction in this model, whose independent word reads
do not reuse overfetch. Better packing can make the bus busier during a shorter
transfer while reducing its occupancy over the unchanged quantum/audio period.
This leaves more capacity for contention; the exact benefit on hardware depends
on controller coalescing, arbitration and other traffic. No burst converter has
been added to the pipeline.

The instrumented 64-loop Q=32 run still matches Rust at 28,996 cycles per quantum.
Each of its four quanta transfers 32 KiB physically for 8 KiB of useful samples:
8,192 occupied DDR data cycles out of approximately 154,645 available cycles.
Data-bus utilization is **5.2973%** during execution, and command-bus utilization
is **1.457–1.460%**. Native traffic averages 113.009 MB/s against a theoretical
2.133 GB/s peak. Whole-model utilization, including idle host setup/readback, is
3.2439%; it must not replace the execution-window number.

For the same unchanged execution time, efficiently grouped chunk reads would
transfer 8 KiB and occupy about **1.3243%** of data-bus time; this is a projection,
not an implemented burst-converter benchmark. At an assumed 48 kHz sample rate,
averaging over the full 666.67 us audio period instead gives 2.304% current load
and 0.576% projected burst load. Recording reports keep late writeback outside
the quantum window separate through the whole-model counters.
