# MIDI expression measurement evidence

The readable results are in [the hardware report](../../midi-expression-hardware.md).
`compiled-profile.json` contains the 64-loop compiled program and capacities.
JSON files retain individual callback/quantum samples and validation scope.
`optimized-source.json` identifies the current implementation and local gates;
`results-manifest.json` hashes the benchmark/configuration JSON, including the
retained initial measurements.
`initial/` preserves the first implementation's CC-only measurements. The `*-off-*`
controls and `cpu-64.json` were measured before the RTL area optimization; their
execution paths are unchanged. Current `*-on-*`, musical and optimized tracker
files use RTL source revision `462ce74e`.

`vivado/` contains selected raw reports and source/artifact manifests. The original
state-enabled full build is `initial-state-100` (revision `79b4b9b9`); it failed
placement. Unit experiments separately attribute the BRAM refinement and address
path savings. The compact-unit source hash matches committed `9313e2eb` (before the
directory wait fix); its provenance records the parent plus working-tree snapshot
because it was prepared before that commit. Unit source/context snapshots are
archived for the uncommitted address-path experiment as well. The connected `compact-session-*` builds use committed revision
`462ce74e`. `reports-manifest.json` hashes every archived report/configuration.

Tool source: public `siliconbootcamp/xilinx-vivado:stable`, image digest
`sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f`.
Vivado 2021.1 SW build 3247384; vendor tools are unchanged. The local runtime image
is `sha256:344aaf4c2f320c6b7cf98707da9ff2374b0568ef554244ff54d0f42b68e9f47a`.
It supplies Linux runtime dependencies and a writable build-local home directory.
This is an OOC fabric experiment with two AXI ports, excluding PS7 and board DDR.
Whole-design counts include the unchanged synthetic registered boundaries
(6,667 context registers); these keep public command/result paths live. They are
not a finished board integration.
The padded stereo store alone is 703.5 MiB, so the full session requires at least
1 GiB system RAM; targeting the 7020 fabric does not establish a particular board's
RAM capacity or clock closure.

Reproduce the connected builds from the repository root:

```sh
python3 fpga/synthesis/midi_shared_jobs.py --revision 462ce74e --mhz 100 \
  --profile docs/fpga/measurements/midi-expression-state/compiled-profile.json \
  --output /tmp/midi-expression-100
bash fpga/synthesis/run_midi_container.sh /tmp/midi-expression-100 /path/to/tools runtime-image
```

Repeat at 150 MHz in a fresh directory. Unit experiments use
`fpga/synthesis/midi_state_unit.py --revision REV --stage route --output EMPTY_DIR`,
with the same container runner. Experimental `--working-tree` runs snapshot and
hash the exact unit source. Never substitute such a result for a different source
revision's connected result.

Run the checked callback/transition and fan-out measurements:

```sh
cargo run --release --locked -p rt_fpga_testbench --features sim \
  --example midi_data_quantum_64 -- --scenario mixed --state on \
  --ddr-delay 0 --output /tmp/mixed-on-delay0.json
cargo run --release --locked -p rt_fpga_testbench --features sim \
  --example midi_expression_transitions -- 0 /tmp/musical-delay0.json
cargo run --release --locked -p rt_fpga_testbench --features sim \
  --example midi_expression_scaling -- /tmp/tracker-scaling.json
```

Repeat callback cases for `midi` and `audio`, `--state off`, and delay 7; repeat
transitions at delay 7. These models retain AXI overhead even at zero added service
delay. See each JSON's scope/validation fields for what was checked.
