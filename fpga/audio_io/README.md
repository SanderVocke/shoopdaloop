# Fabric audio I/O

A lockstep frame interface connects arbitrary audio protocol adapters to quantum
processing. All lanes share frame timing; routing and loop grouping belong to the
graph. The demonstration uses generated input, the real processing core and an
observable output. There is no physical audio protocol or ARM audio transport.

## Interface contract

`audio_clk` clocks the interface; `frame_tick` identifies one sample instant.
`rx_samples`/`tx_samples` pack lane c in bits `[32*c +: 32]`, raw signed Q8.24.
Provide at least four audio-clock cycles between frame ticks for RAM prefetch.
The interface never backpressures a frame. `clk` is an unrelated processing clock.

RX quantum n contains frames `[n*Q,(n+1)*Q)`. Output quantum n begins at
`n*Q + Q + P`, for `1 <= P <= Q`; P=Q selects 2Q latency. The audio domain
chooses a complete output bank, or all-channel silence, for the entire quantum.
`frame_prepare` closes eligibility two audio-clock edges before the sample
edge, allowing synchronous RAM selection/read. A completion visible at that
prefetch edge is eligible; one visible later is late. Startup before Q+P frames is expected silence.

Input/output banks remain private until descriptor publication and are never
reused before acknowledgement. Per-bank two-phase request/acknowledge mailboxes
form the bounded descriptor rings. Payload metadata remains stable from request
until acknowledgement; only request/acknowledge bits cross synchronizers. This
replaces a general asynchronous descriptor FIFO: for two or three banks, the
mailboxes encode bank ownership directly and support arbitrary ring sizes without
binary/Gray FIFO pointer constraints. Sample words use dual-clock BRAM per lane.

The core has dedicated synchronous input reads and output writes indexed by
`channel*Q + frame`. A run handshake latches its input/output bank allocation.
Core completion releases RX and publishes TX only on whole-pipeline success.
The engine remains the sole authority for logical state and clock advancement.

Commands are admitted in a bounded sequence-tagged fabric queue. A command's
quantum is its logical execution boundary, independent of an output underrun or
backlog. Prepared setup is loaded before arm. The demo's finite loader/command
ROM comes from graph compilation; it does not compute sample results.

## Failure and reset contract

Output deadline misses discard only output: finish processing, preserve recording
and state, and continue in input order at fixed playback times. An input bank
collision, malformed sequence, command overflow, watchdog expiry or core/memory
fault requests recovery and stops admission. All outputs become silent.

`recovery_requested` is sticky until coordinated reset. Recovery first quiesces
memory, then resets both I/O domains and the core, reloads a prepared image, and
rearms. Both clocks must run during reset; assert the common reset in both domains
and release it synchronously in each domain. Reset clears control/descriptors,
not sample RAM. A new epoch is assigned on rearm. Old AXI responses must have been
retired before core reset; I/O epoch tags do not make an unsafe AXI reset safe.

The physical audio frame counter continues while stopped/recovering; session
frame zero is the arm boundary. Rearm is forbidden before the reload/quiescence
handshake. Losing input does not silently advance an incomplete session.

## Initial profile

Q=32, chunk=32, 16 RX/TX lanes, three RX/TX banks (12 KiB logical samples).
Two-bank profiles and Q=16/64 are required checks. Local transport tests also use
Q=1 and odd Q independently of supported processing/media profiles.

Media defaults follow the processing core: BLOCK_CHUNKS=32 and SLOTS=1024,
providing 32 allocation blocks. Sixteen simultaneous independent recordings need
at least 16 such blocks even when each take is shorter than one block. The
prepared loader writes block counts; sample transport and Q-sized I/O banks are
independent of this allocation geometry. The runner accepts `--block-chunks` and
`--slots` and rejects insufficient capacity. `--block-chunks 1` selects the legacy
allocation profile for comparison.

