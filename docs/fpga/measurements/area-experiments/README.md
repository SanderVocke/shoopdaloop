# Zynq 7020 area experiment evidence

Execute [the experiment plan](../../area-experiments-plan.md) sequentially.
The immutable implementation control is
`f218087511dc28e72a436ec825445f32a722b5e9`; the primary profile is the
[64-loop compiled profile](../midi-expression-state/compiled-profile.json).

Current status: the fresh B0 implementation reproduces the archived 100 MHz
resources and timing exactly. All twelve callback control cases pass and reproduce all 396 archived
per-quantum rows exactly. Both count alternatives are measured. The serial RAM count is preferred,
and its physical repeat matches all four stage metrics exactly. See `ledger.json` for
completed checks, pending measurements, and local artifact locations.

| B0 routed control | Archived | Fresh |
| --- | ---: | ---: |
| LUTs | 41,980 | 41,980 |
| Slices | 13,205 | 13,205 |
| Registers | 33,441 | 33,441 |
| BRAM tiles | 77.5 | 77.5 |
| DSPs | 4 | 4 |
| CARRY4 | 5,491 | 5,491 |
| Setup slack ns | -5.429 | -5.429 |
| Hold slack ns | +0.008 | +0.008 |

The negative setup slack remains a timing failure. Raw reports are in `b0-100/`;
`b0-100-summary.json` also verifies the mixed workload's RTL source hashes.
The optimized simulator reproduces every original mixed per-quantum result and
cache counter, reducing measured wall time from 440.97 to 361.20 seconds.

The SP and JP/audio standalone checks pass. Unchanged B0 media lint reports
unnamed population-count generate blocks and an unused validity bitmap when
view reuse is disabled; the incremental replacement resolves these warnings.
The [workload matrix](workload-matrix.md) maps directed regression and boundary
coverage to experiment families. Raw callback controls are in `b0-matrix/`.
Descriptor-statistics controls cover repeated writes, sparse restore, reset with
stale RAM, and full occupancy at 1/7/512/513 views. B0 diagnostic reads take six
clocks. Metadata and ownership tests pass, including DRAMsim3.

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

After a successful screen, resume its saved checkpoint into a new directory:

```sh
python3 fpga/synthesis/area_resume.py --build SCREEN_DIRECTORY \
  --output NEW_ROUTE_DIRECTORY --stage route
```

The resume path checks source and context manifests, requires the same flow,
and records the parent checkpoint hash. It retains the earlier stage reports.
Changed HDL, constraints, or implementation settings require a fresh screen.

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

`SHOOP_FPGA_SOURCE_ROOT=/absolute/snapshot/root` selects an immutable source
tree for simulation through the same selector used by the RTL library. Include
the required FPGA test adapters in that tree. Generated simulation files go in
its `work/` directory, so use a separate simulation snapshot rather than the
manifested physical-build `source/` directory. The default remains the repository
tree. This permits a B0 harness control alongside the current candidate without
changing either run's RTL inputs.

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

Retain a completed build as a compact source/report archive and summary:

```sh
python3 fpga/synthesis/area_archive.py --build BUILD_DIRECTORY --label LABEL \
  --benchmark CALLBACK_JSON --control CONTROL_SUMMARY_JSON \
  --output-directory docs/fpga/measurements/area-experiments
```

Each archive has a member-by-member SHA-256 manifest. Checkpoints stay in the
local build directory with their sizes and hashes recorded separately; they are
not uploaded with the source/report archive. The archive round-trip was checked
against all four B0 implementation stages.

Runner checks:

```sh
PYTHONPATH=fpga/synthesis python3 -m unittest fpga/synthesis/test_area_experiments.py
```

## Stage 1 measurements

| Routed 100 MHz result | B0 | Incremental count | Serial RAM count |
| --- | ---: | ---: | ---: |
| LUTs | 41,980 | 39,940 | 39,399 |
| Slices | 13,205 | 13,016 | 12,616 |
| Registers | 33,441 | 33,456 | 32,987 |
| CARRY4 | 5,491 | 3,965 | 3,958 |
| Control sets | 2,215 | 1,672 | 1,494 |
| Setup slack ns | -5.429 | -3.995 | -4.157 |
| Hold slack ns | +0.008 | +0.005 | +0.005 |
| Diagnostic read, clocks | 6 | 6 | 519 |
| Reset admission, clocks | 2 | 2 | 513 |

BRAM remains 77.5 tiles and DSP use remains four. All 33 mixed/on/delay7
measurement rows, including cycles and traffic, are identical: cold start
110,356 clocks, steady median/max 54,146/54,753. Diagnostic and reset measurements use 512 views with reuse enabled.
Reset timing includes one asserted reset clock and the readiness wait; diagnostic
timing covers the driver call including admission and response cleanup. The
serial count scans all configured entries, including sparse raw restores beyond
the issued-view high-water mark. Reuse-disabled counts keep their issued-view
semantics and six-clock reads. These cycle counts do not qualify 100 MHz operation because setup timing
fails. There are zero routing or DRC errors, 18 DRC warnings, and zero
unconstrained internal endpoints in the registered OOC context. The warnings
cover DSP pipelining and the OOC boundary/absent PS7. Board DDR and I/O remain
outside this context.

The original combinatorial population-count path disappears. Metadata node
arithmetic is now the worst routed path (18 CARRY4 plus two LUT3 levels).
The serial count saves 2,581 LUTs, 589 slices, and 454 registers versus B0; it
uses 400 fewer slices than the incremental candidate. Accept its longer diagnostic
and initialization latency because callback measurements are unchanged. The
serial candidate is selected as C1: its repeat reproduces every reported stage
resource, timing, and control-set metric exactly.
Deferred cleanup, fault, reset, and preemption regressions pass, including
DRAMsim3. The optimized and
routed raw archives preserve separate report stages and source/checkpoint hashes.
