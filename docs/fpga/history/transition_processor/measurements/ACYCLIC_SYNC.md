# Acyclic synchronization experiment

Measured on 2026-11-13, relative to `bd031cbf` on
`proto/fpga-rust-reference`. Machine-readable results, source hashes, compiler
provenance and validation counts are in [acyclic-sync.json](acyclic-sync.json).

Functional FPGA checks pass and area decreases. Whole-core timing remains short
of 100 MHz by 0.074 ns, and six native host-startup checks fail. This experiment
does not establish a timing-qualified 100 MHz complete core.

## Transition processor

Vivado 2021.1 SW Build 3247384, `xc7z020clg484-1`, integrated mode,
64 loops, 272 requests and 400 events. Both runs use identical registered
timing context, constraints, Tcl, capacities and `general.maxThreads=4`.
The earlier routed baseline's three RTL hashes match the parent revision.

| Routed TP resources | Before | Acyclic | Saving |
| --- | ---: | ---: | ---: |
| Slice LUTs | 12,624 | 12,279 | 345 (2.7%) |
| Flip-flops | 9,056 | 8,365 | 691 (7.6%) |
| BRAM36 equivalents | 29 | 29 | 0 |
| DSPs | 0 | 0 | 0 |

Both meet 100 MHz with 0.2 ns clock uncertainty. Setup/hold slack changes from
+0.179/+0.007 ns to +0.064/+0.047 ns. Setup margin decreases. All routable nets
are routed with zero routing errors. The two existing context DRC warnings
remain: retained capture registers have no loads, and the context omits PS7.

The controller has 78 declared states, down from 97. It settles each loop once
in source-first order, then handles self-sync after emitting that loop's pulse.
Prepare rejects a distinct source appearing after its dependent, which also
rejects multi-loop cycles. Component boundaries, component tags, follower lists,
the pulse-priority queue and the per-request eligibility bitmap are removed.

The LUT saving is modest because the wide records and their update logic remain:
832 bits per loop, 660 bits per request at these capacities, and 256 bits per
event. Spec rows shrink from 256 to 192 bits, and order rows from 64 to 6 bits.
Both layouts still allocate the same number of fixed-size FPGA RAM blocks.

## Processing cycles

These compare identical successful 64-loop workloads at 32-frame quanta in the
default 64/64/192 simulation profile. Counts cover Process, excluding admission,
configuration, observation and the state/job processors.

| Workload | Before min–max | Acyclic min–max |
| --- | ---: | ---: |
| Eight tracks of eight loops | 861–2,914 | 920–2,621 |
| 64-loop source chain | 1,295–2,893 | 1,033–2,569 |

Peak costs fall by 10.1% and 11.2%, respectively. The lightest eight-track
quantum costs 6.9% more cycles.

## Complete core

This comparison uses the parent RTL and this branch under the same
`loop_pipeline_axi` timing context, with only transition processor RTL changed.
These runs use `xc7z020clg400-1`, 64 loops, 128 requests, 256 events, AXI64 and
32-chunk allocation blocks; their capacities differ from the standalone
transition processor measurement above.

| Standard-flow routed core | Before | Acyclic | Saving |
| --- | ---: | ---: | ---: |
| Slice LUTs | 26,533 | 25,518 | 1,015 (3.8%) |
| Flip-flops | 18,623 | 17,962 | 661 (3.5%) |
| BRAM36 equivalents | 56 | 56 | 0 |
| DSPs | 4 | 4 | 0 |

The parent meets 100 MHz with +0.003/+0.005 ns setup/hold slack. The acyclic
version has -0.074/+0.005 ns and misses the setup target. Two additional matched
trials start from the original routed checkpoints and use identical commands
for both versions:

| Additional trial | Parent setup slack | Acyclic setup slack |
| --- | ---: | ---: |
| `phys_opt_design -directive AggressiveExplore`, then default routing | +0.003 ns | -0.148 ns |
| Routing only, `route_design -directive Explore` | +0.003 ns | -0.168 ns |

Neither trial closes timing. The standard-flow result remains the best measured
acyclic result and supplies the resource figures above. The worst standard-flow
path runs from media-store slot count to media-store phase control; routing
accounts for 7.176 ns of its 9.715 ns data path. A secondary violating path ends
at the TP response-data register enable. Timing closure remains necessary.

## Validation

All five transition profiles pass six RTL test groups each: default, small,
minimum, wide and deployment. Together they cover 8,000 seeded quanta, directed
Rust comparisons, 64-loop workloads, held responses, cancellation, reset,
capacity and checked faults. Raw configuration tests reject reversed source
orders, duplicate/missing entries and two- and three-loop cycles, and accept
self-sync and source orders opposing slot order.

Also passed: 19 common production/RTL pipeline scenarios with deterministic AXI
stalls and allocation blocks; fatal-fault silence/hold/reset; the integrated
64-loop benchmark; continuous 16-channel fabric audio with Q=32 and P=17;
generated AXI wrapper checks; 64-loop ZedBoard workload generation; 10 Rust
adapter/fixture tests; six Python contract tests; 10 Marlin integration tests;
five-capacity Verilator lint; Rust formatting and test-attribute policy; and the
warning-denying Rust workspace build.

The full native suite ran serially with two CPUs reserved: 3,224 passed,
six failed and 11 were skipped. All six also failed in a quiet rerun on four
CPUs with no Vivado jobs. Five failures are LAN profile-readiness deadlines
at 15 seconds; the sixth is the 10-second host-startup deadline in
`bounded_recovery_stress_preserves_recording_and_reclaims_departed_connections`.
The JSON evidence lists the affected tests. Driver switching, UI recovery,
transport carriers, resolver/topology and FPGA-specific tests passed.

## Reproduce

```sh
python fpga/transition_processor/measure.py --vivado /path/to/vivado \
  --stage route --output work/vivado-acyclic-tp
python fpga/transition_processor/simulate.py --config all --random-cases 20
python fpga/synthesis/run.py --vivado /path/to/vivado --output work/vivado-acyclic-core
```

For the baseline, supply parent RTL through the transition measurer's `--source`
option, or run the core synthesis runner in a parent checkout. Keep capacities,
clock constraints and harness hashes identical when comparing results.

The full-core runner exits unsuccessfully for the measured acyclic version's
100 MHz miss, after preserving the reports and routed checkpoint. The additional
trials are [extra-physical-pass.tcl](extra-physical-pass.tcl) and
[explore-routing.tcl](explore-routing.tcl). Run each in its own fresh directory
containing `input.dcp`, copied from the standard run's `post_route.dcp`:

```sh
vivado -mode batch -source /path/to/repo/fpga/transition_processor/measurements/extra-physical-pass.tcl
vivado -mode batch -source /path/to/repo/fpga/transition_processor/measurements/explore-routing.tcl
```

Each writes its reports, timing JSON and checkpoint into that working directory.
