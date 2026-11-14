# Fixed-record MIDI runtime acceptance

Implementation branch: `feat/midi-rtl-runtime`.
Base: `feat/backend-replacement` at `bfe73c9a`.
Delivery: [PR #996](https://github.com/SanderVocke/shoopdaloop/pull/996).

Status: integrated implementation; browser/application and final CI acceptance
are still being completed. This record does not yet close P11.

## Scope and intentional differences

The user approved the [runtime design](../midi-runtime-rtl-design.md): four-byte
payload cap, complete basic channel messages and System Reset, rejection of SysEx
and parameter CC6/38/96–101, fixed events/pages, one engine clock, immutable cuts,
explicit preparation/publication and capacity recovery. Other system messages are
outside this baseline. The optional live-only parameter classifier is not exposed
as a host capability. These excluded message classes are not remaining parity work.

Supported state includes ordinary controls, notes and zero-velocity note-on,
sustain/sostenuto/Hold2, pending/loaded banks and programs, bend, pressure, modes
and defined reset behavior. Initial state excludes the target frame; its originals
play once after restoration. Synthesizer voices and device-specific state are not
reconstructed.

Public editable MIDI uses immutable 32-record pages and a sixteen-level radix
directory, advancing writer frontiers and explicitly granted position jobs. The
removed packet-slab/semantic-tree private implementations are superseded by the
new kernels and independent oracles. Existing file/upload byte encodings remain
adapter formats. The standalone legacy shared-MIDI utility remains tested; normal
application histories and continuous writers select the new block store.

New imports refuse unsupported documents before activation. Only prefix-version-0
null-selector artifacts may normalize, with a version/count report. Timeline
packets are never silently removed. This narrows compatibility deliberately;
SMF's separate representation/timing loss report is retained.

## Native and portable validation

On the provisioned Linux environment, Rust/Cargo 1.100 and nextest 0.9.116:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 CARGO_INCREMENTAL=0 cargo nextest run --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
cargo fmt --all -- --check
RUSTFLAGS='-D warnings' CARGO_INCREMENTAL=0 cargo build --workspace
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_dependency_boundaries.py
```

The integrated full native run passed **3,242 tests, 11 skipped**, in 495 seconds.
The pre-implementation baseline passed 3,241 tests, 11 skipped. Earlier policy
fixture failures were changed to supported ordinary MIDI where packet size or
queue pressure was the subject; explicit unsupported-message tests retain the
new refusal expectations. Earlier process/UI timeouts overlapped Wasm compilation;
all fifteen isolated reruns and the isolated complete suite passed.

The warning-denying workspace build, test-attribute policy and dependency-boundary
audit passed. The audit covers Linux, Windows, macOS and Wasm dependency graphs;
it is not a cross-platform execution claim. The subsequent focused runtime,
loop/engine, codec, native and worklet run passed **757 tests, one skipped**,
including final state-work accounting and the worker-envelope migration regression.
The final ARM USB-enabled check and both `zynq_probe` example tests passed.

Acceptance follow-up fixed four issues exposed by platform jobs: prepared state
cache metadata no longer copies four large state banks through the call stack;
JACK waits for acknowledged reset delivery and completed feedback cycles before
deactivation; standalone manifests include the common MIDI protocol dependency;
and the measurement harness accounts for fixed record pages and actual render
work. A one-MiB-stack socket workflow and all eleven strict real-JACK tests passed.
The release shared-media measurement passed all twenty quantum/partition cases.
Both socket/shared-memory input, persistence and composite canaries passed,
including exact save/reload, client absence and resumed editing. The standalone
loop fixture now checks mixed two-/three-byte basic messages; all sixty tests and
both independent native replay harnesses passed. Its fixed transport buffer uses
the declared upload prefix and returns the complete prepared buffer.

Key executable evidence:

- `rt_runtime/tests/midi_short.rs`: exhaustive status/length/controller policy,
  canonical sixteen-byte records, legacy-prefix normalization and no-heap guards.
- `midi_state_steps.rs`, `midi_state.rs`, `midi_restore_state.rs` and loop
  restoration tests: stepped versus sequential state, notes/pedals/banks/modes,
  known values, cleanup/restoration and output refusal.
- `midi_blocks.rs`, `midi_positions.rs`, `midi_writers.rs`: independent event-list
  oracles, retained versions, sparse extent, clipped normalization, exact-boundary
  replay grants, atomic wrap publication, failure freeze and complete retirement.
- Resource `indexed_midi` tests: pinned Grab across eviction/cancellation, private
  wrapped patches, old exports, shifted raw windows, density/byte/owner pressure,
  long suffixes, advancing frontiers and silent-history copy avoidance.
- Loop/engine input, timing and workflow tests: public scheduled/immediate flows,
  nonquantized raw windows, preplay, simultaneous writers, mixed complete cuts,
  lost-input fencing and continued unrelated audio. Typed and encoded production
  routes retain their existing application oracles.
- Native bridge/direct-host tests: whole-batch refusal, reset-head retries, stale
  producer epochs, accepted input loss and output density beyond 1,024 messages.
- Codec tests: atomic unsupported-content refusal, exact basic-MIDI state/order,
  reported null-selector migration and bounded worker envelope transfer.

## Browser and firmware boundaries

The frontend, worklet and codec worker build for Wasm. The actual module/processor
artifact contract passed, including split framing, timestamped MIDI, bounded
JavaScript ingress, explicit transport loss, profiling, shutdown and no module
memory growth after preparation.

The default unoptimized browser workflow reached its five-minute overall limit
while loading the last of three hosted processor families. It continued advancing;
no per-step failure, render-memory growth, command overflow or discontinuity was
reported. A local `WORKFLOW_TIMEOUT_MS` override leaves the core CI deadline and per-step checks unchanged. The combined
composite workflow has a separate seven-minute overall allowance: earlier release
CI exhausted five minutes during the final resave after the composite checks had
passed. Complete browser/Web MIDI acceptance remains pending.

The firmware USB adapter distinguishes recovery epochs from recorded messages,
establishes a new baseline on device-generation changes and preflights its fixed
256-message transport queue. Capacity refusal and device backpressure invoke its
existing acknowledged reset FSM. Generic C USB decoder/packetizer vectors retain
their independent transport scope; engine policy rejects unsupported messages.
The portable bridge passed address/undefined-behavior sanitizers, and the ARM
USB-enabled check passed. Actual guest/board CI and physical qualification remain
distinct evidence.

## Resource and hardware limits

Event density is retained; memory profiles now name record pages, directories,
leases, state banks and event ownership separately. Desktop sizing conservatively
reserves a page per event credit for sparse layouts. Dense pages use fewer pages.
Preparation, retained exports and pending retirement all count against finite
pools. Cold replay is linear in the prefix; immediate software operations obtain
the complete checked grant before musical mutation. This is not a constant-time
seek or a physical-device deadline certification.

Work counters include directory/record operations, state-cell updates/copies and
retirement, with explicit saturation reporting. The logical FSMs and record
encoding are the future RTL reference. No RTL implementation, RAM-port timing
model, FPGA clock/sample-rate guarantee or new physical-board qualification is
claimed by this Rust implementation.
