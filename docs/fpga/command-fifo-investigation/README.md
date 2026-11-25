# Command FIFO investigation — Stage 2 decision report

**Recommendation: GO, with the ceilings and reduced scope below. User decision:
PENDING. Production implementation remains blocked.**

The ordinary command handler is practical as a separate shared sequential kernel.
The measured prototype is not a gigantic loop scheduler: it has 18 controller
phases, one working loop record and shared arithmetic. Removing composite live
switching avoids importing graph-dependent readiness/restart machinery. The
remaining frontend, snapshot and loader integration is real work, not a measured
finished implementation.

## Scope and reproducibility

See [baseline and removal dependencies](baseline.md), the root
[`FPGA_CMD_FIFO.md`](../../../FPGA_CMD_FIFO.md), and [measurement summary](results.json).
This branch changes only investigation documentation, experimental RTL, its runner
and a feature-gated test. The production owner, ABI-15 bridge, SP, compiler,
firmware and board remain unchanged. No bitstream was generated or board accessed.

Sources:

- `fpga/experiments/command_fifo/edit_kernel.sv`: unconnected edit kernel.
- `fpga/experiments/command_fifo/probe.sv`: synthetic local loop/state RAM fixture,
  not a proposed production configuration interface.
- `src/rust/rt_fpga_rtl/tests/command_kernel.rs`: differential oracle and cycles.
- `fpga/experiments/command_fifo/measure.py`: frozen-source synthesis/OOC route.

Reproduce in the repository development environment:

```sh
nix develop --command cargo test -p rt_fpga_rtl --features sim,rt-interface \
  --test command_kernel -- --nocapture
nix develop --command python3 fpga/experiments/command_fifo/measure.py \
  work/cmd-fifo-measurement-NEW --run
```

Export the report files and regenerate the checksums after running the test and
measurement (the collector verifies current RTL against the frozen inputs):

```sh
nix develop --command python3 fpga/experiments/command_fifo/collect.py \
  work/cmd-fifo-measurement-NEW docs/fpga/command-fifo-investigation
```

The measurement output directory must not already exist. Vivado uses the pinned existing
container, no network, `xc7z020clg400-2`, 100 MHz and 1 ns input/output delays.
The frozen source/Tcl hashes, image digest and working-tree base are in
[reports/provenance.json](reports/provenance.json); the tool identification and
launcher digest are in `results.json`. Reports retain their original tool dates.
The public evidence manifest checksums the archived results. The local full run
is `work/cmd-fifo-investigation/vivado-v2/`; checkpoints remain local.

## 1. What was actually mapped

The CPU chain is `Owner::command_lane/apply` → `edit_loop` → `ControlWindow` →
packed-state helpers → reference semantic helpers. Remote snapshot reads,
`BEGIN/END_CONTROL`, patch sorting/guards/rebasing, logical searches, upload and
reclamation bookkeeping are not part of the hardware semantic kernel.

| Retained command | Fields/operations | Bound and refusal behavior |
| --- | --- | --- |
| Transition | Mode in word 0; cursor/length/capture/cycles/revisions in words 2–7; two plan flags in word 1 and plan words 10–15; newer-slot bit 56 | Normalize self-sync requests; validate immediate operation before live writes; visit exactly two slots; reject impossible plans/empty playback; checked revision/frame increment |
| Clear-plans | Selected validity bits 32/37 of word 1 | Two conditions; on composite/child records clear pulse plans only, preserving absolute commands |
| Repeat-sync | Bit 35 of word 0 | One masked bit update |
| Gain | Constant 129-bit ramp value in its separately bound state slot | ARM retains finite-value validation and state-independent Q8.24 conversion; FPGA performs the state write |
| Composite iteration/seek | Word 6 unit geometry, current mode/length; replace plan 0 with an absolute operation, invalidate plan 1 | Checked u64 × u32 product, 32 serial multiply steps, range/seek-mode checks; invalid arithmetic refuses without writes |

All commands validate physical target identity. `source_self` is immutable
binding metadata supplied by ARM, not a CPU observation of current transport.
Frame and current loop state are execution-owned inputs. No ordinary command
walks the other loops, sync ancestors, media data or graph topology.

