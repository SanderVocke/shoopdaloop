# Logical transition processor RTL

Simulation now uses the complete [Rust/Marlin testbench suite](../../src/rust/rt_fpga_testbench/README.md):

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- transition
```

The Python simulator entry names are aliases for this runner and accept its
current options. The native suite guide describes profiles, replay, VCDs and
reports. Historical measurements are marked separately.

This synthesizable primitive-loop controller is compared directly with the
production Rust loop resolver. The connected
[loop pipeline](../loop_pipeline/README.md) adds state, jobs and media execution.

## Processing contract

One `PROCESS` operation resolves commands, synchronization, natural wraps,
arbitration and ordered events, then advances clocks by one quantum. Supported
quanta are 1, 16, 32, 64 and 128 frames. IDs carry full 32-bit slots/generations;
installed actions retain completion records until collected. ARM owns request
allocation and duplicate-sequence admission through the reusable Rust transition
service; the fabric installs an explicitly addressed action. Absolute and source-delayed
timing, self-sync, acyclic sync chains, repeat waiting, stop cancellation,
record/play/seek and checked counters retain their normal semantics. Composite
kinds/timelines remain reserved and unsupported.

Loop, request and event tables each have one authoritative bank. The
per-loop pending lists and a flat dependency order preserve canonical ordering
without whole-table request searches. Each quantum clears only transient fields,
settles each loop once, sources before dependents, and advances clocks. Each loop
checks whether its source emitted a pulse. External-source requests react before
the loop's pulse is emitted; self-sync requests react afterwards. A pulse is
emitted at most once per loop per quantum. There are no observation memories,
candidate copies, serial tokens, retry, or atomic bank switch. Ordinary reads
occur between processing operations. A required processing failure latches the
first fault; partial tables/events may be read for diagnostics, but execution
cannot resume until reset and configuration reload. Admission refusals remain
separate from processing faults.

For `INTEGRATED=1`, the loop coordinator privately separates settlement from
advancement so the state processor can consume stable settled records while jobs
execute. Command 9 settles and private command 11 advances after successful jobs.
The public loop-pipeline interface provides one run/completion operation.
`INTEGRATED=0` is the standalone default and command 9 performs both phases.

## Command and shared-read interface

One request/response may be outstanding. A command transfers on
`command_valid && command_ready`; all inputs latch at that edge. While busy no
new command transfers. Response data/error/valid remain stable until
`response_ready` accepts them. Reset is synchronous, active high. Inputs use an
8-bit opcode, 32-bit index, 640-bit payload and 832-bit response; these are fabric
signals, not a finalized ARM register map. Errors return zero payload.

| Opcode | Command | Index / payload | Successful response |
| --- | --- | --- | --- |
| 0 | Status | Ignored | Status row |
| 1 | Write spec | Slot / 192-bit spec | Zero |
| 2 | Write order | Index / loop slot in low u32; next u32 must be zero | Zero |
| 3 | Prepare | Six u32: loops, requests, events, ordered loops, quantum, ABI=3 | Zero |
| 4 | Install | Request slot / compact fields, generation and committed publication | Slot u32 at 0, generation u32 at 32 |
| 5 | Cancel | Slot / generation low u32 | Cancelled bit at 0 |
| 6 | Collect | Slot / generation low u32 | Terminal request, or zero |
| 9 | Process | Ignored | Zero |
| 12 | Read state | Loop slot | Current loop row |
| 13 | Read request | Request slot | Compact cell, including empty generation |
| 15 | Read event | Journal index | Current event row |
| 18–21 | Membership BEGIN/NODE/ROOT/COMMIT | See [COMPACT.md](COMPACT.md) | Nonce or zero |

Opcodes 7, 8, 10 and 14 are unsupported. Opcode 11 is private to integrated use
and refused in standalone mode. Configuration loads all spec slots (including
holes) and the compiler's flat `dependency_order` before Prepare. Prepare validates
bounds, IDs, alignment, sources, unique coverage and that each distinct source
appears before its dependent. This rejects multi-loop cycles and reversed
dependencies; a loop may still name itself as source. The compiler determines
the canonical order between independent loops. Install/cancel/collect require idle
phase. A fault prevents control
mutation; status/readback remain available. Reset is required to reconfigure.

The ARM service preserves public submission, capacity, duplicate detection and
request ownership. INSTALL checks the selected empty slot's generation and all
target/source/frame guards and committed membership generation. It
does not search for a free slot or duplicate sequence. See
[the installation contract](INSTALL.md) for ownership, error precedence, prepared
streams and the distinction between public submission and raw fabric traffic.

Codes: 0 success, 1 invalid_config, 2 capacity, 3 invalid_id, 4 duplicate_id,
5 invalid_state, 6 invalid_frame, 7 missing_sync, 8 exhausted, 9 empty_content,
10 unsupported. Bounds checks precede RAM address narrowing.

The synchronous shared port transfers on `loop_read_valid && loop_read_ready`
only while settled; held host replies do not gate this independent read path. A response pulses in
the following cycle without backpressure. The **164-bit** record packs valid at
0, generation u32 at 1, mode at 33, length u64 at 35, position u64 at 99, and
capture-start at 163 (`capture_frames == 0`). The consumer validates generation
and mode. The coordinator prevents transition mutation while state/jobs use this
port. `configured_quantum` is zero until Prepare succeeds.

## Packed rows

A spec has valid at bit 0, generation u32 at bit 1, source-present at bit 33,
source slot/generation at bits 34/66, repeat-sync at bit 98, kind at bits 99–100
(0 primitive, 1 composite), timeline-present at bit 101, and initial length u64
at bit 128. Other bits are reserved. Loop slots come from the table address.

A loop row has valid at bit 0, generation u32 at bit 1, mode at bits 33–34
(0 stopped, 1 playing, 2 recording), repeat-sync at bit 35 and last-pulse-present
at bit 36. Eight u64 fields start at bits 64, 128, 192, 256, 320, 384, 448, 512:
content length, position, capture frames, cycles, content revision, capture
revision, revision watermark, last-pulse frame. Boundary metadata starts at bit
576: intent-present, intent slot u32 at 577, intent sequence u64 at 609,
protected-present at 673, protected sequence u64 at 674, origin-present at 738,
origin slot/generation u32 at 739/771, and discontinuity at 803. Other bits are
reserved. Membership roots live in separate publication banks.

The public reconstructed request has a 640-bit representation. Device action and
result cells use 397 bits; Rust retains originals and reconstructs requested/source
fields from authoritative compact results. Membership uses separate immutable,
double-buffered generation-qualified nodes and roots. The exact wire fields,
publication opcodes, storage and traversal bounds are in [COMPACT.md](COMPACT.md).
There are no device insertion/unlink states or request next/previous writebacks.

Spec RAM is 192 bits per loop. The dependency-order RAM stores one loop slot per
entry using `max(1, ceil(log2(LOOP_DEPTH)))` bits. There are no component tags,
component boundaries or follower links. Settlement retains one delivered-pulse
bit per loop and two pulse flags for the current loop. A pending sync list is
visited once per quantum, so requests need no separate eligibility bitmap.

An event row has frame u64 at bit 0, loop ID at bit 64, operation position u64
at bit 128, operation at bits 192–193, retrigger at bit 194, and kind at bits
254–255 (0 transition, 1 wrap, 2 pulse).

Status has frame u64 at 0, fault code at 64 (4 bits), phase at 128 (2 bits:
0 idle, 1 executing, 2 settled, 3 faulted), configured at 136, event count u32 at
192, and loop/request/event/ordered-loop/quantum counts u32 at 256/288/320/352/384.
Three u32 diagnostics at 416/448/480 report request checks, follower visits and
pulse deliveries for the latest process operation. The follower-visit diagnostic
counts source-pulse reactions, including self-sync. ABI revision 3 is at bits
144–159. Bank versions are acknowledged by publication commands. Reserved fields return zero. `../abi/transition.py` decodes these rows.

## Verification

Full musical/state/media behavior is compared with the production float and
Q8.24 runtime via [common sessions](../verification/README.md). Static lowering
uses production signal and timing compilation. There is no software FPGA executor.
Standalone tests preserve hardware-specific protocol, configuration, stream,
cache, capacity, fault and reset checks without a duplicate looping backend.

```sh
cargo run --locked -p rt_fpga_testbench --features sim -- transition --random-cases 20
```

The subsystem's `check.sh` retains lint and synthesis entry points. Current
coverage and local evidence are recorded in [the retirement ledger](../verification/MIGRATION.md)
and [subsystem gates](../verification/subsystem-gates.json). [Historical results](HISTORY.md)
retain the earlier reference-backed measurements; their commands are retired.

Measure the transition processor alone at the 64-loop deployment capacities:

```sh
python fpga/transition_processor/measure.py --vivado /path/to/Vivado/bin/vivado \
  --stage route --output work/vivado-transition
