# ARM–FPGA boundary completion evidence

Work started at `4b08728b17ab9d28b28d66fe2c78c92690ffd2ea` on
`plan/fpga-boundary-completion`, stacked on #993 at
`60beabb53e1f4a5051db9baa2e3f69815f61ad18`. The parent was fetched and
its PR head checked before execution. No parent changes were needed.
The [completion plan](../../PLAN_FPGA_BOUNDARY_COMPLETION.md) remains the
acceptance contract. This document records evidence, not roadmap completion.

## Frozen baseline

`work/fpga-completion/baseline-source-manifest.json` freezes SHA256 hashes for
97 RTL, service, simulator, application-worker and testbench source files.
Every TP RTL hash matches the preserved #993 routed source manifest in
`work/fpga-compact/m3-route-common/`. The committed
[M2–3 report](../transition_processor/measurements/COMPACT_ACTIONS_AND_MEMBERSHIP.md)
and its raw reports remain the baseline; an identical input does not require
another route just to change its date. Frozen implementation source and report
hashes will accompany each new checkpoint.

| Profile | Capacities / clocks | Role |
| --- | --- | --- |
| TP comparison | L64/R272/E400, xc7z020clg484-1, 100 MHz, 0.2 ns uncertainty | Matched M1/M2/#993 area, cycles and timing |
| Complete-core OOC | L64/R128/E256, program/state/jobs 512, 8 in/out, AXI64, xc7z020clg400-1, Q32, 100 MHz, 0.2 ns | Existing core area/timing comparison; separate package from ZedBoard |
| Kernel support | Q1/16/32/64/128; existing operation whitelist | Preserve all production-supported profiles |
| Transition loads | Dense shared pool, 8×8, 64-loop chains, stale and full terminals | Event/action/state/identity comparisons |
| Continuous I/O | Existing Q16/CF8/P8 default, 2–16 logical lanes; instant/fixed/DRAMsim3; alternate clock phases | Bank/CDC/deadline/sample and service-pause comparisons |
| Physical target | Initial reference ZedBoard XC7Z020, 100 MHz PL, Q32/48 kHz | Board revision/silicon, codec fixture and endpoint still unconfirmed |

Use Vivado 2021.1 build 3247384 / IP 3246043 and the preserved shared TP
context. The recovered runtime and pinned public image layers are described in
[the board catalog notes](../zedboard/CATALOG.md). The absolute local wrapper
is `work/fpga-boundary/tools/vivado/vivado-container` under this checkout.
Do not overwrite earlier frozen runs. New scratch/evidence starts in
`work/fpga-completion/`.

## Service/deadline inventory

| Current operation | Current path | Required disposition |
| --- | --- | --- |
| Command validation, graph compilation, request ownership, membership construction | Adapter service, shared transition owner, prepared Rust/Python producers | ARM service; complete before device admission |
| INSTALL, cancel, repeat; rounded effective frame and outcome | TP command FSM; synchronous transport used by callback today | Bounded device admission, then resident execution; retain authoritative results |
| Per-quantum fence generation and sequence matching | Prepared events, continuous source, demo ROM, board runner, audio control | Remove the empty-stream dependency; complete batch visibility at a device cut |
| PROCESS/FINISH, arbitration, pulse propagation, wrap/repeat and countdown | TP/coordinator | Fabric deadline work; no ARM continuation |
| State reads, pending request reads, completion collection | Application `Core::quantum`, mono `LoopAdapter::process` | Coherent retained device observations; asynchronous ARM delivery/collection |
| Host SETTLE 16, then state/media reads before audio | Both application and mono adapters | Remove only after device-side cut work and observations replace it |
| Capture completion snapshot/view acquisition | `Media::observe`, media opcode 1 once per channel | Required cut-time pin before a later retake can replace the source; move to device |
| Capture revision/frame tracking and latest/playback identities | `Media::observe` before/after processing | Device-stamped history; ARM reconstruction must tolerate missed service |
| Unreferenced content/view retirement | `Media::observe` / `retire`, media opcode 3 | Deferred cleanup; cannot be needed each quantum to keep ordinary playback running |
| Accepted state/export query cut and reference reservation | Adapter pending query and media export/read/release | Reserve historical state/media at the accepted cut; deliver later without substitution |
| Monitor mode and pre-recording changes | Application quantum reads pending actions then writes ramps | Device cut-time behavior, including the quantum before tick recording; no future ARM wakeup |
| Gain/balance/processor controls and graph activation | Scheduler delivery, parameter writes, `snapshot_parameters` | Resident timed controls; device establishes ramp continuity at actual activation, not from stale ARM state |
| Sample/job execution, capture storage, prefetch and reclamation | State/job/media/sample-memory pipelines | Device work; include DDR interference in deadline accounting |
| Model clock advancement and sample transport | Simulator/Marlin worker and continuous scheduler | One model owner; audio clock advances independently of command-service progress |
| ARM/audio transport | Zynq platform currently dummy audio; benchmark halts CPUs and disables GP0 | New real service/PL transport and codec integration required |

