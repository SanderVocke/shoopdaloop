# Historical verification and measurements

These notes describe the retired FPGA Rust reference and earlier RTL revisions.
Their commands, case formats and tool names are historical and are not current
reproduction instructions. Use [the current README](README.md) and
[production verification](../verification/README.md). Numerical measurements retain
their original workload and revision assumptions.

## Reproduce checks

```sh
cargo test --locked -p rt_fpga_reference -p rt_fpga_testbench
python fpga/transition_processor/simulate.py --config all --random-cases 10
python fpga/transition_processor/simulate.py --config default \
  --case fpga/transition_processor/examples/wide_ids.json
bash fpga/transition_processor/check.sh
```

The bench compares exact post-process state, requests, completions and ordered
events with Rust and the independent native resolver for successful cases.
Directed/seeded tests cover scheduling, canonical cycles, repeat waiting, wide
IDs, cancellation and request reuse. Protocol tests cover held responses,
changed/busy command inputs, unsupported obsolete commands, checked exhaustion,
sticky fault and reset during work. Default/small/minimum/wide profiles include
non-power-of-two sizes and priority masks crossing word boundaries. Public
64-loop workloads report process cycles and indexed work separately from musical
state. Hierarchical counter injection is test-only.

## Historical measurements before simplification

The following results describe earlier transactional implementations through
`23887b00`. Current processing has no preview/commit/retry protocol; matched
simplification measurements are in [SIMPLIFY.md](../../SIMPLIFY.md).

## Measurements

Before the shared read port, Yosys 0.52 mapped the indexed default 64/64/192
prototype to 15,648 LUT4,
7,745 flip-flops, 1,161 CCU2C carry cells, 60 DP16KD block RAMs, 8 distributed
RAM cells and zero DSPs. The scan-based mapping used 13,873 LUT4, 7,308
flip-flops, 1,073 carry cells and 59 block RAMs. LUT4 counts exclude carry
logic; these are mapped cells rather than placed utilization.
The connected shared-loop top's current mapping is recorded in
`fpga/loop_pipeline/README.md` at its own documented capacities.

Measurements below use the same programs and 32-frame quanta as the scan-based
prototype. At an assumed 150 MHz, preview + commit + finish measured:

| Workload | Scan-based cycles | Indexed cycles | Indexed estimated time |
| --- | --- | --- | --- |
| 8 tracks × 8 loops, record/play, source-delayed changes, repeat sync | 3,289–19,622 | 2,137–4,078 | 14.25–27.19 µs |
| One 64-member sync cycle, all active, wraps/retriggered seeks | 61,262–61,710 | 3,278–4,686 | 21.85–31.24 µs |

The peaks improve by about 4.8× and 13.2×, respectively. On steady empty-queue
cycle boundaries, request matching performs zero record checks and visits 64
actual follower edges. With 130 logical request slots, the same cycle takes
3,542–4,950 cycles: the increase is the linear copy of 66 additional rows,
without additional request matching. Diagnostic counts are in each preview's
`preview_work` entry in the cycle summaries.

Counts include candidate copies, indexed matching/settlement, preflight and
logical advancement. They exclude plan loading, command/observation admission,
completion collection, decision/journal readback, response stalls and the
state/job processors. No FPGA clock-frequency guarantee or whole-pipeline
deadline is established; ECP5 mapping counts are separate from placement and
timing closure.

Streaming the bulk passes and taking singleton/no-work fast paths further reduces
the same public workload cases, with the Rust model and packed interfaces
unchanged:

| Workload | Indexed cycles above | Streamed cycles | Estimated time at 150 MHz |
| --- | ---: | ---: | ---: |
| 8 tracks × 8 loops, record/play, delayed changes, repeat sync | 2,137–4,078 | 928–2,917 | 6.19–19.45 µs |
| One 64-member sync cycle, all active, wraps/retriggered seeks | 3,278–4,686 | 2,320–3,728 | 15.47–24.85 µs |

These remain single-preview + commit + Finish counts. The integrated replay in
`fpga/loop_pipeline/README.md` also measures retry-heavy quanta and actual media
jobs at 64 loops with 128 request slots.
The wide profile's 130-request cycle measures 2,386–3,794 clocks: its 66 extra
logical request rows now add 66 clocks instead of 264.

With streaming and the shared loop-read port, Yosys 0.52 maps the default
64/64/192 top to 16,927 LUT4, 7,750 flip-flops, 1,227 CCU2C carry cells,
60 DP16KD block RAMs, eight distributed RAM cells and zero DSPs. Streaming adds
no RAM ports or table storage. This mapping is separate from routed timing.