The prototype reads one 16-word record, edits local working state and writes the
record only on success; gain writes its state slot instead. It deliberately
writes unchanged words too: this is a simple measurable upper bound, not an
optimized minimum-traffic implementation. Scratch is shared between commands;
there is no 32-target shadow-state bank. It does not provide group rollback.

Primitive immediate Stop/Play/Record operations overlap existing SP operation
logic. Reusing it could save some logic but would entangle the existing SP FSM
and command refusal behavior. **Keep the first implementation separate**: the
measured cost is small enough not to require speculative sharing or a soft CPU.
A ROM-sequenced alternative is not justified by this result.

## 2. Measured resources and timing

Vivado **2021.1 SW Build 3247384**, target `xc7z020clg400-2`. Counts below are the
utilization report's packed LUT totals, not the number of LUT primitive objects.
The local RAM probe includes 512 129-bit state slots in every geometry.

| Experiment | Stage | LUTs | FFs | RAMB36 | DSP |
| --- | --- | ---: | ---: | ---: | ---: |
| Kernel only | Synthesis | 1,798 | 1,976 | 0 | 0 |
| Kernel only | OOC routed | 1,783 | 1,976 | 0 | 0 |
| Kernel + 16-loop RAM | Synthesis | 2,055 | 1,976 | 3 | 0 |
| Kernel + 32-loop RAM | Synthesis | 2,068 | 1,977 | 3 | 0 |
| Kernel + 128-loop RAM | OOC routed | 1,916 | 1,975 | 6 | 0 |
| Kernel + 512-loop RAM | Synthesis | 2,114 | 1,979 | 18 | 0 |

The logic does not scale by duplicating controllers per loop. The BRAM growth is
loop storage, which the production SP already owns; do not add a second copy of
this fixture's RAM when integrating the kernel.

OOC routed experiment summaries at 100 MHz:

| Experiment | Setup WNS | Hold WHS |
| --- | ---: | ---: |
| Kernel only | +1.239 ns | +0.112 ns |
| 128-loop local RAM probe | +1.083 ns | +0.149 ns |

**Limit:** these are isolated OOC experiments, not board closure or a maximum
clock-frequency claim. Vivado warns about unanchored OOC ports/clock source;
external-interface delay/skew/routing is not characterized as in the integrated
board. The bare kernel's worst reported path ends at its memory-data output;
the RAM-connected probe's worst reported path is internal phase-control fanout
to the product register reset. Both DRC reports contain the expected missing-PS7
warning; no critical warnings/errors were reported. Integration must remeasure
registered RAM boundaries, high-fanout controls and the whole placed design.

Port/config/readback/result paths remain externally driven/observable, and the
probe infers actual BRAM. This is not a constant-fed kernel optimized away by
synthesis. The 1,976 FFs include working-record/plan/request storage and counters;
most area is datapath/storage selection, not 18 state bits.

## 3. Correctness and cycle measurements

**3,970 deterministic cases passed** against the existing packed-state/reference
helpers. Coverage includes:

- Primitive, composite and composite-child records; each mode, self/external
  sync, all combinations of absent/Stop/Play/Record in now/two future slots.
- Skip counts, capture finalization, stale unused plan words/reserved flags,
  empty content, frame/revision overflow, iteration boundaries and checked
  multiplication (including 512 additional generated multiply cases).
- Clear/repeat/gain, unsupported commands, invalid target validity/generation,
  memory request stalls, retained replies and a 32-command repeated-target trace.
- 16/32 distinct targets with skip 0/1 and one supplied common frame.

Fresh validation also passed the 17 baseline RTL cases, the new differential test,
two reference rendering tests and one Marlin endpoint test (21 Rust tests total).
Formatting, test-attribute policy, register/top generators, Python syntax checks
and the warning-denying workspace build passed. The full Linux/Wasm suites were
not rerun for this unconnected prototype: production behavior is unchanged; those
integration gates remain in Stage 6. No historical full-suite result is relabeled
as a fresh investigation result.

The first measurement runner stopped because it used a collection helper not
available in Vivado Tcl. The corrected `vivado-v2` run completed every case; the
failed initial attempt remains local and contributes no accepted result.

