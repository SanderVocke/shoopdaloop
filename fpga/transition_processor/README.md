# Logical transition processor RTL

A synthesizable, single-issue SystemVerilog implementation of the primitive-loop
`rt_fpga_reference::transitions::TransitionProcessor`. It owns logical loop clocks,
request arbitration and sync propagation. The existing state/job RTL executes
signal programs independently; wiring the processors together is a later step.

The control owner compiles a `transition_plan` using `fpga-reference compile`,
loads the resulting loop specifications/component membership, and prepares the
processor. No signal-graph traversal or SCC compilation happens in hardware.

## Supported behavior

- Stop, play, record, seek and retrigger for primitive loops.
- Absolute scheduling (rounded up to the quantum), delayed source pulses, repeat
  alignment and holding the playback endpoint while waiting for a source.
- Stable slot/generation loop IDs, retained request slots, sequence arbitration,
  cancellation/completion collection, and retirement of exhausted generations.
- Canonical timing components, legal sync cycles and self-sync, and at most one
  delivered pulse per loop per boundary.
- Transactional preview/retry, capture-unavailable and request-refused feedback,
  guarded commit/finish and checked 64-bit musical counters.
- Ordered transition/wrap/pulse journal, complete decisions and request readback.

Supported quanta are 1, 16, 32, 64 and 128 frames. Primitive content lengths and
play/seek positions are quantum aligned. Record means logical capture state;
media reservation and sample storage belong to the surrounding pipeline.
Composite kind and timeline-presence fields are explicitly refused with
`unsupported`. Triggers, program execution, request batches, media ownership,
capture monitoring and live topology replacement remain outside the profile.

## Structure

Loop state and request records each have two banks. Preview copies committed
records into the candidate bank and clears boundary scratch. The FSM offers due
absolute requests, then visits each compiled component in three phases:

1. Apply pending intents and natural boundaries in member order.
2. React to already delivered source pulses.
3. Deliver the lowest pending member pulse, react to its component followers,
   and repeat until no undelivered pulse remains.

It preflights clock advancement before declaring the candidate valid. A retry
starts again from the committed bank, so discarded candidates cannot consume
pulse delays, publish capture revisions or produce visible completions/events.
Commit switches the shared loop/request/journal bank selector atomically. Finish
advances committed positions/capture lengths once. It retains playback endpoints
as `position == length`; the next preview resolves wrap/repeat waiting.

Wide synchronous RAM rows favor a simple first implementation. The active and
candidate banks share each table's read/write ports. Reads and writes to the same
address never overlap; simulation asserts that property. The RAM's `no_rw_check`
attribute avoids synthesis emulation for a collision that the FSM does not use.
Small pulse/delivery/eligibility masks and refusal reasons remain prepared scratch.
No arithmetic divider, multiplier or DSP block is required.

Defaults are 64 loop slots, 64 request slots and 192 events per journal bank.
Logical capacities can be smaller, including holes in the loop slot table.
Event capacity must be at least `2 * loop_slots + request_slots`: one pulse and
one wrap per loop plus one transition per request. Runtime errors invalidate only
the candidate; the committed tables remain readable. Reset aborts work and
invalidates the configuration; reload a complete plan afterward.

## Command interface

One request/response may be outstanding. A command transfers when
`command_valid && command_ready`. Capture all inputs at that edge. While the FSM
runs, `busy=1` and no further command transfers. A response holds `response_valid`,
`response_error` and `response_data` stable until `response_ready=1` accepts it.
The response must retire before another command transfers. Errors return zero
data. Reset is synchronous, active high, and suppresses command acceptance.

The prototype uses an 8-bit opcode, 32-bit index, 640-bit command payload and
832-bit response payload. These are internal FPGA signals, not a board pinout or
an AXI peripheral. A bus bridge or the eventual pipeline coordinator can drive
this interface. Integers are unsigned, little field first, without sample data.

