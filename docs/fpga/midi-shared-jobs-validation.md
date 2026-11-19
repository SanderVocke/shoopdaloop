# Validation of the shared MIDI execution path

The Rust and RTL tests check both the audible/message result and the route taken
to produce it: a compiled SP program, one mixed queue, and the existing JP.
The frozen Rust session exists only in tests. The old RTL controller remains
historical source and is absent from the new pipeline hierarchy.

| Behavior | Evidence |
| --- | --- |
| Record, silent duration, stop, play, repeat and timestamp boundaries | Frozen-session Rust comparisons; RTL service tests and compiled mixed differential test at all five supported quantum sizes. |
| Audio alongside MIDI | Exact audio samples and mixed SP job traces in the connected pipeline; retained audio/compiler/loading regressions. |
| Incremental state, pedals, cleanup, restoration and channel ownership | Independent Rust interpreter/transition tests and RTL destination-state/output comparisons, including ownership handoff and multiple destinations. |
| Partial acceptance, refusal and recovery | Accepted-output progress comparisons; compiled mixed refusal test; fatal audio fault plus explicit MIDI recovery. |
| History, shared grabs and content operations | Rust allocation/capacity scenarios; RTL shared views, pinned exports, staged import/export/duplicate/replace/clear and 128-binding service tests. |
| Atomic publication | Compiled RTL pipeline holds control completion and the next quantum while the embedding publishes the corresponding SP metadata. |
| Program loading and identities | Rust mixed image relocation/replacement and queue preflight; retained live RTL loading tests; channel jobs carry checked loop identities/generations. |
| DDR boundaries | Delayed word responses, errors and stalls; 32/64-bit MIDI AXI tests, including a logical arena above 4 GiB; retained DRAMsim3-backed audio workload. |
| Context lifetime | Stopped/cleaned-up removal; independently pinned exports remain readable after unbind. Input/history services also run with zero Rust MIDI bindings. |

The compiled mixed RTL test has two MIDI loops and an audio passthrough. The
128-binding RTL test exercises MIDI services. The synthesis profile separately
compiles 64 loops with two audio channels and one explicit MIDI binding each.
These checks do not constitute a 64-stereo-loop simultaneous playback deadline
qualification under a real shared DDR controller. Clock timing and job throughput
are separate questions.

The final all-feature Rust reference/compiler run and all **18 MIDI RTL tests**
passed. The connected dual-AXI RTL top passes Verilator lint without diagnostics.
Generated AXI, host and MIDI tops match their generators.

## Local commands

Run from the repository's configured Rust/native-dependency environment:

```sh
cargo fmt --all
python3 scripts/check_shoop_test_usage.py
RUSTFLAGS="-D warnings" cargo build --workspace
cargo test -p rt_fpga_reference -p rt_fpga_compiler --all-features
cargo test -p rt_fpga_testbench --features sim --test midi
cargo test -p rt_fpga_testbench --features sim --test core --test audio --test graph_compiler --test loading --test targets
python3 fpga/axi_memory/generate_tops.py --check
python3 fpga/host_bridge/generate_top.py --check
python3 fpga/midi/generate_pipeline.py --check
```

The loading suite's timed DDR case requires its documented DRAMsim3 adapter;
`python3 fpga/axi_memory/build_dramsim3.py` provisions it. CPU profiling is
reported separately in [the architecture review](midi-shared-jobs.md), without
a cloud elapsed-time gate.

## Broader workspace results

The warning-denying workspace build passed. The broader local nextest run
completed 3,503 tests: 3,487 passed and 16 failed; 11 were skipped. Twelve failed
in the native application's startup with `Runtime(Capacity)`. The untouched
starting worktree reproduces that failure. Three of the other four failures
passed when rerun individually. The remaining launcher-recovery test fails its
launcher-exit assertion outside the modified FPGA reference path. These failures
are retained as limits of the broader local validation; this is not an all-green
workspace result.

No CI results or automated review feedback were consulted. Application MIDI
transport enablement, the SoC's DDR arbitration and board deployment remain
outside this implementation's scope.
