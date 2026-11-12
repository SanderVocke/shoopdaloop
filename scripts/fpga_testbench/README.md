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

Every document has `format_version: 3` and a `payload` with `kind` and `value`.
Payload kinds are `graph`, `compiled`, `program`, `instructions`, `state`, `jobs`,
`read_intents`, `resolver_case`, `resolver_trace`, `bindings`, `state_writes`, `on_wrap`, `status`, `case`, `trace`, `audio_case`,
`audio_trace`, `media`, `media_case`, `media_trace`, `chunk_case`, `chunk_trace`, `transition_plan`, `transition_plan_image`,
`transition_case`, `transition_trace`, `pipeline_case`, and `pipeline_trace`.

- IDs, addresses, ports, capacities, and instruction counts are unsigned 32-bit
  integers on the wire. Existing host `usize` fields use checked conversions.
- State words, playback chunk indices and chunk counts are unsigned 64-bit integers.
  Graphs/programs require `chunk_frames`. Playback nodes/jobs use explicit
  `position_chunks` and `length_chunks`; read intents add the actual window's
  `chunks` count. The quantum and arithmetic ramps retain sample units.
  Version 1 used frame positions and is rejected; existing examples now state
  their chunk size explicitly, including one-sample chunks for older sample grids.
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
| `trace` | `resolver_case` | `resolver_trace` with resolved IDs for each advisory read window |
| `check` | `resolver_trace` | Same trace, after recomputing the resolver |
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

Use `assert_audio_result_matches` for complete bit-exact comparison. These payload
variants are included in format version 3. See `fpga/job_processor/README.md` for
the supported job subset,
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
explicit null is rejected. This optional field is included in format version 3,
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

`pipeline_case` / `pipeline_trace` connect transition settlement, state, jobs and
media through `pipeline::Pipeline::process`. A case supplies a plan, program,
node state, queue/buffer/I/O capacities, initial media and steps. Each step has
`transition` commands, node-state `writes`, and `work` of kind `audio` carrying
per-port input samples. The old external completion mode is removed.

Results contain post-process transition state, ordered jobs, node state,
optional staged error and audio/media results. The overall pipeline error decides
success; a transition-stage success alone does not imply completed sample work.
Capture-start jobs create/reuse recording views internally. Failures preserve
partial diagnostic state and silence output; subsequent steps verify sticky
fault behavior rather than retrying. Fixture control writes are skipped once
faulted. Recreate/reload the engine to recover. Multi-quantum host-block silencing
is additionally covered directly by Rust tests.

```sh
target/debug/fpga-reference trace \
  < fpga/loop_pipeline/examples/record_play_samples.json > work/pipeline.trace.json
target/debug/fpga-reference check < work/pipeline.trace.json > /dev/null
```

See `fpga/loop_pipeline/README.md` for the single-run RTL contract and comparison
bench. Format version 3 rejects old preview/observation/external-step fields;
versions 1 and 2 are intentionally incompatible.

## Serial media commands

For lower-level storage tests, `chunk_case` / `chunk_trace` expose read/write spans,
prefetch, fence and retirement with u32 request IDs. Each result includes status,
read samples and logical arena contents. Prefetch is advisory; the Rust reference
has immediate writes/fences, while RTL validates buffering and delayed writeback
separately. See [chunk-memory protocol and replay](../../fpga/chunk_memory/README.md).

`media_case` and `media_trace` are additional format-version-2 payloads for testing
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

1. If `collect_completed` is true, collect terminal requests in slot order, then
   the explicit `collect` IDs.
2. Process `cancel` IDs and submit `commands` in order.
3. Call `process()` exactly once, then report post-process state and next frame.

Commands contain `target`, `sequence`, `at`, `timing` and `operation`. Timing is
`at` or `after_ticks` with `source` and `skip`; operations are `stop`, `record`,
`play` or `seek` (the latter two carry position/retrigger). Control and `processed`
responses use `accepted` / `rejected` with a `value`. There is no observation list,
preview result, committed flag, separate after-finish snapshot or boundary token.

Results report starting frame, admission replies, `processed`, loop states,
retained requests, ordered events and `next_frame`. Terminal requests retain
slots until collected; remaining ticks are null for terminal/absolute requests.
Processing failures latch and may expose partial diagnostic state. Successful
cases also compare with the independent native resolver; its internal native
transactions are outside the hardware contract.

```sh
target/debug/fpga-reference trace \
  < scripts/fpga_testbench/examples/transition_record_play_sync.json \
  > work/transition.trace.json
target/debug/fpga-reference check < work/transition.trace.json > /dev/null
```

Python/cocotb uses `assert_transition_result_matches` for exact architectural
comparison. RTL cycles remain implementation-dependent. See the transition
processor README for packed fields, fault/reset semantics and workload checks.
