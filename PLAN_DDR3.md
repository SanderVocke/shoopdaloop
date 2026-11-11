# AXI memory endpoint and DRAMsim3 integration

Status: implementation and local validation complete; PR delivery in progress.
Branch: `proto/fpga-rust-reference`, baseline `8f64c32d`.

## Goal and scope

Run the existing RTL loop pipeline against a live DDR3 timing model through
cocotb and a small ctypes-compatible native wrapper. All external memory traffic
must pass through one synthesizable AXI master endpoint that can later connect to
a Zynq PS DDR interface. Keep the Rust reference as the functional oracle.

The endpoint and simulation backend must support bursts, but the current chunk
memory producer must continue issuing one 32-bit word per transaction. There is
**no word-to-burst converter, burst grouping, or producer concurrency change** in
this work. Burst support is exercised independently with directed transactions.

Not included: a DDR controller/PHY, board deployment, moving metadata into DDR,
changes to graph execution or cache policy, cross-iteration prefetch, MIDI, or the
previously discussed trace-replay/statistical-delay design.

## Current implementation

- `fpga/chunk_memory/rtl/chunk_memory.sv` exposes a 64-bit byte address and a
  single-outstanding, 32-bit request/response port. Both reads and writes receive
  responses. Chunk transfers currently consist of successive word transactions.
- `fpga/media_store/rtl/media_audio_pipeline.sv` inserts `memory_delay.sv` before
  exposing that port; `fpga/loop_pipeline/rtl/loop_pipeline.sv` forwards it.
- The functional memory service is implemented in
  `fpga/media_store/tests/media_driver.py`, with duplicate service logic in
  `fpga/loop_pipeline/tests/loop_memory.py`. Standalone chunk tests have another
  service in `fpga/chunk_memory/tests/chunk_driver.py`.
- `fpga/media_store/tests/prefetch_timing.py` assumes a uniform chunk-transfer
  duration. Those assumptions cannot be used for live DRAMsim3 measurements.
- Media descriptors and chunk-ID tables currently reside in local RAM. This
  change routes existing external sample accesses through AXI without relocating
  those tables. The Rust elastic store remains independent of timing and caches.

## Immutable acceptance criteria

1. Standalone chunk memory, the media audio pipeline, and the connected loop
   pipeline can use the same synthesizable AXI master implementation. Their
   integrated simulation tops expose this as the sole external memory path.
2. Normal pipeline execution issues only single-beat, 32-bit accesses, with one
   transaction outstanding. Adding AXI must preserve samples, logical media,
   ordering, cache forwarding, cancellation, retirement, and fault reporting.
3. The AXI endpoint and cocotb/DRAMsim3 slave support single beats and bounded
   INCR read/write bursts. Burst correctness is tested without introducing a
   burst converter into the existing producer.
4. The same AXI boundary supports instant functional memory, deterministic fixed
   latency, and live DRAMsim3 timing. DRAMsim3 is simulation-only and optional for
   the ordinary fast regression workflow.
5. A checked-in, explicitly documented DDR3 profile approximates a small
   Zynq-7010 system. Its clock, bus width, geometry, timing, and address mapping
   are recorded assumptions, not claims that all 7010 boards have that memory.
6. Reproducible reports include transaction latency statistics and end-to-end
   quantum timing, including the aligned 64-loop, Q=32 workload. Rust/RTL data
   comparisons pass with the detailed model as well as the fast backend.

## Design rules

### Synthesizable boundary

```text
chunk memory's existing word request/response port
    -> word adapter (one request becomes one AXI beat)
    -> common AXI master endpoint
    -> cocotb AXI slave -> ctypes C ABI -> DRAMsim3
                       -> functional sample storage

future hardware: the same AXI master -> PS HP compatibility wrapper -> PS DDR
```

- Place the reusable endpoint, word adapter, simulation support, and isolated
  tests under a new `fpga/axi_memory/` directory. Add thin integration wrappers
  around the existing cores rather than rewriting processors or cache engines.
  Lower-level word-port tests may remain, but the new integrated path must use
  the real AXI RTL even with instant memory.
- Give the endpoint a command header and read/write beat streams. Commands carry
  byte address, direction, beat size, and beat count. The word adapter always
  requests one four-byte beat; an isolated fixture supplies multi-beat commands.
- Use AXI4 with a deliberately AXI3-compatible subset: fixed ID, one outstanding
  transaction, INCR bursts of at most 16 beats, and no interleaving. Zynq-7000 PS
  HP ports use AXI3; document the required LEN/WID and width adaptation. Actual
  board integration is later work.
