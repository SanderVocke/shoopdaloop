# FPGA state processor interchange

The Rust boundary types define a versioned JSON format for programs, instruction
lists, state snapshots, jobs, compiler results/bindings, transition requests and
state writes, and execution results. `rt_fpga_testbench` generates the schema and
runs the actual standalone Rust state processor. Python validates and transports
these values without implementing the processor again.

This is the data layer for RTL comparison drivers. It defines architectural
values, not a packed instruction encoding, physical RAM layout, or cycle model.

## Build and check

Run Cargo in the repository's development environment. Install Python dependencies
in a local virtual environment if they are not already available:

```sh
python3 -m venv work/fpga-testbench-venv
. work/fpga-testbench-venv/bin/activate
python3 -m pip install -r scripts/fpga_testbench/requirements.txt
cargo build -p rt_fpga_testbench
cargo test -p rt_fpga_reference -p rt_fpga_testbench
python3 -m unittest discover -s scripts/tests -p test_fpga_testbench.py -v
```

The Python suite uses `target/debug/fpga-reference` (or `.exe` on Windows).
Set `SHOOP_FPGA_REFERENCE` when using another target directory.

`schema.json` is generated from Rust using Schemars, with explicit integer bounds
added to its integer formats. Regenerate it after changing boundary definitions:

```sh
target/debug/fpga-reference schema > scripts/fpga_testbench/schema.json
```

Both suites check that the published schema matches the Rust generator. The
cross-language tests round-trip every boundary variant and integer limits through
the Rust executable and Python, and check rejection of malformed documents.
When a format changes incompatibly, bump `FORMAT_VERSION` and regenerate the
schema; saved documents with other versions are rejected.

## Values and representation

Every document has `format_version: 1` and a `payload` with `kind` and `value`.
Payload kinds are `graph`, `compiled`, `program`, `instructions`, `state`, `jobs`,
`bindings`, `state_writes`, `on_wrap`, `status`, `case`, `trace`, `audio_case`,
`audio_trace`, `media`, `media_case`, `media_trace`, `transition_plan`, `transition_plan_image`,
`transition_case`, `transition_trace`, `pipeline_case`, and `pipeline_trace`.

- IDs, addresses, ports, capacities, and instruction counts are unsigned 32-bit
  integers on the wire. Existing host `usize` fields use checked conversions.
- State words, playback positions, and lengths are unsigned 64-bit integers.
- Samples and gains use raw signed 32-bit Q8.24 integers. Ramp duration and elapsed
  counters use unsigned 32-bit integers. No floating-point conversion is involved.
- State snapshots are `{"memory": [...]}` in address order. A value is either
  `{"kind": "word", "value": 17}` or `{"kind": "ramp", "value": {...}}`.
- Instructions, jobs, and graph nodes have a `kind` plus named operand fields.
  Bindings and statuses use `kind` and, for variants with data, a nested `value`.
  `{"kind": "none"}` and `{"kind": "completed"}` have no `value` field.
- Required fields and numeric bounds are enforced. Unknown fields/variants,
  unsupported versions, booleans/floats in integer fields, duplicate fields, and
  non-finite numbers are rejected. Rust and Python retain integers above 2^53
  exactly; consumers must not convert them through floating point.

The schema checks representation. Graph validation and processor errors still
come from the reference implementations. A representable snapshot can deliberately
contain an invalid state address or value for a negative execution test. Interpreted
IDs and produced jobs must also fit the wire's 32-bit ID range.

`rt_fpga_reference` exposes optional `serde` and `schema` features; default native
renderer builds acquire neither dependency. FIFO storage, transition-processor
internals, and native app objects are outside this format. Explicit media snapshots
include the standalone arena and its allocation metadata.

## Commands and traces

The executable reads one document from stdin and writes one document to stdout:

| Command | Input | Output |
| --- | --- | --- |
| `schema` | None | Generated JSON Schema |
| `roundtrip` | Any document | Deserialized and reserialized document |
| `compile` | `graph` | `compiled` program, initial state, and bindings |
| `trace` | `case` | `trace` containing the case and reference results |
| `check` | `trace` | Same trace, after recomputing and comparing its results |
| `trace` | `audio_case` | `audio_trace` from actual state and job execution |
| `check` | `audio_trace` | Same trace, after recomputing both processors |
| `trace` | `media_case` | `media_trace` from actual storage commands |
| `check` | `media_trace` | Same trace, after recomputing the media store |
| `compile` | `transition_plan` | Canonical `transition_plan_image` |
| `trace` | `transition_case` | `transition_trace` from the logical hardware model |
| `check` | `transition_trace` | Same trace, checked against both model and native resolver |

