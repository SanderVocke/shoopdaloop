# Rust reference for retained FPGA components

This restores the functional state/job and logical-memory implementation from
`354305d4697d2e6068e28d4a2e369f08974e434d`, before its deletion in `16679ba7`.
It covers the retained hardware, including constant, arithmetic/balance and
restart-smoothing instructions added after that historical revision.

The crate is `no_std` with `alloc`. It contains:

- `state`: persistent programs, typed state, ordered job emission and read hints.
- `jobs`: sample buffers, Q8.24 processing, persistent smoothing histories and
  memory operations through an explicit `Memory` trait.
- `fixed`: independent saturating arithmetic and sample-duration ramps.
- `records`: caller-supplied read-only records with generation checks.
- `storage`, `chunk_memory`, `resolver`: logical media ownership, slices,
  recording/readback, synchronous chunk storage and advisory read resolution.

Programs and records are supplied explicitly. There is no transition processor,
scheduler, graph compiler, owning-request adapter, ARM protocol or application
backend. Records never advance logical clocks. `serde` and `schema` are optional
fixture/data-description features, not a hardware transport contract.

## Functional contract

SP emits a complete quantum's ordered jobs before JP consumes them. Gain state
advances when its job is emitted; failed later instructions retain earlier state
writes and emitted jobs. Empty/undersized queues and invalid state are reported.
`run_with_records` supplies logical playback/capture/smoothing inputs directly;
the records contain no future actions or transition requests.

JP validates ready/destination buffers, performs integer sample operations, and
preserves history across quanta. `begin` resets buffer readiness; `reset` also
invalidates smoothing history. `with_history_slots` permits physical history
capacity to exceed the run's logical sample-buffer count. `Memory::supports_media`
describes a media-disabled JP configuration; enabled memory resolves chunk-based
read windows and accepts capture jobs.

These are functional models, not cycle simulators. The logical memory model is
not an AXI/cache/DDR timing model and does not predict physical allocation order
or delayed reclamation. Transport framing, malformed packed instructions,
pipeline fault containment and bus/reset timing remain RTL harness obligations.
There is no claim that all possible programs, capacities or hardware faults have
been exhaustively verified.

## Verification

```sh
cargo build --locked -p rt_fpga_reference --no-default-features
cargo test --locked -p rt_fpga_reference --all-features
cargo test --locked -p rt_fpga_testbench --features sim --test reference --test state --test audio --test smoothing
```

The Marlin tests use Verilator 5.032 and the existing native component drivers.
They independently run the same explicit inputs through Rust and RTL and compare
emitted jobs, state writes, failure prefixes, remaining jobs, buffer contents and
every output sample exactly. No audio tolerance or alignment is applied.

Coverage includes all twelve current SP opcodes, generation/mode rejection,
logical record delays, FIFO admission/exhaustion, persistent ramps, saturating
gains/mixes, all arithmetic modes, interleaved smoothing histories, restarts,
physical history indices beyond the logical buffer count, invalid JP targets,
media-disabled operation refusal, and SP failures after earlier successful quanta.
Seed 20261115 drives 80 audio
sessions of six quanta. Additional TP-free memory sessions compare capture,
wrapped playback, clear/retake and logical sample readback for Q=4/8/32.

The five historical chunk-memory tests and fifteen independent media ownership,
slice, snapshot and capacity tests are retained. The former compiler/backend test
is replaced by direct state/job/media comparisons. Existing production CPU
numeric checks and explicit RTL vectors remain separate validation sources.
