# Logical transition processor RTL

A synthesizable, single-issue SystemVerilog implementation of the primitive-loop
`rt_fpga_reference::transitions::TransitionProcessor`. It owns logical loop clocks,
request arbitration and sync propagation. The existing state/job RTL executes
signal programs independently. `fpga/loop_pipeline` coordinates them through
preflight, commit, shared loop reads, job completion and finish.

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

Loop state and request records each have two banks. Their pending-request
indexes share those banks: per-loop heads and per-request next/previous links
are copied with the musical state. A pair of pending-absolute masks is published
with the shared bank selector. Terminal requests are unlinked immediately while
retaining their record slots for completion collection. Reused slots are inserted
in slot order under their new target and timing kind.

Prepare tags component membership and builds canonical, component-local follower
chains from the uploaded specs and member order. These immutable links remove
follower discovery from quantum execution. Cross-component source pulses are
observed when visiting the later component.

Preview copies committed records into the candidate bank and clears boundary
scratch. It then:

1. Pops pending absolute requests from a priority mask and offers those due now.
2. Applies intents and natural boundaries in canonical component/member order,
   then reacts to source pulses already delivered by earlier components.
3. Selects the lowest ready loop ID with a balanced priority encoder, delivers
   its pulse and walks its prepared follower chain. Each follower visits only
   its own pending source-timed request list.

The pending lists use storage linear in loop/request capacity; there is no
loop-by-request matrix. Stop cancellation walks the target's two lists. Indexed
refusal maps handle both feedback lookup and duplicate detection. Empty lists
bypass request matching, and singleton components reuse their loaded state for
source reaction. Newly ready lower IDs retain canonical priority; each loop can
deliver only one pulse per boundary.

The FSM preflights clock advancement before declaring the candidate valid. A
retry restores the committed indexes and musical state. Commit switches banks
atomically. Finish advances committed positions/capture lengths once and retains
playback endpoints as `position == length`; the next preview resolves wrap/repeat
waiting. The full linear copy/preflight/finish passes remain. Admission still
checks retained sequence IDs and finds a free request slot.

Wide synchronous RAM rows favor a simple implementation. The active and candidate
banks share each table's read/write ports. Reads and writes to the same address
never overlap; simulation asserts that property. The RAM's `no_rw_check` attribute
avoids collision emulation. A separate dispatch edge flushes state writes before
list-head read/modify/write. Small pulse/delivery/eligibility masks and refusal
reasons remain prepared scratch. No divider, multiplier or DSP block is required.

Defaults are 64 loop slots, 64 request slots and 192 events per journal bank.
Logical capacities can be smaller, including holes in the loop slot table.
Event capacity must be at least `2 * loop_slots + request_slots`: one pulse and
one wrap per loop plus one transition per request. Runtime errors invalidate only
the candidate; the committed tables remain readable. Reset aborts work and
invalidates the configuration; reload a complete plan afterward.

## Command interface

The shared loop read port transfers `loop_read_valid && loop_read_ready` with a
32-bit slot. It borrows the existing RAM port while the engine is idle in the
committed phase, with no outstanding command/response. Commands have priority.
The following cycle pulses `loop_response_valid`; `loop_response_error` reports
an out-of-range logical slot without accessing a truncated address. Successful
data packs valid at bit 0, generation u32 at 1, mode at 33, length u64 at 35 and
position u64 at 99. The state consumer checks presence and full generation.
No response backpressure is required for this synchronous read port.

The coordinator prevents transition commands and Finish during state/job
execution, so every node reads the same committed loop bank. `configured_quantum`
reports the prepared quantum, or zero before configuration. The standalone
comparison wrapper ties the shared request port inactive.

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
reserved except for private pending-list heads described below. A decision's
playback-discontinuity is that bit AND active playback;
waiting-repeat means playing with nonzero content and position equal to length.

