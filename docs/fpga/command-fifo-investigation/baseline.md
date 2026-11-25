# Semantic command FIFO investigation baseline

## Scope and provenance

- Investigation branch: `investigate/fpga-command-fifo`.
- Production base: `ff6c09551e9dee2cd3a12d37ccad19fa844b09aa`.
- Investigation started 2026-11-25. Stage 2 user decision remains pending.
- No board access, feature removal, production-path replacement or deployment.
- Existing Vivado container: `docker.io/siliconbootcamp/xilinx-vivado@sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f`.
- Target for new measurements: `xc7z020clg400-2`, 100 MHz. Verify actual tool version/build and record new source hashes in each experiment.

The historical routed ABI-15 baseline is in
[`bulk-arming`](../../hardware/zturn-v2/2026-11-25/bulk-arming/README.md).
It reports 27,831 LUTs, 19,015 FFs, 88 RAMB36, 7 RAMB18 and 4 DSPs;
setup WNS +0.321 ns, hold WHS +0.027 ns. It is not a same-synthesis comparison
against the new prototype and does not implement a semantic command FIFO.
The archived hierarchy attributes 4,497 LUTs/1,949 FFs to `sp_engine`,
1,829 LUTs/1,680 FFs/37 RAMB36 to `host_dispatch`, and
2,640 LUTs/1,895 FFs to `host_mailbox` (including 856 LUTRAMs).
The dispatcher storage includes two snapshot banks and the ABI-15 patch buffers;
these costs cannot all be claimed as savings because the replacement also needs
observation and queue storage.

## Fresh baseline checks

Passed before changing any execution source:

```sh
nix develop --command cargo test -p rt_fpga_rtl --features sim,rt-interface \
  --test batch_controls --test batch_dispatch --test mailbox_faults \
  --test dispatcher_faults --test recovery --test recovery_pipeline --test host_mailbox
```

The local transcript is `work/cmd-fifo-investigation/baseline-tests.log`;
`baseline-tests.status` is zero. Earlier complete Linux-suite and physical results
remain dated historical evidence; they are not fresh results from this branch.

## Existing boundaries and capacities

- Shared immediate protocol: at most 32 commands/group, 256 software FIFO records,
  four staged application batches. Paired software queues reserve reply credit.
- Software observations are latest-complete publications, separate from reliable
  command responses; a hardware snapshot FIFO must preserve nonblocking rendering.
- Physical mailbox: one pending request and response, plus an ARM-drained event
  queue. Event overflow is an engine fault: replacing only command submission
  would not remove all ARM progress dependencies.
- ARM `Owner` consumes the semantic software group, resolves logical bindings,
  reads loop state, and applies commands through `ControlWindow`. The ABI-15
  future-only multi-transition path instead prepares and uploads state patches.
- Loop storage is 16 64-bit words/slot. The board geometry has 512 state/loop RAM
  capacity; the host compiler profile admits 128 loops. These distinct limits
  must not be confused when estimating snapshot or prototype RAM.
- At 48 kHz, quantum 32, 100 MHz: approximately 66,667 cycles/quantum, shared by
  command work, SP/JP/memory, publication and margin. The old control-window bound
  is 16,666 cycles, not proof of the future controller's measured allowance.

## Composite live-switch removal dependency map

Removal is planned only after the Stage 2 go; this inventory changes no behavior.

| Area | Responsibilities to remove or audit | Responsibilities to preserve |
| --- | --- | --- |
| `rt_fpga_compiler/src/loading.rs` | `CompositeCut`, parent/ancestor guard preparation, conditional future-switch/restart metadata, old/new ending reconciliation | Allocator leases, address reuse, inactive image preparation, ordinary primitive switches |
| `rt_fpga_reference/src/{rt_interface.rs,pipeline.rs,state.rs}` | Pending conditional composite selection, restart preview program swaps and special cut/retirement acceptance | Semantic command oracle, fixed schedules and runtime controls, fence correctness |
| `shoop_rt_host_reference/src/fpga/owner.rs` | `finish_future_switch`, future attempts, composite-wrap retry rules, cut readiness/readback | Binding/transport role, results, lifetime ownership, health/recovery |
| `rt_fpga_rtl/src/host/control{.rs,/window.rs,/session.rs}` | Cut readiness helpers, restart/retirement edits, special completion metadata | Ordinary edits as oracle evidence; loader/fenced ownership contract |
| SP and wrappers | Conditional `future_switch` preview/paired-source/handoff branches and plumbing | Static nested/shared composite processing, ordinary pulse/wrap/render logic |
| Host bridge/generated ABI/fixtures | Conditional-switch payloads, special retry/status handling; later old control-window/patch ABI | Safe expected refusals, reset incarnation, first fault, AXI recovery fence |
| Compiler/reference/RTL/host tests and docs | Live composite edit acceptance claims and dedicated machinery | Static composites, runtime commands, native/browser composite behavior and preserved historical evidence |
| Public service/capabilities | Live graph edits affecting any active/candidate composite, including indirect child edits | Explicit no-mutation refusal, initial/quiesced installation, primitive-only live updates |

The native feature-parity ledger was read. Its CPU/browser composite feature floor
is not authorization to delete those implementations or tests. This deliberate
regression applies to the FPGA-oriented profile only.

## Investigation evidence still required

The isolated kernel, differential cases, source manifests, measured cycles,
synthesis results, frontend overhead accounting and final recommendation remain
separate deliverables. Source-line counts alone are not FPGA cost estimates.