- Handle independent AW/W handshakes, AR/R/B backpressure, stable stalled
  payloads, byte strobes, lane placement, LAST, and response errors. Define legal
  beat sizes and the 4 KiB boundary constraint. Parameterize the AXI data width
  for 32/64-bit attachment without aggregating word requests.
- Define a configurable, checked mapping from the logical arena to the physical
  DDR address window. Existing tests use `0x1234000000`, above the PS address
  range; do not silently truncate it. Use the same mapping in simulation and the
  eventual hardware wrapper. Record physical placement in timing reports.
  Integration finding: the full-loop tests deliberately change the live arena
  input after run admission. Expose the cores' already-bound `memory_arena_base`
  as an additional word-port output, so address translation uses the same
  snapshot as the DMA engine. Request/response handshakes remain unchanged.
- A word write completes on its AXI B response; a read completes on its R beat.
  Fence/retire require earlier transactions to complete at the AXI boundary,
  preserving same-address visibility and safe reuse. They do not assert that
  physical DRAM cells have been updated. Model drain is a separate operation.
- Define reset as a coordinated endpoint/slave reset in simulation: discard old
  responses without allowing them to satisfy a new request. Document that
  independent reset against a live PS port requires a system-level drain/reset
  policy; do not imply AXI transactions can be canceled on real hardware.

### Native timing model and functional memory

- Pin DRAMsim3 and build a small C++ shared library with a C ABI, loaded using
  Python ctypes. Expose creation/destruction, configuration/geometry, admission,
  transaction submission, batched clock advancement, completion polling, and
  statistics/drain control. No C++ callbacks into Python are needed.
- DRAMsim3 supplies timing; the shared functional store supplies sample data and
  applies write strobes. Keep ordered visibility and fault injection explicit.
  Track logical request IDs and native transaction associations in the wrapper,
  including repeated accesses to the same address.
- Map AXI byte ranges onto native DRAM burst transactions using the configured
  bus width and burst length. For BL8, native transfers are 16 bytes on x16 or
  32 bytes on x32. A four-byte request still incurs native-burst timing. Cover
  each touched native block once within an AXI burst; do not invent coalescing
  between independent word requests. Preserve partial-word/byte write semantics.
- Queue capacity must backpressure admission. Read beats become available in
  AXI order after their native block is ready and obey AXI bus serialization;
  completion of one native burst does not create a separate full DRAM delay for
  every AXI beat. Buffer completed responses while RREADY/BREADY is low.
- Verify the pinned DRAMsim3 write semantics: its normal write callback can mean
  posted acceptance rather than physical completion. Keep AXI-visible write
  acknowledgement, functional visibility, physical accounting, and final model
  drain distinct. Use a supported queue/drain configuration or a narrowly scoped
  native hook so short write workloads cannot remain buffered indefinitely.
- Advance DRAM time on every fabric cycle, including idle and backpressured
  cycles, so refresh and queue progress continue. Use an integer time accumulator
  for non-integral clock ratios and batch native ticks per fabric cycle. Keep
  ctypes overhead per simulated DRAM tick out of Python.
- Disable the existing fixed-delay floor in DRAM mode. Retain instant and fixed
  timing modes without requiring DRAMsim3 to be installed; fail clearly if the
  detailed mode is requested without its library/configuration.

## Staged implementation

Stages are sequential; each verification gate precedes integration that depends
on it. Keep detailed-model runs small during development.

### 1. Freeze interfaces and the model profile

- [x] Record the baseline and working branch. Reuse `proto/fpga-rust-reference`
  and its PR if still appropriate; otherwise create a dedicated implementation
  branch from the current feature base, preserving concurrent upstream work.
- [x] Specify endpoint commands/streams, supported AXI subset, word responses,
  error/reset/fence semantics, address mapping, and simulator selection.
- [x] Pin DRAMsim3, its build recipe and license handling. Check its native burst
  size, callback meanings, queue behavior, and end-of-run drain requirements.
- [x] Add one illustrative DDR3-1066-class profile with coherent device timings,
  capacity, x16/x32 width, bank/row geometry, refresh, and controller policy.
  Name the assumed part/board if known; otherwise label the assumptions clearly.
- [x] Verify the contract against existing cache/fence tests and check profile
  geometry and clock units before writing RTL or integration code.

### 2. Implement and verify the AXI master boundary

- [x] Add the synthesizable burst-capable endpoint and single-word adapter.
  Keep the existing chunk engine's request/response contract unchanged.
- [x] Add a minimal instant AXI slave and direct command fixture. Verify reads,
  writes, burst lengths 1/2/16, native-block crossings, narrow transfers, byte
  strobes, response errors, reset, and independently stalled AXI channels.