The 16-loop graph maps lane c to input port c, a mono capture/playback view for
logical loop c, and output port c. Input/Output and BeginCapture/Capture/ReadView
remain ordinary jobs. Monitoring and playback fan-in is an explicit graph mix.

## Build and replay

Use Rust stable, Verilator 5.032, Python with the existing cocotb requirements,
and Yosys for synthesis. Run from the repository root:

```sh
cargo build --locked -p rt_backend_testbench
python fpga/audio_io/prepare_demo.py --output work/prepared-audio
python fpga/audio_io/simulate.py
python fpga/audio_io/simulate.py --q 1 --banks 2 --audio-period 10
python fpga/audio_io/simulate.py --q 3 --p 1
python fpga/audio_io/simulate.py --control
python fpga/audio_io/simulate_pipeline.py
python fpga/audio_io/simulate_pipeline.py --overrun --output work/audio-overrun
python fpga/audio_io/simulate_pipeline.py --q 16 --chunk 8 --p 8 --banks 2 \
  --monitoring --frame-cycles 1024 --output work/audio-q16
python fpga/audio_io/simulate_pipeline.py --demo --output work/audio-demo
python fpga/audio_io/check.py
python fpga/audio_io/check.py --demo --output work/audio-demo-synthesis
```

For the detailed DDR model, first run `python fpga/axi_memory/build_dramsim3.py`,
then add `--memory-model dramsim3 --axi-data-width 64 --stalls` to the pipeline
runner. `--frame-cycles 1600 --audio-period 13` approximates 48 kHz; clock periods
and frame spacing are explicit simulation inputs. `--memory-model fixed` provides
bounded response delay. Keep output directories separate between concurrent runs.
`--routing reverse|fanout|sparse` tests graph-side mapping; `--monitoring` adds an
explicit ordered mix with playback. `--q` changes compiled and physical quantum;
`--chunk` can select a smaller compatible media chunk.

The runner saves scenario, compiled image, fixed/float production results,
prepared ROMs, observed frames, publication times, summary and simulator logs.
Expected processing samples/media come from production Rust, not a Python DSP
model. Each prepared case also passes the shared production float-accuracy/audit
gate; fixed/RTL comparisons remain bit exact. Publication measurements include RX handoff, controls, core execution and
TX handoff. Check them against the selected P budget plus a physical timing margin;
they establish measured cases, not every possible graph's worst case.

## Integration and prepared demo

`audio_loop_pipeline` is the reusable synthesizable AXI top. Set MAX_FRAMES to
Q and hold run shape/count metadata stable while enabled. Input/output channel
counts must match physical lane counts. `sample_input_*` and `sample_output_*`
are dedicated synchronous core accesses, with Q as their per-channel stride.
`EXTERNAL_IO=1` removes legacy I/O RAM; configuration accesses to those spaces
are rejected. Scratch debug remains. Existing standalone benches use mode 0 and
keep their original configuration transport. Regenerate wrappers after core port
changes:

```sh
python fpga/axi_memory/generate_tops.py
python fpga/audio_io/generate_top.py
python fpga/audio_io/generate_demo.py
```

`audio_demo` supplies the deterministic frame generator, prepared boot ROM,
command ROM, queue producer and recovery controller. Its compiled graph has 16
independent mono loops, 64 instructions, 32 state entries/buffers and a worst-case
80 emitted jobs including all BeginCapture jobs. The prepared controller records
all lanes for two quanta, plays them, stops/re-records even-numbered loops while
odd-numbered loops continue, and plays the retakes. Later quanta continue playback
with dynamically numbered empty command fences. The demo never feeds TX back to RX.

The checked-in `examples/default/{boot,events}.hex` and `config.json` are generated
from the ordinary production graph by `prepare_demo.py` with default arguments.
For another graph/profile, regenerate the files and use the emitted lengths and
counts as the demo parameters. The ROM stores control/setup only, never expected
sample output. The sink is the full TX frame port plus `tx_valid`.