The initial differential run caught a composite-record edge case: its capture
length is interpreted as zero by the reference even if the packed capture word
is nonzero. The corrected kernel and frozen synthesis inputs pass the cases.

With an always-ready local RAM and one-cycle responses:

| Successful command | Kernel work cycles | At 100 MHz |
| --- | ---: | ---: |
| Transition, including two plans | 73 | 0.73 µs |
| Clear/repeat | 65 | 0.65 µs |
| Gain | 35 | 0.35 µs |
| Composite iteration/seek | 99 | 0.99 µs |
| 16 transition targets, skip 0 or 1 | 1,168 summed cycles | 11.68 µs |
| 32 transition targets, skip 0 or 1 | 2,336 summed cycles | 23.36 µs |

The multi-target numbers sum independent kernel invocations. They **exclude**
acceptance/response turnover, FIFO decoding and scheduler overhead; they are not
an implemented atomic group, measured FPGA hold time or continuous-audio test.
The longest successful path is 99 work cycles; 32 such commands imply 3,168 work
cycles before frontend overhead. State-machine bounds (16 reads, at most 32
multiply steps, two plan slots, at most 16 writes) support this fixed-memory
bound; finite testing is not a formal proof over every 64-bit input.

## 4. Proposed frontend and costs not yet measured

Candidate hardware ABI, to freeze only after approval:

- Incarnation, group sequence, expected revision, count and optional deadline in
  a fixed header; up to 32 commands. Use fixed records (budget 48 bytes each)
  for semantic arguments and bound physical target/generation/state address.
  Preserve per-command invalid-value results during stateless lowering (for
  example a nonfinite gain), rather than dropping a command or changing group
  semantics. The prototype tests fixed gain operands, not a float decoder.
- Four complete group banks; ARM fills an unpublished bank and publishes one
  descriptor only after writes/barriers complete. Never fetch unfinished payload.
- Reserve one complete result packet at admission: group/header outcome plus up
  to 32 per-command result codes. ARM may retain original command records for
  response formatting; execution never needs an ARM callback or acknowledgement.
- Select one complete group per quantum from a fixed publication cutoff. Extra
  groups wait in FIFO order; deadlines are tested at the eventual attempt frame.
  **This is a proposed explicit service-policy change from the current Rust
  drain-at-cutoff loop, not an existing guarantee; align the reference after go.**
- A separate two-bank complete snapshot stream. Skip publication when full;
  sequence/loss counts expose this. Required sync-event effects accumulate locally,
  rather than depending on ARM event draining. No telemetry-full engine fault.
- Separate low-priority inactive-program/new-state writes. Ordinary primitive
  switches remain ordered semantic commands with real ownership fences.

Storage arithmetic (estimates, not synthesis results):

| Addition/replacement | Payload size | Budget implication |
| --- | --- | --- |
| Four command banks | 4 × (64 + 32 × 48) = 6,400 bytes | Approximately 2–4 RAMB36 depending on banking/width |
| Four compact reply packets | 4 × (64 + 32 × 4) = 768 bytes | Approximately 0.5–1 RAMB36; credits kept separately |
| Two raw 128-row snapshot banks | 2 × 128 × 128 = 32 KiB, plus extra gain/failure/header fields | Approximately 8–10 RAMB36; preserve coherent settled phase |
| Keep full 512-row snapshot extent instead | 128 KiB raw for two banks | Approximately 32–34 RAMB36 including extra fields |

The current `program_limits()` admits 128 loops and `Allocator` selects physical
slots from `0..limits.loop_slots` (`loading.rs`, allocation loop near line 917).
Thus this profile's allocations, including preparation overlap, remain below 128
even though the board has 512 physical RAM slots. A 128-row snapshot extent is
valid for that explicitly enforced profile. Preserve and test that bound during
integration; a different profile admitting higher physical addresses must use a
bounded row list or budget the full extent. Do not silently reduce an advertised
hardware limit.

