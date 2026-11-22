# Zynq 7020 area experiment evidence

Execute [the experiment plan](../../area-experiments-plan.md) sequentially.
The immutable implementation control is
`f218087511dc28e72a436ec825445f32a722b5e9`; the primary profile is the
[64-loop compiled profile](../midi-expression-state/compiled-profile.json).

Current status: stage 0 tool provisioning and baseline reproduction are in
progress. No experimental RTL changes have been made. Archived measurements are
reference evidence, not a new baseline run. See `ledger.json` for completed
checks, pending measurements, and exact local artifact locations.

Prepare a committed or explicitly hashed working-tree candidate:

```sh
python3 fpga/synthesis/midi_shared_jobs.py --revision REVISION --mhz 100 \
  --stage opt --profile docs/fpga/measurements/midi-expression-state/compiled-profile.json \
  --output EMPTY_BUILD_DIRECTORY
```

`--stage` selects `synth`, `opt`, `place`, or `route` (default). `--param NAME=INTEGER`
selects a declared top parameter and records the override. `--working-tree`
snapshots current sources, including new RTL files. Lower clock targets use the
same constraint generator. All input snapshots have SHA-256 manifests; completed
build directories are never reused for a different candidate.

Run `run_midi_container.sh` with the documented tool directory/runtime image, or
run `vivado -mode batch -source run.tcl` in the prepared directory. Preserve
optimized/routed checkpoints for controls and selected candidates by setting
`MIDI_SAVE_CHECKPOINTS=1` when using the container runner.

Build the checked benchmark and run the fixed workload matrix:

```sh
cargo build --release --locked -p rt_fpga_testbench --features sim --example midi_data_quantum_64
python3 fpga/synthesis/area_benchmarks.py \
  --binary target/release/examples/midi_data_quantum_64 --output NEW_RESULT_DIRECTORY
```

Use `--case mixed-on-delay7` for the first smoke measurement. The full matrix
contains audio/MIDI/mixed workloads, tracking off/on, and DDR delays 0/7. Cold
start, 16 warmup quanta, and steady samples remain separate. `--quanta` extends
the measured steady interval. Directed ownership and musical-transition tests
remain separate from this playback fixture.

`--parameters FILE.json` supplies explicit benchmark parameters, with string
values such as `{"TABLE_LINES":"64"}`. Physical geometry changes also require a
matching driver/reference fixture. Each output records source hashes, model
parameters, tool/revision identity, and per-quantum traffic/cycles. The matrix
manifest also hashes the frozen executable and preserves failed-run logs.

Collect area and callback evidence:

```sh
python3 fpga/synthesis/area_results.py --build BUILD_DIRECTORY --label LABEL \
  --benchmark CALLBACK_JSON --output NEW_SUMMARY_JSON
```

Add `--control CONTROL_SUMMARY_JSON` for resource deltas. The collector verifies
snapshot manifests and simulator source hashes, lists simulation/implementation
parameter differences, and rejects comparisons at different clocks/parts.
Missing routed slices remain null. Setup/hold success is separate from complete
DRC, constraint coverage, and board qualification. Original per-quantum samples
and raw reports remain the source of truth for the compact summary.

Runner checks:

```sh
PYTHONPATH=fpga/synthesis python3 -m unittest fpga/synthesis/test_area_experiments.py
```