- [x] Check that word-adapter traffic always has LEN=0 and size=4 bytes, and that
  one outstanding request cannot be accidentally duplicated or reordered.
- [x] Run isolated cocotb protocol tests, lint, and synthesis for the supported
  widths. Record endpoint resource overhead; do not infer achieved clock timing
  from synthesis alone.

### 3. Implement the DRAMsim3 C ABI and Python timing backend

- [x] Add reproducible native-library build/loading and a thin ctypes binding.
  Implement transaction bookkeeping, bounded admission, clock conversion,
  completion polling, and explicit statistics/drain behavior.
- [x] Implement one shared functional memory and AXI slave service with instant,
  fixed, and DRAMsim3 timing policies. Separate clock scheduling from service
  state so both existing manual-clock and free-running tests can use it.
- [x] Verify native-wrapper behavior without RTL first: read/write visibility,
  repeated addresses, partial writes, queue saturation, non-integral clocks,
  idle refresh, reset generations, and a final small write workload that drains.
- [x] Run the same directed AXI word/burst tests against DRAMsim3. Add focused
  row-hit/conflict, bank, read/write-turnaround, and refresh cases; check timing
  relationships and transaction counts rather than fragile exact cycle totals.

### 4. Wire the common endpoint into the existing pipelines

- [x] Add AXI integration tops for chunk memory, media audio, and the full loop
  pipeline. Keep the core word port internal and expose one AXI memory path.
- [x] Update simulation runners and source lists to select timing mode, native
  library, profile, fabric clock, and address window. Retain quick default runs
  and existing deterministic-delay comparisons without stacking two delays.
- [x] Adapt `media_driver.py`, `loop_memory.py`, and chunk tests to the shared
  backend. Preserve host configuration/snapshot helpers, seeded stalls, held
  responses, fault injection, and timing observer hooks.
- [x] Compare instant-AXI runs with existing Rust fixtures first; then run a
  bounded detailed-model subset covering record/play, wrap, multiple chunks,
  ordered read/write/read, stale prefetch responses, fence, and retire/reuse.
  Drain to the appropriate visibility point before comparing backing contents.

### 5. Add measurements that handle variable memory latency

- [x] Update prefetch instrumentation to record actual transfer intervals and
  wait cycles without requiring all chunks to have the same service time. Keep
  the old constant-cost replay clearly confined to fixed-latency runs.
- [x] Emit machine-readable reports with counts, mean, median, standard
  deviation, min/max and tail percentiles for reads/writes, single beats/bursts,
  first/last read beat, and AXI acknowledgement versus native service where
  available. Count logical bytes separately from native DRAM traffic.
- [x] Include queue/backpressure time, chunk fill/writeback durations, exposed
  demand stalls, total quantum cycles/time, and simulation execution time. Record
  the seed, revision, model/config hash, clocks, placement and cache capacities.
- [x] Verify counts and summary statistics against small hand-checkable cases;
  run word/burst microbenchmarks without changing the application producer.

### 6. End-to-end validation and delivery

- [x] Run the existing standalone chunk, media and full-loop Rust/RTL comparison
  suites through instant AXI; repeat representative seeded cases with DRAMsim3.
  Retain isolated endpoint burst tests as a separate required gate.
- [x] Re-run aligned 8x8/64-loop playback at Q=32 with moving positions and media
  larger than cache capacity. Compare instant, fixed, and DRAMsim3 timing with
  identical inputs/cache sizes; distinguish cold and subsequent quanta. Include
  a smaller capture/writeback workload to validate posted-write accounting.
- [x] Check samples and logical media bitwise against Rust. Report timing rather
  than imposing a speculative performance target. A measured maximum is not a
  hard real-time bound, and generic DRAMsim3 controller timing is not an exact
  model of the Zynq PS controller, HP bridge, or other system traffic.
- [x] Run lint/synthesis for the integrated AXI top and record resource changes.
  Run the repository's applicable local gates once at this milestone; use Rust
  formatting, warning-denying builds and test-attribute checks if Rust changed.
- [x] Document setup, fast/detailed commands, burst tests, profile assumptions,
  statistics meanings, reset/fence guarantees and current modeling limitations.
- [ ] Commit milestones, push, and update the existing PR or open a new PR for
  the chosen branch. Check automated review comments, address applicable
  feedback, and repeat until approval or a documented blocker. **Do not inspect
  PR CI**, per the user's instruction. Do not merge autonomously; report local
  validation and any unresolved review feedback.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user
  approval.
