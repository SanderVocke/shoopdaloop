# MIDI execution-time breakdown

`profile.json` is a full 64-loop, MIDI-only run with zero added DDR delay.
It uses the same 30-second clips and hardware capacities as the earlier callback
benchmark. Observation-only probes expose controller phases; a Rust simulation
agent counts rising edges during quantum execution. Detailed counters are enabled
for quanta 17 and 19, emitting five and four messages respectively.

Each category partition sums exactly to the measured accepted-run-through-done
cycle count. Every output matches Rust, and all 20 quantum clock counts match
the original uninstrumented ideal-DDR run. `smoke.json` checks the counters with
two loops and no messages due in the sampled quanta. No functional RTL changed.

`job_cycles` and `categories` are separate views of the same clocks: they must
not be added together. The summary's disjoint groups use the playback-read job,
other per-loop jobs, the emission job's directory walk, and remaining time.
The sampled windows have no restoration or cleanup messages. Opcode histograms
count execute-stage cycles, including a blocked command instruction; they are
not retired-instruction counts. Memory occupancy counts an AXI transaction's
address-acceptance through final-response clocks, inclusive. MIDI transactions
do not overlap. Instruction/control and adapter cycles outside those transactions
still count toward quantum execution.

Reproduce in the development environment with Verilator 5.032:

```sh
RUSTFLAGS='-D warnings' cargo build --locked --release -p rt_fpga_testbench --features sim --example midi_quantum_64
target/release/examples/midi_quantum_64 --midi-only --ddr-delay 0 --quanta 3 --profile --output profile.json
XDG_CACHE_HOME=/tmp/midi-font-cache MPLCONFIGDIR=/tmp/midi-plot-cache python3 summarize.py
```

The probes add host simulation overhead. They do not add hardware clocks. Initial
loading and unselected warmup quanta do not collect per-cycle detail. Source
hashes identify the counter implementation and relevant RTL. The local build and
run logs are retained. CI and review feedback were not consulted.

Validation is scoped to these testbench-only changes: warning-denying build,
formatting, the small counter check, and full-session Rust/output and original
clock-count comparisons. The application and full RTL suites were not rerun.
