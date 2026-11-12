# Production / RTL verification

`rt_backend_testbench` runs the production Current `GraphRuntime`, loop planner,
playback/capture nodes and shared engine gain DSP. Float and Q8.24 are static
instantiations of that path. No old FPGA Rust executor is used by this runner.

The version-1 scenario describes logical nodes/connections, loops/sync sources,
media in raw Q8.24, per-quantum input, and named actions. `compile` lowers the
production graph's ordered descriptor and production timing components into an
RTL image. Instructions, state addresses and chunk tables are outputs of that
compilation. The program remains constant when actions or loop modes change.

```sh
cargo build -p rt_backend_testbench
target/debug/rt-backend-testbench run < fpga/verification/scenarios/mono-record.json
target/debug/rt-backend-testbench run --numeric float < fpga/verification/scenarios/mono-record.json
target/debug/rt-backend-testbench compile < fpga/verification/scenarios/mono-record.json
python fpga/verification/simulate.py \
  --case fpga/verification/scenarios/mono-record.json \
  --case fpga/verification/scenarios/silence.json \
  --case fpga/verification/scenarios/gain-ramps.json
```

The CLI also accepts multiple whitespace-separated JSON documents, including
JSONL. `schema` emits the checked-in Rust-derived schemas. Python rejects unknown
and duplicate fields, nonfinite values, fractional integers, and integer overflow
before starting a simulation. `roundtrip --kind scenario|image|result` checks the
shared documents through Rust. Run contract tests with:

```sh
cargo test -p rt_backend_testbench
python -m unittest discover -s scripts/tests -p test_rt_backend_testbench.py
```

The full pipeline consumes the generated image plus common input/actions. It
independently settles transitions and executes state/jobs, real media allocation,
cache and AXI. Compare all raw output and exported media samples, post-advance
loop state, boundary events, pending requests and collected outcomes. Pending and
outcome containers use name order; event order remains timing-component order.
Physical addresses/request-slot identities are not common semantic results.

`collect_completed` drains the production transport's FIFO at the beginning of a
quantum. A final drain observes terminal results after the last quantum without
processing another block. Outcome records retain their real requested, delivered
and terminal frames. An unread completion retains runtime credit. Selective
slot collection remains a standalone hardware transport test; it is not a common
FIFO operation. Media exports are read through production snapshot codecs after
processing; retained snapshots keep their ownership throughout the session.

Artifacts include the original scenario, generated image, fixed/float production
results and RTL results. Exact failures report a field path down to quantum,
channel and sample. Replay by passing the saved scenario to `--case` with the
same physical/memory options. `--memory-model fixed|dramsim3`, `--word-transfers`,
`--axi-data-width`, buffer counts and deterministic stalls use the existing AXI
services. Instant mode preserves the upstream native SystemVerilog memory.

The directed corpus covers ordering, clipping, stereo recording/retakes, retained
exports, sync chains/self-sync/cycles, repeat waits, scheduling/conflicts,
cancellation, stale identities, completion credits, shared ranges and reuse.
Seeded sessions include capture and playback. Long sessions check 1,024 playback
quanta and 256 consecutive ramp interruptions. Alternate Q/chunk geometries and
host-buffer partitions have separate gates. Run the common corpus with:

```sh
python fpga/verification/simulate.py --directed --random-cases 30 --faults
python fpga/verification/simulate.py --long-sessions
python fpga/verification/simulate.py --benchmark64 --config 8x8 \
  --read-buffers 128 --memory-model dramsim3
```

Every scenario also runs through production float. `float-accuracy.json` reports
maximum absolute/RMS error, signed bias, first-to-last quantum drift, and a checked
error budget propagated through the actual compiled arithmetic order. Conversion
error, each fixed truncation and float rounding contribute to the bound; repeated
read-only playback does not accumulate a fictitious error per iteration. Ramp
interruption carries its boundary error into the next ramp. Media bounds follow
captures and retained exports. The audited Q8.24 policy delegates to the same
arithmetic and records actual saturation counts and their first operation ordinal
per quantum. Undeclared saturation fails; explicitly intentional saturation is
reported separately. The audit must itself match ordinary production Q8.24 exactly.
The float budget never relaxes the raw Q8.24/RTL comparison. Contract tests inject
both a one-bit fixed mismatch and an above-budget float mismatch to prove these
gates detect errors. `accuracy-gates.json` records campaign evidence.

TP can admit a future timestamp before the runtime schedule delivers it. Common
`delivered` means the first eligible audio boundary, and common pending requests
are observed after delivery. Raw TP acceptance/refusal frames remain in each
`.ingress.json` diagnostic. Both transports retain completion credits; TP's
`capacity` admission refusal maps to runtime `full` in this profile. Hardware
memory/descriptor exhaustion and bus faults are fatal, unlike graceful native
resource refusals, and have separate sticky-silence/reset checks.

`timing.json` records TP, SP, JP, job kinds and memory wait phases per quantum.
DRAMsim3 adds actual data/command bus utilization over the active processing
window, excluding host setup and observation. These are simulator measurements,
not board-level deadline guarantees.
