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

`SignalTransitions` supplies node controls and one-shot `OnWrap` actions for the
standalone `Backend` audio fixtures, including arbitrary non-grid loop lengths.
The separate logical `TransitionProcessor` described below is the hardware model.

The job processor drains the ordered FIFO, resolves views/ranges into chunks,
and performs sample arithmetic and storage operations. Changing the job stream
from quantum to quantum is expected; changing the state program is not.

```sh
cargo run -p rt_fpga_reference --example record_play
cargo test -p rt_fpga_reference
```

`record_play` prints one compiled node program and two different job streams,
then verifies exact recorded playback without changing the program.

## Logical transition processor

`transitions::TransitionProcessor` models the transaction boundary intended for
hardware. Logical `LoopId` values are independent of signal nodes, media views,
and instruction order. A loop can have separate playback and capture bindings,
or eventually be a timing-only composite. The native app still uses its existing
resolver; adopting this model in the app is a separate integration step.

Compile a `TransitionPlan` once from loop IDs, initial content lengths, sync
sources, repeat-sync flags, quantum, and storage limits. Compilation produces
canonical dependency components: acyclic components run in source-first order,
and legal sync cycles settle in stable ID order. Each loop can deliver at most
one pulse per boundary. `LoopKind::Composite` and a timeline reference reserve
the extension point; this version returns `Unsupported` for either.

The runtime protocol is:

1. `submit(Command)` retains a request in a fixed slot with a generation-checked
   ID. Stop, play, record, seek, retrigger, absolute frame scheduling, and delayed
   source pulses are supported. Positions and lengths use the quantum grid;
   scheduling frames round up. Supported quanta are 1, 16, 32, 64 and 128 frames.
2. `preview(observations)` settles into prepared scratch. Inspect
   `decisions(boundary)` to preflight media resources. `CaptureUnavailable` or
   request-specific `Refused` observations can replace the preview at the same
   frame. Every retry starts from committed state and retained inputs; it cannot
   consume a delayed pulse twice. Preview does not publish state or completions.
3. `commit(boundary)` publishes the candidate exactly once. Only the latest
   successful boundary token is valid. `StateBindings::export` maps committed
   decisions to validated playback enable/position/length and capture-enable
   words before running the existing state/job processors.
4. After successful quantum execution, `finish_quantum(boundary)` advances logical
   clocks exactly once. A playback endpoint remains `position == content_length`;
   the next preview resolves wrap/repeat waiting against the source's pulse.
   Signal-state position operands are exports, not a second clock.
5. Collect terminal requests with `take_completion`. Until collected, terminal
   requests retain their slot and sequence. Reuse increments the generation;
   exhausted generations retire the slot. A successful stop cancels older pending
   commands for that loop, and the greatest eligible sequence wins arbitration.

Submission, cancellation and collection are backpressured from the first preview
until finish, including after a failed preview. Retry with corrected observations
to obtain a valid candidate. Preview and commit can fail without publishing partial
logical state; counters use checked arithmetic. State/job execution failures after
commit require stopping the pipeline, rather than replaying that quantum.

Preparation may allocate. Submission, cancellation, preview/retry, commit, finish,
collection and binding export use prepared storage without heap operations.
Loop/request capacities are explicit; event storage reserves at least two events
per loop plus one per request, covering pulse, wrap and transition events without
silent truncation. Preparation currently bounds loop/request slots to 65,536 each
and events to 196,608. This is a transaction-level model, not a cycle count or a
promise that every configuration meets a hardware deadline.

Composite execution, trigger actions, request batches, playback monitoring during
recording, media ownership/admission and live topology replacement are deferred.
The model exports logical capture enable and published length/revision; a media
adapter remains responsible for choosing recording views and reserving storage.
`StateBindings` validates the entire export before writing any operand. It does
not perform media preparation or enforce playback/capture exclusion in sample jobs.

The testbench compares decisions, retained requests/completions, ordered events,
and post-quantum state against `shoop_loops::Resolver` for this primitive subset.
See `scripts/fpga_testbench/README.md` for replayable transition traces.

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
and publication. It retains native resource ownership and transaction semantics;
preparation and transition settlement rendezvous before the transition commits. This is
a functional boundary, not a hardware implementation or a hardware protocol.
The state program never contains transition propagation instructions. Its job
writer only constructs sample jobs and transient operands.

Node feedback, meters and bulk readback also execute under the app state
processor's feedback phase. Sample jobs still execute separately in the job
processor. Native device I/O uses `f32`; retained reference audio uses the elastic
fixed-point chunk store. Capture staging and playback pass raw fixed-point words.
Bulk import/export converts to/from interleaved `f32` at the persistence boundary.
Native control/ramp metadata retains its existing representation and endpoint
semantics. Native and standalone ramp conventions can differ.