The synchronous `LoopDevice` trait is shared by mono and application adapters;
moving one callback's calls to a thread does not fix the other path or establish
device-cut media ownership. The existing continuous harness already provides
independent audio/fabric clocks and expected digital samples. It is the first
integration target; diagnostic stepped simulation remains distinct.

## Qualification envelope

Freeze the software comparison profiles above. The initial physical design target
is Q32/48 kHz/100 MHz with the existing fixed-latency choices preserved, at least
1,024 quanta with all command/observation/media service stopped, and a one-hour
endurance capture after smoke/correctness qualification. Require zero unintended
input loss, output deadline misses or faults, and at least 10% measured margin
to the actual publication cutoff in the frozen contention workload. Exercise
idle, sparse, dense, stale/full-credit, retained media, 8×8/64 chains, command
floods and concurrent ARM/cache/DMA/DDR traffic. Record maximum processing and
publication time, not only mean CPU utilization.

These numbers do not establish a board qualification contract yet. In particular,
48 kHz makes Q32 nominally 666.667 microseconds / 66,666.7 PL cycles, but this is
not the publication budget. Input quantum n is published on its last input
sample edge; output eligibility closes before sample `n*Q + Q + P`, two audio
clock edges before the first output sample. Account for descriptor synchronizers,
phase, prefetch and memory traffic between those edges. Measure the actual
cutoff and derive control/processing budgets and admission lead time before
claiming that a profile fits. Keep internal Q8.24 exactness separate from physical
codec word length, conversion and analog tolerance. Simulated tick spacing is a
test parameter, not a physical sample-rate measurement.

As of execution start no board/serial endpoint or audio fixture is configured in
the managed workspace, and no hardware-server process or serial device is
present. The user has been asked for an existing board/fixture endpoint. Old
Windows-local JTAG instructions and the halted-CPU benchmark do not establish
current access. Exact board/silicon/codec/pins/physical channel count, firmware
memory map, service CPU budget and contention bounds remain open until that
fixture is identified. Physical qualification and overall completion stay open.

## Focused resource review

The #993 shared-context synthesis reports 13,340 TP LUTs / 8,539 FFs / 28.5 BRAM36
equivalents before implementation; route reports 13,312 / 8,813 / 28.5. Hierarchy
attributes 8,845 synthesized LUTs to the processor body, 2,052 to loop RAM logic,
1,206 to request RAM logic, 542 to events, 540 to specs, 116 to dependency order
and 39 to the two membership/root bank wrappers. Hierarchy alone cannot divide
the processor body's shared mux/control logic into independent protocol costs.

Both membership node banks contain `(32+H)*R` bits and both root banks contain
`2*H*L` bits, where `H=ceil(log2(R))+1`. Staging/active masks each retain R bits.
Resident requests are 397 bits per slot. The generation comparisons protect
reused slots; checked publication/nonce values protect bank lifetimes; staging
counts and presence checks protect complete publication. None can be removed
merely because the ARM is trusted. Read/return-state muxes, 64-bit frame math and
wide RAM writeback share logic with resolver execution and require controlled
experiments for attribution.

The first controlled variant restores the old numeric encodings of the 66
retained FSM states without restoring unused states, selectors, ports or guards.
It leaves the ABI and all storage/lifetime rules unchanged. Inputs, assigned
values and synthesis provenance are frozen in `work/fpga-completion/encoding-*`.
The [raw reports and replay patch](../transition_processor/measurements/boundary-completion/encoding/)
preserve the experiment; the baseline source manifest is stored alongside them.

| Same-context synthesis | LUTs | FFs | BRAM36 | Estimated setup/hold slack |
| --- | ---: | ---: | ---: | ---: |
| #993 current encoding | 13,340 | 8,539 | 28.5 | +1.120/+0.063 ns |
| Old retained-state numbers | 13,461 | 8,523 | 28.5 | +0.946/+0.063 ns |

Decision: discard the variant and retain #993 unchanged. It increases LUTs by
121 (0.91%); it does not justify routing or a production encoding change.
The original prototype's smaller mapping cannot be recovered just by restoring
these numbers. This experiment does not prove the entire M3 increase came from
banks/guards, nor quantify a routed saving. No guard, check or lifetime rule was
removed. All protocol, construction, transfer, traversal and storage costs stay
as reported at #993; its functional and routed evidence remains applicable.
Stage 2's conditional protocol cutover, candidate campaigns and route are not
applicable. The focused review is complete and does not block M4.