The current dispatcher uses 37 RAMB36: two raw 512-row snapshot banks plus the
512-entry patch and computed-value buffers. The patch buffers occupy about five
RAMB36 in this organization and are unnecessary for execution-owned semantic
commands. New queues can reuse that capacity. Snapshot storage is **replacement**
capacity, not all free savings. Likewise the old mailbox/dispatcher total 4,469
LUTs/3,575 FFs, but loader, diagnostics, transport and publication logic still need
replacement. No measured net whole-board area reduction is claimed.

Planning estimate: the kernel plus packet/credit logic, snapshot scheduling,
AXI handling and loader arbitration should fit a **4,000–6,000 LUT frontend**.
Only the ~1,800-LUT kernel is measured. Removing composite preview/handoff code
may recover SP/compiler complexity, but its exact net LUT saving is unmeasured
and is not used to justify the recommendation. The generic primitive memory
fence and static-composite runtime machinery must not be removed with it.

## 5. Reduced capability matrix

| Operation | Proposed scope |
| --- | --- |
| Install a prepared static composite program into an empty/quiesced engine | Retain |
| Execute its existing nested/shared schedules and ordinary runtime controls | Retain; no new composite modes |
| Primitive-only live program replacement | Retain, including state continuity and fencing |
| Live edit/introduction/removal with a composite in active or candidate program, including child changes | Refuse before mutation; remove special implementation |
| Replace a composite-containing graph after explicit quiescence/ownership handoff | Retain via explicit lifecycle, not automatic stop/retry |
| Native CPU/browser composite live editing | Unchanged; outside this regression |

Delete FPGA-oriented cut/ancestor guard preparation, conditional restart preview,
paired-source/ending handoff and CPU retry machinery, not merely its capability
flag. The dependency inventory in `baseline.md` shows this crosses compiler,
reference, RTL SP/wrappers, adapters, ABI and fixtures. Ordinary SP wrap/pulse
execution is not equivalent to live-switch reconciliation and remains.

## 6. Proposed ceilings for user approval

These are **recommendations, not approved limits or already-demonstrated system
results**. Return to the user if integration cannot meet them.

| Budget | Proposed ceiling |
| --- | --- |
| Shared semantic kernel | 2,500 LUTs, 2,500 FFs, no DSP; at most one scratch BRAM if beneficial |
| Replacement frontend, excluding existing SP/JP/media and persistent state RAM | 6,000 LUTs, 7,000 FFs |
| Whole protected board design | 32,000 LUTs, 24,000 FFs, 100 RAMB36-equivalents (RAMB36 + RAMB18/2), at most the existing 4 DSPs |
| Command phase, one group of up to 32 commands | 4,096 cycles including FIFO decode/turnover under the specified local RAM service bound |
| Snapshot phase | 8,192 cycles when publishing up to 128 rows; decimated/skipped without ARM waits; revisit if full 512-row capture is required |
| Total admitted quantum | At most 60,000 cycles at 100 MHz for Q=32/48 kHz, approximately 10% margin below the shortest quantum interval |
| Integration timing | Nonnegative routed setup and hold slack at 100 MHz; no critical implementation errors |

The total-quantum ceiling leaves at most 47,712 cycles for SP/JP, required memory
ordering and other work when both command and maximum proposed snapshot phases
occur. Their **integrated worst-case has not been measured here**. Define the
admitted graph/memory-service envelope and test it; arbitrary DDR stalls cannot
be promised a finite successful-render bound. Existing watchdog/fault behavior
must remain independent of command/reply reader progress.

The initial sequential kernel reads all sixteen words even for gain and flag
edits. Optimizing those paths or word storage is an option if needed, not a
prerequisite assumed in the budget. Do not chase a smaller FSM at the expense of
large parallel datapaths or renewed CPU dependencies.

## Decision requested

**Approve proceeding with Stages 3–7 under the reduced capabilities, one-group-per-
quantum policy and ceilings above, or choose no-go/revise?**

The evidence supports the edit-kernel approach. The main unresolved risks are
coherent snapshot capture at the settled SP point, preserving the profile's
physical-allocation bound, nonblocking memory-port arbitration, and reliable
program handoff.
They must be verified in integration; this report does not claim those are solved.
Until the user decides, no composite feature is deleted, no production FIFO is
implemented and no board test is performed.
