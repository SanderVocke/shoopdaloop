# ARM-owned request slots

Measured on 2026-11-13 against `5b7dd0a5b242884f86862991dc7b456a5b091df7`.
Candidate RTL is committed in `2e19b3d5` after merging upstream `2bdd10c9`.
The merged RTL was routed again and reproduces the earlier cleanup result exactly.
[Machine-readable results](arm-request-slots.json) include source hashes,
tool provenance, capacities, command samples and report hashes. The accompanying
[evidence directory](arm-request-slots/) retains timing, utilization, routing and
DRC reports, the shared harness/constraints/Tcl and command samples.
Published text reports remove trailing whitespace only; JSON records hashes of
both the original local reports and the committed copies.

The Rust owner now selects request slots and checks retained sequences. Indexed
INSTALL removes the FPGA allocation and duplicate scan. Resident actions still
resolve in FPGA, including during a 1,024-quantum pause of the Rust service.
Admission is faster, and this layout-preserving step is nearly area-neutral:
13 more routed LUTs, 103 fewer flip-flops and unchanged BRAM. It establishes the
boundary needed for later record and list changes.

## Matched transition processor implementation

Both runs use Vivado 2021.1 SW Build 3247384, `xc7z020clg484-1`, integrated
64-loop/272-request/400-event TP, four threads, a 10 ns clock and 0.2 ns
uncertainty. RTL was frozen independently for each run. The harness, constraints
and Tcl hashes match exactly; the frozen RTL hashes match the revisions above.
These are TP-only resource counts within the registered timing context.

| Routed TP metric | Baseline | ARM slots | Change |
| --- | ---: | ---: | ---: |
| Slice LUTs | 11,297 | 11,310 | +13 (+0.12%) |
| Flip-flops | 8,402 | 8,299 | -103 (-1.2%) |
| BRAM36 equivalents | 29 | 29 | 0 |
| DSPs | 0 | 0 | 0 |
| Setup slack | +0.131 ns | +0.103 ns | -0.028 ns |
| Hold slack | +0.006 ns | +0.006 ns | 0 |

Both meet the standalone 100 MHz target. Synthesis uses 11,208 versus 11,287
LUTs, 7,979 versus 7,886 flip-flops, and 29 BRAM36 equivalents each.
No capacity or packed record width was reduced: loops remain 832 bits, requests
660 bits and events 256 bits at this deployment size.

The scan reused one comparator and RAM port over many cycles; removing it does
not remove a comparator per slot or any RAM. Most datapath and record-update
logic remains. An initial implementation copied the installed slot/generation
into redundant registers and used 12,075 synthesis / 12,039 routed LUTs.
Reusing the already-latched command identity reduces synthesis by 788 LUTs and
57 flip-flops, and routing by 729 LUTs. Wide mux/control mapping changes amplify
the effect of this small cleanup; the registers alone do not account for that
many LUTs. Synthesis hierarchy reports are retained for inspection.

A controlled variant preserving the remaining states' original binary values
uses 12,206 synthesis LUTs instead of 12,075. Preserving state values alone does
not recover the increase, and that variant is not adopted. The JSON retains
both investigations. Larger structural savings need narrower records and simpler
record/list update paths; they are later milestones rather than a claim here.

All 18,258 baseline and 18,085 candidate routable nets are fully routed, with zero
routing errors. Each DRC report has zero errors/critical warnings and the same
two context warnings: unloaded capture registers and an omitted PS7 block.

The earlier acyclic synchronization report used a different baseline and run.
Its numbers are not substituted for this fresh matched pair. Complete-core
timing was not rerouted for this milestone; its previously recorded -0.074 ns
setup miss remains an unresolved qualification item. Passing TP timing and
integrated functional regressions does not establish full-core or board timing.

## Command and processing cycles

The same native Marlin measurement runs against frozen baseline and current RTL,
using physical depths 64/R/400, one configured logical loop and Q=32. Eight
repeated empty-list iterations give identical cycle counts. Measurements run
from command acceptance to response availability, excluding response hold time,
host execution and configuration. Candidate public refusals issue no FPGA command.