Malformed input, a mismatched command/payload, or a failed trace comparison exits
nonzero. A processor execution error is instead recorded in the trace's status.

```sh
target/debug/fpga-reference trace \
  < scripts/fpga_testbench/examples/record_play.json > work/record-play.trace.json
target/debug/fpga-reference check < work/record-play.trace.json > /dev/null
```

A case loads its program and initial state once. Before each quantum it applies
the step's ordered state writes, representing settled loop decisions and host
controls. It runs `StateProcessor::run`, records the ordered jobs, full resulting
state, and status, then drains the queue. The quantum size stays in the program.
No transition clock or sample processing runs: a playback position stays fixed
unless a step writes it. The example explicitly advances its playback position.

The first failed step ends the trace. Any earlier writes, jobs, and state changes
from that step are retained, matching the reference's partial-failure semantics.
FIFO capacity refusal happens before instruction execution; it does not roll back
the step's preceding control writes. Queue capacity is reserved for a whole
quantum, as in the standalone model.

## Python and cocotb use

Put `scripts` on `PYTHONPATH`, then use dictionaries validated by the generated
schema. `load`/`dump` handle files; `loads`/`dumps` handle JSON strings.

```python
from fpga_testbench import load, run_reference, assert_result_matches

case = load("scripts/fpga_testbench/examples/record_play.json")
trace = run_reference("trace", case, executable="target/debug/fpga-reference")
reference = trace["payload"]["value"]

# The RTL-specific driver is to be supplied with the hardware implementation.
async def compare(driver):
    await driver.load(reference["case"])
    for step, expected in zip(reference["case"]["steps"], reference["results"]):
        # Apply writes, collect accepted FIFO jobs, and read state at completion.
        observed = await driver.run_quantum(step["writes"])
        assert_result_matches(expected, observed)
```

`observed` has `jobs`, `state`, and `status` with the same shapes as the reference
result. The comparator checks schema validity and exact values, including job
order. Internal registers and cycle counts are free to differ. Drivers must also
check completion, extra output, FIFO handshakes, and bounded stalls independently.
The RTL-specific cocotb/Verilator driver lives in `fpga/state_processor`; see its
README for the packed ABI, supported bounds, simulation commands, and regression
coverage. Its cocotb dependencies are separate from these serialization helpers.

`fpga/job_processor` adds a BRAM sample-processing pipeline and comparison bench.
An `audio_case` supplies program/initial state, queue capacity, logical `buffers`
and `output_ports`, and steps containing ordered state writes and raw sample
arrays per input port. `audio_trace` results include the emitted jobs, unconsumed
jobs, state, output arrays, and nullable sample arrays for each scratch buffer
(null means unready), plus `stage` (`media`, `state`, or `jobs`) and status. The
actual Rust state and job processors execute; no transition clock runs. Omitted
media starts an empty store and keeps media fields absent from results. Output
and scratch state persist between steps. Shape and execution errors preserve
partial effects and stop the trace.

Use `assert_audio_result_matches` for complete bit-exact comparison. The new
payload variants are additive in format version 1; existing state traces are
unchanged. See `fpga/job_processor/README.md` for the supported job subset,
hardware interfaces, simulation commands, and measured results.

## Elastic media traces

An optional `media` field on `audio_case` initializes the slot arena and view
tables. A standalone `media` payload can also round-trip that snapshot. Its fields
are `chunk_frames`, `samples` (flat raw i32 values), `free_chunks` (in stack order),
and `views` (an array of `{chunks, frames}` or null for removed view IDs). A
read-only view additionally has `"range":{"offset":3}`, where the unsigned
32-bit offset is within its first chunk. Its `frames` is the range's logical
length; reads wrap within that length. Aligned and empty read-only ranges use
offset zero. Empty ranges own no chunks. Writable views omit `range` entirely;
explicit null is rejected. This optional field is additive in format version 1,
preserving existing writable-view documents.