## Deliberate first-model choices

- Node-level single issue; no register ISA, VLIW or cycle-level timing model.
- Finite FIFO, filled for a whole quantum before job execution. The standalone
  model refuses insufficient queue capacity before changing state.
- Fresh temporary sample buffers per producer per quantum; fan-out shares a buffer.
- `Memory` separates jobs from storage. `MediaStore` uses a growing contiguous
  mono sample-slot arena and growing per-view chunk tables; extra channels use
  separate views. Chunk size and quantum are independent.
- Views support append and reads within their recorded frame count. Capture and
  playback may run in the same quantum; FIFO order determines which samples have
  been written when a read executes. Clear/remove return chunk IDs to a LIFO free
  stack, and allocation reuses those slots before growing the arena. View IDs
  never recycle. Sharing and editing remain deferred.
- Provisional signed Q8.24 samples and gains. Multiplication uses `i64`, truncates
  toward zero and saturates; binary mixes saturate per job. Ramp interpolation
  preserves its time domain. This is a functional model, not a hardware layout.
- A standalone ramp starts at its current value and reaches its target at offset
  `duration`; zero duration changes immediately.
- Compilation/capture may allocate. Capacity hints can prepare storage and tables
  before execution. Elastic growth and table retirement can allocate/deallocate
  during reference callbacks. Ordinary renderers retain their heap guards;
  reference callbacks measure heap operations, and warmed playback is checked to
  perform none.
- Execution errors can leave partial work: stop rather than retry that quantum.
- No MIDI, effects, smoothing, history/grab or advanced loop editing.

## Elastic media storage

`MediaStore::new(chunk_frames)` grows as needed without a configured slot limit.
`with_chunk_limit(chunk_frames, slots)` enables bounded-memory comparison tests;
it does not allocate the pool up front. `reserve_chunk_capacity(slots)` and
`reserve_view_capacity(view, frames)` are optional preparation hints, not exclusive
allocation reservations. IDs remain stable when the arena or tables relocate.

`create_recording` is fallible. Creation and append use checked arithmetic and
fallible reservations, returning `MemoryFull` on capacity or allocation refusal.
An append reserves everything before changing samples, chunk IDs, or frame count.
Earlier jobs in a failed quantum still retain their existing partial effects.
Reads and appends copy spans bounded by chunk edges and logical loop wrap.

`clear(view)` retains its ID/table capacity for another recording; `remove(view)`
invalidates its ID permanently. Both release chunk IDs in table order, so the last
released ID is allocated first. Call them only after jobs using the view have
drained. They are linear control operations; no asynchronous reclamation, sharing,
or reference counting is modeled. Reused slots may retain samples outside the
new recording's published frame count, which reads cannot access. Arena capacity
is retained for reuse rather than returned to the OS on each clear.

`snapshot` and checked `from_snapshot` preserve samples, view tables, removed-view
holes, optional limits, and free-stack order. They omit host capacity hints and
cache/timing behavior. The audio trace tool accepts initial media, boundary
clear/remove actions, and resulting snapshots. See its README and the
`elastic_record_play.json` example. The standalone `Backend` uses this store
through the existing job memory interface.

The app-selected reference renderer uses the same store for recording, playback,
bulk readback, and import/export. Resource revisions and snapshot leases still
own each take. A snapshot bounds reads to its published prefix even while the
recording grows. Clear releases capture-bank owners; pinned snapshots keep their
audio until their last lease ends. Retired takes release at most one chunk per
resource reclamation step. This bounds reclamation work by the existing step
budget instead of walking an entire recording on clear. Reusable arena capacity
stays allocated; no BRAM cache is modeled.

Audio payloads can outgrow the configured native page pool. Resource/version/
snapshot counts, queues, and other engine metadata still have their prepared
limits. The native page pool is still prepared for other resource facilities but
does not hold a duplicate of reference audio.

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

For release callback timings and actual allocator counts through the offline
engine endpoint:

```sh
cargo build --release -p shoop_rt_engine --example elastic_cpu_benchmark
python3 scripts/benchmark_elastic_cpu.py --voices 1 4 8 --callbacks 1024 --repeats 3
```

This measures stereo playback, one recording alongside playback, all recording,
and clear/reclamation at 48 kHz, quantum 32 and callback 128. It alternates renderer
order and records per-callback CPU/heap counters, growth events and slot usage.
The host benchmark CSV also reports elastic growth, sample access and reclamation
counters. These shared-machine measurements do not establish a worst-case bound.
Measured results and reproduction commands are recorded in
`docs/design/chunks/ELASTIC_REFERENCE_RESULTS.md`.