| Opcode | Command | Index / payload | Successful response |
| --- | --- | --- | --- |
| 0 | Status | Ignored | Status row |
| 1 | Write spec | Loop slot / 192-bit spec | Zero |
| 2 | Write member | Membership index / slot at bit 0, component-end bit 32 | Zero |
| 3 | Prepare | Five u32 fields: loop slots, request slots, event capacity, member count, quantum | Zero |
| 4 | Submit | Command fields in request-row layout | Request slot at bit 0, generation at bit 32 |
| 5 | Cancel | Request slot / generation in low u32 | Boolean cancelled in bit 0 |
| 6 | Collect | Request slot / generation in low u32 | Terminal request row, or zero if still pending |
| 7 | Clear observations | Ignored | Zero |
| 8 | Add observation | Observation row | Zero |
| 9 | Preview | Ignored | Frame at bit 0, serial at bit 64 |
| 10 | Commit | Frame/serial token | Zero |
| 11 | Finish | Frame/serial token | Zero |
| 12 | Read state | Loop slot | Committed loop row |
| 13 | Read request | Request slot | Committed request row, including empty-slot generation |
| 14 | Read decision | Loop slot / frame/serial token | Latest candidate row, or committed decision before finish |
| 15 | Read event | Journal index / candidate selector in bit 0 | Event row |

Write every logical spec slot, including zero rows for holes, and every member
entry before prepare. Prepare validates IDs, quantum alignment, sync-source IDs,
member bounds/uniqueness/coverage, final component termination and capacities.
The host must supply the canonical component order from the Rust compiler;
hardware does not independently reconstruct or validate the SCC partition.
Configuration writes and another prepare are refused after activation.

Submit/cancel/collect are allowed only between quanta. The first preview freezes
those inputs until successful finish, including after a failed preview. Load a
replacement observation list using clear/add, then preview again at the same
frame. Only the newest successful token can commit; only its committed token can
finish. Duplicate commit/finish and stale decision tokens are refused. State,
request and committed-journal reads remain available between commands. A stale
candidate's table bytes are inaccessible through the decision interface.

The observation buffer has `LOOP_DEPTH + REQUEST_DEPTH + 1` rows. Overlong input
sets a sticky overflow flag; preview returns `capacity` until observations are
cleared. This preserves whole-list rejection instead of applying a truncated list.
The logical profile allows at most `loop_slots + request_slots` observations.

Error codes are 0 success, 1 invalid_config, 2 capacity, 3 invalid_id,
4 duplicate_id, 5 invalid_state, 6 invalid_frame, 7 missing_sync, 8 exhausted,
9 empty_content and 10 unsupported. Loop/request IDs retain full 32-bit slot and
generation values on the command boundary; range checks precede RAM addressing.
Reserved payload fields should be zero.

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
reserved. A decision's playback-discontinuity is that bit AND active playback;
waiting-repeat means playing with nonzero content and position equal to length.

A request row has valid at bit 0, generation u32 at bit 1, after-ticks at bit 33,
operation at bits 34–35 (0 stop, 1 play, 2 record, 3 seek), retrigger at bit 36,
outcome at bits 37–39 (0 pending, 1 applied, 2 superseded, 3 cancelled, 4 failed),
and error reason at bits 40–43. Nine u64 fields start at bits 64 through 576:
target ID, sequence, requested frame, source ID, original skip, requested
position, effective frame, remaining ticks, outcome frame. An ID packs slot low
and generation high. Empty slots retain their next generation; zero generation
retires the slot. Terminal/absolute requests export null remaining ticks in JSON.

An observation row has kind at bit 0 (0 capture-unavailable, 1 request-refused),
ID at bit 1 and error code at bit 65. An event row has frame u64 at bit 0, loop ID
at bit 64, operation position u64 at bit 128, operation at bits 192–193, retrigger
at bit 194, and kind at bits 254–255 (0 transition, 1 wrap, 2 pulse).

Status has frame/serial at bits 0/64, phase at bits 128–129 (0 idle, 1 preparing,
2 preview, 3 committed), configured at 136, active bank at 137, active/candidate
event counts u32 at 192/224, and loop/request/event/member/quantum counts u32 at
256/288/320/352/384. Candidate count is meaningful only in preview/committed phase;
it equals committed count after commit.
`tests/codec.py` translates these rows into the existing JSON boundary types.

