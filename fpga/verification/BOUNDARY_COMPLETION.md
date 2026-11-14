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

The user confirmed that no ZedBoard is available here and instructed execution
to defer actual hardware tests as long as possible, then stop at that dependency.
No board/serial endpoint or audio fixture is configured in the managed workspace,
and no hardware-server process or serial device is present. Old
Windows-local JTAG instructions and the halted-CPU benchmark do not establish
current access. Exact board/silicon/codec/pins/physical channel count, firmware
memory map, service CPU budget and contention bounds remain open until that
fixture is identified. Complete independent work first; physical qualification
and overall roadmap completion stay open. No further access request is pending.

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

## Prepared-profile autonomous control checkpoint

[Control ABI 1](../audio_io/CONTROL.md) replaces the prepared live fence stream.
A nonempty batch contains a versioned header and all of its operations before
execution begins. Empty/future/unfinished ingress leaves audio runnable. One
complete due batch may execute between quanta, with a device watchdog bounding
the window. Late admission uses the actual device frame; the TP still owns
rounding, dynamic validation and transition outcomes.

The device reserves every transport reply before starting a batch. Full retained
replies defer new control work without backpressuring audio; held results remain
stable until acknowledgement. Ordinary command refusals are retained replies,
while malformed transport/timeout/processing faults retain fault-to-silence.
A direct host reply pending at arm is preserved and blocks new control admission,
not PROCESS/FINISH. Direct configuration/sample/media handshakes are setup and
diagnostic interfaces with audio disabled; live traffic uses bounded batches.

Rust and Python prepared producers load frame-zero admissions in boot, emit only
nonempty future batches and stop at the end of the ROM. The continuous wrapper,
demo and unpaced board runner use the same batch protocol. The normal application
and mono adapters still require the later migration; this checkpoint does not
claim callback separation or autonomous application media ownership.

The [raw checkpoint directory](../audio_io/measurements/autonomous-control/)
contains compressed complete sample/state/outcome/media/publication reports,
source hashes, generic statistics and test logs. Each pause case runs 1,040
quanta and compares all 16,648 stereo sample frames against independent production
Rust. Future absolute actions, self/source sync, repeat waits, a retake and stop
cancellation execute from resident requests; all 11 terminal results are retained.
The host status reply stays unchanged. Runtime assertions reject ingress,
collection or accepted direct service traffic while audio runs; hostile held
legacy requests are presented throughout the pause.

| Memory | Audio clocks/frame | Processed | Control transfers | Missed/discarded |
| --- | ---: | ---: | ---: | ---: |
| Instant | 256 | 1,040 | 0 | 0/0 |
| Stalled fixed latency | 256 | 1,040 | 0 | 0/0 |
| DRAMsim3 with stalls | 512 | 1,040 | 0 | 0/0 |

These are Q16/CF8/P8 simulations with a 10 ns core clock and 13 ns audio clock,
not physical 48 kHz measurements. The reports retain every publication edge;
derive simulation margins from the recorded last-input/prefetch edges. Existing
continuous historical comparisons pass for coincident clocks, stalls and output
overrun. Demo input-loss/memory-fault/reset recovery passes in all three memory
models. Board-runner simulation, odd-depth credit/backpressure/refusal/timeout
tests, ROM parity, Q16 strict lint/generic synthesis, warning-denying workspace
build and the Rust test-attribute policy pass.

Default ingress remains 128×746 bits (11,936 bytes). The new result array is
128×1,088 bits (17,408 bytes), plus its synchronous head register and control
state. Each padded ingress transfer is 24 u32 words; each result is 34. The
watchdog/quota are containment and arbitration bounds, not physical deadline
qualification. TP RTL, resident storage and the #993 TP resource checkpoint are
unchanged. Native mapped/routed wrapper accounting and full-core timing closure
remain open; generic cell counts are not FPGA LUT counts.

M4 is not complete. Larger live graph updates still need inactive image
publication and activation-time ramp continuity. Required media pinning,
historical queries and monitoring must move into device cuts before removing
normal SETTLE or migrating application service. The final native/application
matrix is intentionally reserved for the final behavioral milestone.

## M4 live-image and combined functional gate

The prepared ABI-1 checkpoint above is historical. The current contract is
[control ABI 2](../audio_io/CONTROL.md), including inactive signal images and
resident actions managed by the shared Rust owner. Complete-body preflight
rejects invalid/overbudget batches before mutation. A registered admission
decision is valid only for the current device sequence. Result capacity is
reserved before dispatch; no held reply or incomplete producer blocks audio.