Control queue words are 746 bits: logical sequence [745:682], kind [681:680],
opcode [679:672], index [671:640], payload [639:0]. Runtime kinds are transition
command (0), state/program write (1), and quantum fence (2). Kind 1 uses index bit
31 for state/program selection, index bits 15:0 as address and payload bits 128:0
as the existing state/config word. A fence is required for every quantum, even
when it has no controls. Producers obey valid/ready backpressure and preserve
order; an unexpected sequence or kind is fatal. Later controls cannot cross an
unfinished run or a fence. Boot kind 2 instead writes media configuration; boot
kind 3 ends loading. Boot and runtime streams are distinct.

## Recovery ownership and diagnostics

Set `quiesce` to stop admitting AXI addresses while preserving exposed requests
and retiring accepted reads/writes. `memory_quiescent` acknowledges that the
transport has drained. With enable low, quiesce high and memory quiescent,
`session_reset_ready` permits a held session reset. The demo performs this
sequence automatically, holds reset for RESET_CYCLES processing clocks, reloads
and increments epoch. Choose RESET_CYCLES long enough for at least four audio
clock cycles; both clocks must continue running. Global reset is a power/system
reset and requires the platform to have removed old external memory traffic.

A nonresponding memory system cannot safely be reused: the controller stays in
DRAINING with silent output until responses retire. It does not falsely declare
an unsafe reset successful. `memory_quiescent` and the retained fault distinguish
this condition. Platform-level memory repair is the external owner's task.

Recovery reasons: 1 input bank loss, 2 processing watchdog, 3 core/control/memory
failure, 4 malformed input descriptor. Core error stage/code remain available
until session reset. The demo retains last reason, failure sequence/logical frame,
core stage/code and a saturating recovery count across reload; epoch distinguishes
new sessions. The watchdog begins at accepted run and is configured by
WATCHDOG_CYCLES. Missing controls/banks also have a finite input-storage bound.

`processed_quanta`, queue high-water marks and recovery metadata belong to clk;
playback/miss/discard/input-loss counters and frame counters belong to audio_clk.
Counters saturate, except the 64-bit timelines/sequences and epoch. Played counts
successful bank selections; a later fatal fault can interrupt a selected block.
Diagnostics crossing into a future register interface require a coherent snapshot.
See `constraints/README.md` before physical integration.

## Validation checkpoint

The pre-#972 baseline in `measurements/validation.json` records the measured configurations and replay
commands. Complete-top mapping with Yosys 0.52 to ECP5 primitives gives:

| Top, Q=32, 16x16, three banks each | LUT4 | FF | DP16KD | MULT18X18D |
| --- | ---: | ---: | ---: | ---: |
| Reusable main top, 128 physical scratch buffers | 37,079 | 20,860 | 152 | 4 |
| Prepared demo, 32 physical scratch buffers | 28,261 | 16,600 | 132 | 6 |

Both include the actual processing/media/AXI path and 12 KiB of logical I/O
samples. The demo adds its generator and loader ROMs. RAM granularity means these
counts are not the logical byte total divided by BRAM size. These figures do not
establish fit or routed timing for a particular device.

Measured maximum last-input-sample-to-output-eligibility times are 91.819 us for
Q=16/P=8 with two banks and monitoring, 111.488 us for Q=32/P=16 with stalled
AXI64/DRAMsim3, and 208.806 us for Q=64/P=32 with permutation, monitoring and
stalled AXI64/fixed memory. The corresponding conservative P budgets in those
test clock profiles exceed 106 us, 332 us and 212 us. Select P from the complete
worst-case bound plus margin for the actual graph and physical clock; these are
measured workloads. The Q=32 DDR case uses a sample rate close to 48 kHz.

Both demo latency modes pass complete record/playback sessions, input-loss
recovery, memory-error recovery and a withheld-response drain before reset.
Local tests verify exact cutoff behavior, one/two silent quanta followed by
catch-up, signed/extreme words, bank reuse and reset at ownership transitions.
The target branch's production-backed subsystem tests remain in service.