```

This uses the ZedBoard `xc7z020clg484-1`, 272 request slots, 400 events and a
registered 100 MHz context. Each run freezes RTL, harness and constraint hashes,
records capacities and compiler version, and reports core-only LUTs, flip-flops,
BRAM equivalents and setup/hold slack. Use `--source` with frozen parent RTL to
compare revisions under the same parameters.

The [acyclic synchronization experiment](measurements/ACYCLIC_SYNC.md) records
matched resource measurements, processing cycles and verification evidence.
The [ARM-owned request-slot experiment](measurements/ARM_REQUEST_SLOTS.md) records
the indexed-installation boundary, matched admission costs, resources and timing.

## Application adapter extensions

Command 17 updates repeat-sync policy between iterations. Index selects the loop;
payload bits 31:0 contain its generation and bit 32 the new value. A stale identity
or non-idle phase is refused; the loop state row is authoritative.

The submitted-request encoding reserves bit 44 for the application's empty-content
replacement. It uses operation 3 and carries the new revision in the position
field. Submission bypasses position alignment for this tag. Settlement refuses a
recording target; otherwise it selects zero length, resets position, stops the
loop and advances the revision watermark. Existing request ordering, supersession
and terminal receipts apply. The emitted transition event marks bit 195 and uses
its position field for the revision, distinguishing this operation from seek.
Samples/reference retirement stays with the media subsystem and owning leases.