| Operation/workload | R=11 baseline | R=11 ARM slots | R=272 baseline | R=272 ARM slots |
| --- | ---: | ---: | ---: | ---: |
| Empty-list submit / INSTALL | 59 | 16 | 1,103 | 16 |
| CANCEL, one pending action | 11 | 11 | 11 | 11 |
| COLLECT, applied or cancelled terminal | 4 | 4 | 4 | 4 |
| PROCESS, one ready action | 44 | 44 | 44 | 44 |
| READ_STATE | 4 | 4 | 4 | 4 |
| STATUS | 1 | 1 | 1 | 1 |
| Full-pool refusal | 50 | 0 | 1,094 | 0 |
| Retained-terminal full refusal | 50 | 0 | 1,094 | 0 |
| Last INSTALL into a dense per-loop list | 102 | 59 | 2,190 | 1,103 |
| PROCESS, dense list | 245 | 245 | 5,465 | 5,465 |

The 272-slot empty-list admission improves by 68.9 times. Dense insertion still
traverses the per-loop list; it remains variable work for a later milestone.
Duplicate rejection samples and all dense insertion samples are retained in the
JSON evidence. Collection frees the lowest reusable slot; cancellation and
uncollected terminal records keep their credits.

The 64-loop workloads also reproduce every baseline PROCESS count at both
64/64/192 and 64/272/400 physical capacities: eight tracks of eight loops take
920–2,621 cycles per quantum, and the active 64-loop source chain takes
1,033–2,569. Q=32; these exclude command service, observation and SP/JP work.
The JSON retains the complete matched cycle sequences.

Rust metadata occupies 1,321 bytes at R=11 and 24,289 bytes at R=272 for these
one-loop images on the native 64-bit host. This counts the owner and allocated
registry/image storage, excluding allocator overhead and transport buffers.
Measured Rust `begin` median/max is 2.194/7.952 microseconds at R=11 and
68.543/815.870 microseconds at R=272. `finish` median/max is 0.150/18.157 and
0.136/3.656 microseconds respectively. Samples mix installs, cancellation,
collection and refusals, including the growing dense list. They use an
unoptimized native dev build with debug information disabled, on a shared host,
and exclude JSON parsing/serialization and IPC. They are characterization samples,
not ARM or audio-callback deadline guarantees. The service runs outside the
intended audio callback; qualification of the physical ARM remains future work.

Port payload widths are unchanged: INSTALL and CANCEL use 80-byte payloads,
and COLLECT returns 104 bytes. These counts exclude opcode/index and handshakes;
this milestone does not claim bus bandwidth savings or a physical ARM register map.

## Verification evidence

- All five TP capacities pass directed/protocol groups and 20 seeded cases each
  (8,000 seeded quanta). Raw guard tests cover invalid index, occupied pending or
  terminal slots, zero/stale generation, non-idle phase and non-mutation.
- The shared Rust service tests cover credit/sequence retention, validation
  precedence, rollback, uncertain replies, stale identities, generation
  retirement and finite prepared streams. The primitive kernel codec is broader
  than the deliberately unchanged application whitelist.
- Marlin passes the simulator and regular-host application suites. A dedicated
  test finishes installation acknowledgements, makes no service calls for 1,024
  quanta, compares audio and final state/events, then compares retained
  completions, frames and captured media. A separate delayed-install test checks
  authoritative FPGA rounding after audio advances. Oracle delivery boundaries
  are explicit; no resolver outcomes are used to prepare actions.
- The common production/RTL pipeline passes 45 scenarios with faults, exact
  Q8.24 comparison and the retained float-accuracy gate. The integrated 64-loop
  benchmark also passes with AXI64, 32-chunk blocks and deterministic stalls.
  An additional 19-scenario run passes at the odd capacities with AXI64,
  32-chunk blocks, stalls and faults.
- Continuous 16-channel fabric audio passes with Q=32/P=16, AXI64 and stalls.
  Generated AXI/audio/board wrappers, prepared demo ROM checks and 64-loop
  ZedBoard workload preparation pass. CI exercises both instant and DDR timing
  models, prepared recovery and all seven audio synthesis configurations.