Optional `chunk_limit` bounds the number of slots for FPGA comparisons; omit it
for normal elastic growth. `samples.len()` must be a multiple of `chunk_frames`.
Each slot must have at least one view reference or appear exactly once in the
free stack. At most one writable view may reference a slot. Duplicate IDs within
a view, free/live overlap, orphaned slots, invalid range geometry and read-only
ranges exposing a writable chunk's unpublished tail are rejected on restore.
Reference counts are derived from the tables rather than duplicated on the wire.

An optional per-step `media_actions` array accepts `{"kind":"clear","view":0}`,
`{"kind":"remove","view":0}`, or
`{"kind":"slice","source":0,"start":3,"frames":5}`. Slice coordinates are
unsigned 64-bit frame counts relative to the source view. Creation returns the
next monotonically assigned view ID, visible as the new entry in the result's
view table. Slices can reference slices and remain valid after their source is
cleared or removed. Slice creation copies no sample payloads.

Actions run before state writes at the drained quantum boundary, outside the
signal program. Clear retains the view ID and its writable/read-only kind; remove
invalidates it. Both release references in table order. Only a slot's last release
adds its ID to the free stack; subsequent captures pop free IDs before extending
the arena. Capturing into a read-only view reports `invalid_state`. BRAM-cache
behavior is not modeled.

Media-enabled results include the full resulting `media` snapshot, so Python's
audio comparator checks slot contents, ownership, and reuse order as well as
samples and processor state. Host `Vec` capacity is excluded. Storage exhaustion
reports `memory_full` in the jobs stage, with the refused capture unchanged and
later jobs left in the FIFO. A media-control error reports the media stage; bad
initial snapshot geometry/ownership instead rejects initialization with a nonzero
tool exit. All failures retain previously completed effects. Old audio cases and
BRAM-only RTL comparisons continue to omit media fields.

```sh
target/debug/fpga-reference trace \
  < scripts/fpga_testbench/examples/elastic_record_play.json > work/elastic.trace.json
target/debug/fpga-reference check < work/elastic.trace.json > /dev/null
```

This six-quantum example grows a recording, plays a wrapped range, clears and
reuses slots, plays new content, and removes the view. Supplying a resulting state
and media snapshot as another case's initial state/media resumes the same logical
memory state. The ordinary BRAM top keeps media disabled. The media-enabled
`fpga/media_store` top executes ReadView/Capture through an external sample-memory
port and compares these traces against Rust.

`examples/shared_ranges.json` creates an unaligned slice, clears the source and
captures into its reusable slot while playing the slice bit-exactly. It then
clears the slice and captures into the remaining released slots without growing
the arena. The trace can be replayed from intermediate state/media snapshots;
the Python audio scoreboard compares range metadata, ownership and reuse order
as well as samples.

## Connected shared-loop traces

`pipeline_case` / `pipeline_trace` connect the transition, state and job phases.
A case contains a transition plan, signal program, node state, queue/buffer/I/O
capacities, optional initial media (explicit null when absent), and steps. Each
step supplies the existing `transition` step, node-state `writes` and `work`:
`external` carries a completion status, while `audio` carries per-port input
samples. The result includes the full transition result, ordered jobs, node
state, an optional staged pipeline error and optional audio results.

The runner invokes `pipeline::Pipeline`. Logical playback/capture instructions
read its committed transition records by ID, with no copied control operands.
External steps drain descriptors and report a modeled completion. Audio steps
run the actual Rust sample processor and media store. State/job failures stop
the trace with partial effects retained and logical time unadvanced. A failed
preview remains replaceable at the same frame. The additive variants retain
format version 1 and the existing subsystem document shapes.

```sh
target/debug/fpga-reference trace \
  < fpga/loop_pipeline/examples/record_play.json > work/pipeline.trace.json
target/debug/fpga-reference check < work/pipeline.trace.json > /dev/null
target/debug/fpga-reference trace \
  < fpga/loop_pipeline/examples/record_play_samples.json > work/pipeline-samples.trace.json
target/debug/fpga-reference check < work/pipeline-samples.trace.json > /dev/null
```

`fpga/loop_pipeline/README.md` describes the shared RAM interface, coordinator,
external job acknowledgement, combined Rust/RTL comparison and limitations. Its
internal RTL executor now includes the bounded media store: `simulate.py --media`
compares complete transitions, jobs, recorded/replayed samples and media snapshots.