A request row has valid at bit 0, generation u32 at bit 1, after-ticks at bit 33,
operation at bits 34–35 (0 stop, 1 play, 2 record, 3 seek), retrigger at bit 36,
outcome at bits 37–39 (0 pending, 1 applied, 2 superseded, 3 cancelled, 4 failed),
and error reason at bits 40–43. Nine u64 fields start at bits 64 through 576:
target ID, sequence, requested frame, source ID, original skip, requested
position, effective frame, remaining ticks, outcome frame. An ID packs slot low
and generation high. Empty slots retain their next generation; zero generation
retires the slot. Terminal/absolute requests export null remaining ticks in JSON.

The architectural request response remains 640 bits. Internal records append
next/previous pointers at offsets `640` and `640 + H`, where
`H = max(1, ceil(log2(REQUEST_DEPTH))) + 1`. Each pointer uses H bits; all ones means
no entry. Per-loop absolute/source-timed heads use H bits at offsets 37/804 in the
existing 832-bit loop row. Those heads survive preview's transient-field clearing.
They are private metadata, like the intent/protected fields, and do not appear in
JSON snapshots.

The uploaded spec remains 192 bits; internal spec RAM is 256 bits. Prepare places
follower-head/next pointers at offsets `192` and `192 + L`, using
`L = max(1, ceil(log2(LOOP_DEPTH))) + 1` bits each, with an all-ones sentinel.
Component ID follows at `192 + 2*L`, using `max(1, ceil(log2(LOOP_DEPTH)))` bits.
These links are derived at preparation, so the upload protocol and compiled-plan
JSON image remain unchanged.

An observation row has kind at bit 0 (0 capture-unavailable, 1 request-refused),
ID at bit 1 and error code at bit 65. An event row has frame u64 at bit 0, loop ID
at bit 64, operation position u64 at bit 128, operation at bits 192–193, retrigger
at bit 194, and kind at bits 254–255 (0 transition, 1 wrap, 2 pulse).

Status has frame/serial at bits 0/64, phase at bits 128–129 (0 idle, 1 preparing,
2 preview, 3 committed), configured at 136, active bank at 137, active/candidate
event counts u32 at 192/224, and loop/request/event/member/quantum counts u32 at
256/288/320/352/384. Candidate count is meaningful only in preview/committed phase;
it equals committed count after commit.
Three u32 diagnostic counts at offsets 416/448/480 report request-matching
checks, component-local follower visits and delivered pulses for the latest
preview. Request matching excludes bulk copying and command admission. These
counts remain separate from musical state and JSON comparison values.
`tests/codec.py` translates architectural rows into the existing JSON types.

## Reproduce checks

Use the repository's development environment, Yosys and Verilator on PATH, and
Python dependencies from `requirements.txt` in a virtual environment:

```sh
cargo build --locked -p rt_fpga_testbench
python fpga/transition_processor/simulate.py --config default --random-cases 100
python fpga/transition_processor/simulate.py --config small --random-cases 10
python fpga/transition_processor/simulate.py --config minimum --random-cases 10
python fpga/transition_processor/simulate.py --config wide --random-cases 10
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
regression. Small (7/11/25), minimum (1/1/3), and wide (130/130/390) configurations
additionally exercise non-power-of-two depths, the single-slot case, high
request-slot reuse, and sparse ready IDs across priority-word boundaries. The workload test
runs only where 64 loop/request slots fit and reports omissions explicitly.
The wide profile also runs the 64-loop cycle with 130 logical request slots;
empty queues must still report zero matching checks and 64 follower visits.
Linked-list head/middle removal, slot reuse, arbitration and discarded retries
have explicit directed cases.
`check.sh` runs warning-clean lint and generic no-latch/no-multiply/no-divide checks
for all four configurations, plus default ECP5 mapping and a resource JSON file.

The commands above passed with Verilator 5.032 and cocotb 1.9.2: 216 cases and
11,081 quantum boundaries across the four configurations and wide-ID replay,
including 130 seeded graphs × 80 quanta. Transport/negative tests also passed.
The corresponding Rust tests (50) and Python tests (14) passed.

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
