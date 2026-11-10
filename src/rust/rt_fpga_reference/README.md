# Rust FPGA reference

An audio-only functional reference for the decomposition in `FPGA_ARCHITECTURE.md`.
Readability and explicit subsystem boundaries take precedence over performance.
This is not a cycle simulator, RTL generator, or hard real-time guarantee.

## Persistent node programs

Compile a program when the graph changes. Execute the same instructions every
quantum, including when loops change between recording, playback and stopped.
Controls change live state, not code. The current abstraction is deliberately
coarse: an instruction processes a whole node using its current state and emits
sample jobs. A hardware-like instruction set and VLIW lowering are deferred.

The standalone compiler emits `ProcessLoopPlayback`, `ProcessLoopCapture`,
`ProcessGain`, `ProcessMix`, input and output instructions. Operands identify
persistent state addresses and temporary buffers. The state processor owns
mode-dependent job selection and ramp progression. It reads
settled loop modes and positions without resolving transitions or advancing loop
clocks. It has no graph or sample-store access and executes the instruction list
once per quantum.

The separate `TransitionProcessor` owns immediate loop controls, logical clocks,
and one-shot enable/disable actions at a source loop's next wrap. It applies
actions before state execution and advances clocks after successful job execution.
Wrap eligibility is computed for all loops before any target changes, independently
of their signal order. This standalone model only observes quantum boundaries;
it does not yet split a quantum at an internal wrap or implement the app's full
scheduling rules. Future composite loop handling belongs in this subsystem.

The job processor drains the ordered FIFO, resolves views/ranges into chunks,
and performs sample arithmetic and storage operations. Changing the job stream
from quantum to quantum is expected; changing the state program is not.

```sh
cargo run -p rt_fpga_reference --example record_play
cargo test -p rt_fpga_reference
```

`record_play` prints one compiled node program and two different job streams,
then verifies exact recorded playback without changing the program.

## Serialization and RTL comparison

Optional `serde` and `schema` features describe the standalone subsystem boundary
values without adding serialization dependencies to default renderer builds.
The `rt_fpga_testbench` tool publishes a versioned JSON Schema and can compile
graphs, generate reference quantum traces, round-trip values, and verify saved
traces. A Python module validates the same format and compares job/state results
for a future cocotb driver. See `scripts/fpga_testbench/README.md` for commands,
the example case, exact integer representations, and the comparison boundary.

## Normal app / host selection

```sh
SHOOP_AUDIO_RENDER=fpga-reference SHOOP_AUDIO_BENCH_AUDIO_ONLY=1 \
  target/debug/shoop_host --config host.json --export-profile profile.json
# Attach the normal app to the exported profile using --ui-only.
```

The native adapter disables device MIDI, input history, restart smoothing, and
hosted processors for this mode. It supports basic audio strips, input monitoring,
recording, playback, gains/balance/mute, routing, and buses. Unsupported processing
nodes are refused rather than run through an existing audio renderer. MIDI tracks,
processed tracks/buses, preview, nonzero smoothing, special playback windows, and
advanced recording modes are outside the subset.

The app compiler attaches an immutable `NodeProgram` to a prepared graph on the
control owner. Its `ProcessNode` instructions reference persistent native node
instances; explicit routing instructions use compiled port addresses. Neither
mode nor parameter values are specialized into the program. Prepared graph slots
reserve instruction storage, and program/graph activation transfers one owner.
Signal execution follows the compiled instructions; it does not traverse the
signal-graph descriptor, compile a program, or reinitialize node state each
quantum. Structural graph changes compile another program.

The app executes explicit transition, state, and job phases:

1. The transition processor delivers scheduled actions and previews logical loop
   decisions using the existing resolver. Timing dependencies have their own order.
2. State preparation reserves captures and selects media from those decisions.
   A refused preparation feeds back into transition resolution before commit.
3. The state processor interprets the compiled signal program and emits jobs.
   `process_fpga_state` implementations handle live node behaviour, including
   monitoring and ramps, through a read-only view of settled loop decisions.
4. The job processor executes sample work. State feedback publishes captures.
5. The transition processor advances logical time and collects completions.

`rt_runtime` has separate FPGA transition and state executors. The native loop
adapter separates resolver/timing logic from physical capture state preparation
and publication. It retains shared native storage and transaction semantics;
preparation and transition settlement rendezvous before the transition commits. This is
a functional boundary, not a hardware implementation or a hardware protocol.
The state program never contains transition propagation instructions. Its job
writer only constructs sample jobs and transient operands.

Node feedback, meters and bulk readback also execute under the app state
processor's feedback phase. Sample jobs still execute separately in the job
processor. Native I/O and retained media use `f32`; `ExternalMemory` converts at
that boundary, while the standalone media store uses fixed-point chunks.
Native control/ramp metadata retains its existing representation and endpoint
semantics. Native and standalone ramp conventions can differ.

## Deliberate first-model choices

- Node-level single issue; no register ISA, VLIW or cycle-level timing model.
- Finite FIFO, filled for a whole quantum before job execution. The standalone
  model refuses insufficient queue capacity before changing state.
- Fresh temporary sample buffers per producer per quantum; fan-out shares a buffer.
- `Memory` separates jobs from storage. `MediaStore` uses mono chunks and external
  chunk tables; extra channels use separate views. Chunk size and quantum are independent.
- Views support append and reads within their recorded frame count. Capture and
  playback may run in the same quantum; FIFO order determines which samples have
  been written when a read executes. IDs never recycle; editing, reclamation and
  finite external-memory admission are deferred.
- Provisional signed Q8.24 samples and gains. Multiplication uses `i64`, truncates
  toward zero and saturates; binary mixes saturate per job. Ramp interpolation
  preserves its time domain. This is a functional model, not a hardware layout.
- A standalone ramp starts at its current value and reaches its target at offset
  `duration`; zero duration changes immediately.
- Standalone compilation/capture may allocate. The native adapter retains the
  existing callback/control heap guards using prepared storage.
- Execution errors can leave partial work: stop rather than retry that quantum.
- No MIDI, effects, smoothing, history/grab or advanced loop editing.

## Comparison

```sh
cargo test -p shoop_rt_engine --test audio_variants
cargo build -p shoop_host --no-default-features --features testing \
  --bin shoop_host --example chunk_benchmark
python3 scripts/benchmark_audio_chunks.py \
  --host target/debug/shoop_host \
  --launcher target/debug/examples/chunk_benchmark \
  --output work/fpga-reference-comparison \
  --modes current chunks chunks-descriptors fpga-reference \
  --scenarios idle record play record-play ramps churn
```

Tests compare output and captured audio at Q=1/32 and callback partitions 1/13/128,
and assert that program contents and allocation identity remain unchanged across
mode and parameter changes. Standalone coverage includes bounded view reads,
playback while appending to the same view in either signal order,
record/play/stop/seek/ramp changes through one program, FIFO refusal and fixed-point
saturation. Standalone and app timing tests also reverse signal order relative to
sync dependencies and check that sync edits and transitions preserve the signal
program. The historical three-renderer script default is unchanged.
Timing results describe software, not predicted FPGA timing.