## Serial media commands

`media_case` and `media_trace` are additional format-version-1 payloads for testing
the store independently of signal execution. A case contains `name`, initial
`media`, and `steps`. Command kinds are `create`, `slice`, `clear`, `remove`,
`reclaim`, `append`, and `read`. Append supplies `view` and raw `samples`; read
supplies `view`, u64 `position`/`length`, and a u32 output `frames` count.
Other operands follow the media actions above; reclaim takes `view`.

Every command result contains `status`, a tagged `value` (`none`, `view`, `empty`
or `samples`), and a complete media snapshot. Failed commands return `none` and
the unchanged media; subsequent commands still run. Invalid initial snapshots
instead reject initialization. Rust executes actual store operations. Python's
`assert_media_result_matches` validates and compares every field.

```sh
target/debug/fpga-reference trace \
  < fpga/media_store/examples/shared_ranges.json > work/shared-media.trace.json
target/debug/fpga-reference check < work/shared-media.trace.json > /dev/null
```

The RTL driver also compares reference counters with counts derived from view
tables. See `fpga/media_store/README.md` for ports, physical-capacity differences,
simulation, backpressure/reset tests, cycle counts and synthesis results.

## Logical transition boundary traces

`transition_case` supplies a name, a plan (`description` and `limits`) and steps.
The description contains a quantum and logical loop specifications: stable
slot/generation IDs, initial content length, sync source and repeat-sync flag.
The compiler emits canonical dependency components independently of declaration
or signal order. Composite kind/timeline fields are reserved and rejected by the
current compiler. See `src/rust/rt_fpga_reference/README.md` for the supported
musical subset and runtime protocol. No sample jobs execute in these traces.

Each step executes these actions in order:

1. If `collect_completed` is true, collect all previously terminal requests in
   slot order, followed by the explicit `collect` request IDs.
2. Process explicit `cancel` IDs, then submit `commands` in list order.
3. Run each observation list in `previews` against the same committed frame.
   This list must contain at least one preview. A retry replaces the candidate
   and recomputes source delays from the retained inputs.
4. If the final preview succeeds, commit it and finish one quantum. Earlier
   previews are never published. After a failed final preview, retry at the same
   frame with corrected observations; new control mutations remain backpressured
   in the hardware model.

Commands contain `target`, `sequence`, `at`, `timing` and `operation`. Timing is
`at` or `after_ticks` with `source` and `skip`. Operations are `stop`, `record`,
`play` or `seek`; the latter two include `position` and `retrigger`. Observations
are `capture_unavailable` for a loop or `refused` for a request, with a reason.
Control/preview replies use `accepted` or `rejected` with a `value`.

Each result records the starting frame, collection/cancellation/submission
replies, decisions for every preview, whether commit occurred, committed loop
states, retained request slots, ordered events, states after finish, and next
frame. Terminal requests keep their slots until collected. `remaining_ticks` is
null for terminal or absolute-time requests. Decisions include playback
discontinuity, request origin, and `waiting_repeat`; a waiting primitive remains
at its content endpoint without advancing until a source pulse releases it.

```sh
target/debug/fpga-reference trace \
  < scripts/fpga_testbench/examples/transition_record_play_sync.json \
  > work/transition.trace.json
target/debug/fpga-reference check < work/transition.trace.json > /dev/null
```

The example uses three logical loops: a 64-frame master, a take initially being
recorded, and a 16-frame follower. At frame 64 the master's second pulse changes
the take from recording to playback and starts the follower at that same boundary.
It also exercises discarded preflight refusals, seek and stop. The `check` command
independently executes `shoop_loops::Resolver` for the supported primitive subset
and compares every architectural result. Native-only fields and media operations
are outside this profile; the standalone processor has no native resolver
dependency. Rust regression tests add 100 seeded graphs over 8,000 quanta.

Python/cocotb drivers can use `assert_transition_result_matches(expected,
observed)` to compare complete boundary results. The RTL and packed command/table ABI
are described in `fpga/transition_processor/README.md`. Boundary tokens are local
runtime guards and are not serialized in JSON; internal settlement cycles remain
implementation-dependent. The native oracle adapter applies the hardware
profile's input-freeze and observation-capacity rules before calling the resolver.