A staged image contains the whole program, initial state and live-copy map.
Ordered uploads leave the active image untouched. At the admitted activation
cut, all old mutable values are copied before any active rewrite, supporting
permutations without publishing a stale ARM snapshot. Ramp changes read the
actual device value at their cut and retain their result. Private action rows
are admitted only by complete ordered-bank publication; moving-cut refusal is
retried by the shared owner, with uncertain outcomes quarantined.

The [combined evidence](../audio_io/measurements/resident-images/combined/)
freezes 173 sources and all 12 passing continuous/demo/image/board tests. It
includes the three-memory 1,040-quantum pause, exact resident transition/media
comparisons, held legacy results, full transport credits, unfinished ingress,
command flood, fault recovery and prepared-ROM parity. The live 512-program /
512-state / 512-copy case matches all 34,340 stereo frames, including activation
at frame 2,048 and channel ramp changes at 2,560. The independent floating bound
is 5.9582816902548075e-8; the smallest simulated publication margin is 118.742 us.
This uses accelerated clocks, not a physical 48 kHz measurement. The flood runs
160 quanta with all 128 results held and a backpressured source. No quantum is
missed or discarded. Strict lint/generic mapping passes Q16/Q32/Q64; ECP5 mapping,
warning-denying workspace build and test-attribute policy pass. M4's functional
gate is complete, while M5 application and final timing/physical gates remain.

[Timing experiments](../synthesis/measurements/boundary-completion/) retain exact
sources, matched harnesses and raw reports. The TP reply change routes at 13,078
LUTs / 8,773 FF / 28.5 BRAM36, setup/hold +0.236/+0.008 ns against #993's
13,312 / 8,813 / 28.5 and +0.105/+0.007 ns. These are single matched runs, without
a variation study or a claim about final system savings. Full-core runs improve
the original -1.845 ns miss but do not close timing; the latest archived run is
-0.351/+0.005 ns. The first autonomous-wrapper route is -2.105/+0.005 ns; later
registered admission/action timing work is measured separately. No clock,
capacity or constraint exception hides these misses. An unbounded Yosys proof
covers the reclaimer budget mux; DMA/metadata tests cover all original memory
models, alternate widths/geometries, odd cache depths and retirement races.

Required capture pinning, historical queries, monitoring and application service
separation still belong to M5. The final broad native/application gate remains
reserved for that behavioral milestone. There is no ZedBoard here: complete
independent software, RTL, firmware, build and timing work, then stop before
physical tests. The overall roadmap remains incomplete without physical evidence.

## M5 prerequisite: device-side capture ownership and coherent reads

The [device-cut checkpoint](../loop_pipeline/measurements/device-cuts/) moves
capture finalization, retake roots and required reference cleanup into the device
for the supported one-loop mono/stereo capture profile. It observes full TP state
after PROCESS and after FINISH. Completed takes retain their actual physical
roots; new recordings receive fresh roots. Logical channel mapping protects
completed roots and raw host media mutation is disabled in this profile. The
general kernel profile retains its existing raw interface and loop capacity.

All three memory modes match 33,600 stereo frames and 2,100 coherent cuts over
1,050 quanta, with no host TP/media/observation service after initial admission.
Modes, positions, lengths, revisions, capture counts, cycles and pulses match
production Rust. Component tests cover retained export and historical empty
identities, allocation stalls, counter exhaustion and reset. Long-take component
finalization is bounded without slicing all references. The real near-capacity
two-take application case is still required after application migration.

The independent TP observer reads complete loop/request/event rows with their
actual frame and phase; a private resident command/result port permits future
fabric delivery/collection independently of a held host reply. They share the
existing resolver/RAMs and preserve execution/SP priority. General-profile ties
remove unused private ports. Five TP capacities with 20 seeds pass the observer
checkpoint; directed resident-port tests also pass. A separate retained-record
RAM component passes full 256-credit storage, 1,024-quantum retention, exact
historical reads, checked generation/epoch ACK and reset during partial capture.

Combined media/demo/board tests, strict general/capture lint, Q32 generic
synthesis, formatting, test policy and warning-denying workspace build pass.
The source overlay and complete reports/logs preserve both success and initial
harness failures. Public action/query delivery, export bulk reads, real session
epoch wiring, callback/worker separation and application/mono migration remain
open. The optional cut pulse alone is not an accepted historical query. M5 is
not complete, and this checkpoint does not advance physical qualification.