## Reproduce checks

Use the repository's development environment, Yosys and Verilator on PATH, and
Python dependencies from `requirements.txt` in a virtual environment:

```sh
cargo build --locked -p rt_fpga_testbench
python fpga/transition_processor/simulate.py --config default --random-cases 100
python fpga/transition_processor/simulate.py --config small --random-cases 10
python fpga/transition_processor/simulate.py --config minimum --random-cases 10
python fpga/transition_processor/simulate.py --config default \
  --case fpga/transition_processor/examples/wide_ids.json
bash fpga/transition_processor/check.sh
```

`--case` replays one `transition_case`/`transition_trace` with a single configuration.
`--waves` enables wave capture. Logs, failure documents, results XML and cycle
summaries go under `work/fpga-transition-sim`. The simulation-only wrapper uses
an internal timed clock to avoid Python callbacks on every idle engine cycle;
it is excluded from synthesis.

`examples/eight_tracks.json` saves the 8×8 workload. `examples/wide_ids.json`
additionally checks loop generations near u32::MAX and command sequences near
u64::MAX through recording, delayed playback, retriggered seek and stop.

The bench asks the Rust model for a trace, checks it against the native resolver,
and compares RTL decisions, state, retained requests, completions, ordered events
and post-quantum state exactly. It checks committed snapshots after every preview
and discarded retry. Directed coverage includes all supported modes/quanta,
arbitration, refusal, cancellation, slot reuse, off-grid scheduling, wide integers,
self-sync, legal cycles, repeat waiting, invalid IDs and observation overflow.
Protocol tests cover held responses, changed/busy command inputs, stale tokens,
phase guards, invalid configuration, reset during work and counter/generation
exhaustion. Exhaustion uses simulation-only hierarchical fault injection; no
production test-write interface exists.

The default seeded generator uses the same 100 graphs × 80 quanta as the Rust
regression. Small (7/11/25) and minimum (1/1/3) configurations additionally exercise
non-power-of-two RAM depths and the single-slot address case. The workload test
runs only where 64 loop/request slots fit and reports omissions explicitly.
`check.sh` runs warning-clean lint and generic no-latch/no-multiply/no-divide checks
for all three configurations, plus default ECP5 mapping and a resource JSON file.

## Measurements

Recorded validation with Verilator 5.032, cocotb 1.9.2 and Yosys 0.52 passed
177 cases across the three configurations and the wide-ID replay, totaling
10,047 compared boundary steps. This includes 9,600 seeded steps and both
64-loop workloads. The targeted Rust and Python suites passed 44 and 14 tests,
respectively, including the oracle adapter's observation-limit regression.

The default 64/64/192 ECP5 mapping uses 13,873 LUT4, 7,308 flip-flops, 1,073 CCU2C
carry cells, 59 DP16KD block RAMs and 8 distributed RAM cells, with no DSP blocks.
LUT4 excludes carry-cell logic; these are synthesis mapping counts, not placed
utilization or timing closure. The wide-row implementation favors clear control
and parallel row access over dense packing of short tables into BRAM.

At an assumed 150 MHz, preview + commit + finish measured:

| Workload | Quanta compared | Cycles per quantum | Estimated time |
| --- | --- | --- | --- |
| 8 tracks × 8 loops, record/play, source-delayed changes, repeat sync | 16 | 3,289–19,622 | 21.93–130.81 µs |
| One 64-member sync cycle, all active, wraps/retriggered seeks | 8 | 61,262–61,710 | 408.41–411.40 µs |

Both use 32-frame quanta. These counts include candidate copies, request scans,
settlement, preflight and logical advancement. They exclude loading a plan,
command/observation admission, completion collection, decision/journal readback,
response stalls and the state/job processors. The trace driver records individual
phase counts in `workload-summary.json`. No FPGA clock-frequency guarantee or
whole-pipeline deadline is established.