- All five TP capacities pass Verilator lint and generic Yosys synthesis; the
  default capacity also passes ECP5 mapping. Rust formatting, test-attribute policy
  and the warning-denying workspace build pass.
- After the redundant-register cleanup, all five TP profiles, the 45-scenario
  pipeline corpus with stalls/faults, both Marlin boundary tests, bounded
  application replay, both command measurements and lint/mapping pass again.
- After regular-host integration and the protocol/diagnostic-pipe fixes, the
  serialized full native suite passes all 3,261 tests, with 11 skipped. The
  detached-host shutdown regression also passes ten consecutive repetitions.
  Earlier intermediate failures remain in the local logs; they are superseded
  by this clean full sweep.
- Upstream `2bdd10c9` replaces cocotb with the native `rt_fpga_testbench` matrix.
  Both wire drivers and both native/Python prepared-stream generators use the
  shared Rust request owner. All five migrated TP capacities pass seven groups
  plus 20 seeded cases each, and both command depths reproduce every prior
  baseline/current cycle count. The complete migrated matrix runs separately
  from the native sweep above. The expanded matrix exposed a Marlin teardown
  hang: exported-scope destructors consulted another model's current context.
  Selecting the owned context before model destruction fixes it; both the
  focused overlapping-model regression and the complete AXI backend matrix pass.


Local detailed logs and build/checkpoint artifacts are under
`work/fpga-boundary/`. CI evidence is attached to
[PR #990](https://github.com/SanderVocke/shoopdaloop/pull/990).
The installation and lifecycle contract is documented in [INSTALL.md](../INSTALL.md).
Pending lists, terminal records, held-response coupling, fabric fences and the
application settlement pause remain for subsequent roadmap milestones.

## Reproduce

Freeze baseline `fpga/transition_processor` from the revision above into a
separate directory. Install Verilator and Rust, then run:

```sh
python fpga/transition_processor/measure_commands.py \
  --source work/baseline/fpga/transition_processor --requests 272 \
  --output work/slots-baseline-commands
python fpga/transition_processor/measure_commands.py --requests 272 \
  --output work/slots-candidate-commands
python fpga/transition_processor/measure.py --vivado /path/to/vivado \
  --source work/baseline/fpga/transition_processor/rtl --stage route \
  --output work/slots-baseline-route
python fpga/transition_processor/measure.py --vivado /path/to/vivado \
  --stage route --output work/slots-candidate-route
```

Repeat command measurements with `--requests 11`. Use Rust 1.100.0, Verilator
5.032, Marlin 0.21.1 and `CARGO_PROFILE_DEV_DEBUG=0` for comparable host samples;
cycles do not depend on host wall time. The command wrapper launches the Rust
runner and detects the explicitly supplied frozen legacy source. Full commands,
source manifests and runner hashes are recorded in the JSON. The migrated Marlin
measurements reproduce all earlier cocotb cycle counts; published host samples
come from Marlin and exclude JSON transport. The measurement runner is a child subreaper so it also
works in containers whose PID 1 does not reap detached Vivado workers.

Vivado was recovered from the public `siliconbootcamp/xilinx-vivado:stable`
image, digest
`sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f`.
The verified original vendor layer is
`sha256:187ac31317b800d0b7782a3aff7fdedd200a27303a22121d7c3a6a623b36bf26`
(5,729,231,096 compressed bytes). Extract that layer directly for `/tools`,
and the two original runtime layers listed in the JSON for Ubuntu 20.04 and
its dependencies. Later image overlays are excluded; vendor files, including
`base.tcl`, are unmodified. The optional IP catalog is absent; this primitive TP
flow does not use it.

For these measurements the extracted runtime ran as uid/gid 1000 in a small
local container, with the repository mounted at the same path, `/tools` mounted
read-only, networking disabled and the original runtime's
`LD_PRELOAD=/lib/x86_64-linux-gnu/libudev.so.1`. No full vendor image installation
is needed. Native execution on the newer host was not used for the final pair.
