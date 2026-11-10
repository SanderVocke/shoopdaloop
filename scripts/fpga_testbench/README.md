# FPGA state processor interchange

The Rust boundary types define a versioned JSON format for programs, instruction
lists, state snapshots, jobs, compiler results/bindings, transition requests and
state writes, and execution results. `rt_fpga_testbench` generates the schema and
runs the actual standalone Rust state processor. Python validates and transports
these values without implementing the processor again.

This is the data layer for a future cocotb driver. It defines architectural
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
`bindings`, `state_writes`, `on_wrap`, `status`, `case`, and `trace`.

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
renderer builds acquire neither dependency. Private sample storage, FIFO storage,
transition-processor internals, and native app objects are outside this format.

## Commands and traces

The executable reads one document from stdin and writes one document to stdout:

| Command | Input | Output |
| --- | --- | --- |
| `schema` | None | Generated JSON Schema |
| `roundtrip` | Any document | Deserialized and reserialized document |
| `compile` | `graph` | `compiled` program, initial state, and bindings |
| `trace` | `case` | `trace` containing the case and reference results |
| `check` | `trace` | Same trace, after recomputing and comparing its results |

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

## Python and future cocotb use

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
There is no RTL adapter or cocotb dependency yet.
