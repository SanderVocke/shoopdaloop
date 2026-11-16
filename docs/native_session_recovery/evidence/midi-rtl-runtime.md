# Fixed-record MIDI runtime acceptance

Implementation branch: `feat/midi-rtl-runtime`.
Base: `feat/backend-replacement` at `bfe73c9a`.
Delivery: [PR #996](https://github.com/SanderVocke/shoopdaloop/pull/996).
Verified implementation: [`24319e72`](https://github.com/SanderVocke/shoopdaloop/commit/24319e7273bce270fd302c1d6057d3aafe9d9c0c).

Status: accepted; P11 complete within the user-approved basic-MIDI scope.

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

Production editable MIDI uses immutable 32-record pages, a sixteen-level radix
directory, advancing writer frontiers and explicitly granted position jobs. The
packet-slab/semantic-tree private implementations have been removed. All normal
application histories and continuous writers use the new store. Existing
file/upload byte encodings and the independently tested standalone shared-MIDI
utility retain their separate adapter/utility roles.

Imports refuse unsupported documents before activation. Only prefix-version-0
null-selector artifacts may normalize, with a version/count report surfaced in
the completed native/browser import result. Timeline packets are never silently
removed, including unsupported packets followed by a reset. Exact files preserve
supported bytes/order and canonical state; SMF's separate timing/representation
loss report remains applicable.

## Native and portable validation

The provisioned Linux environment uses Rust/Cargo 1.100 and nextest 0.9.116:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 CARGO_INCREMENTAL=0 cargo nextest run --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
cargo fmt --all -- --check
RUSTFLAGS='-D warnings' CARGO_INCREMENTAL=0 cargo build --workspace
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_dependency_boundaries.py
```

The baseline passed **3,241 tests, 11 skipped**. The isolated refreshed complete
native suite passed **3,244 tests, 11 skipped**, in 344 seconds. Final native
scripting/adapter follow-up passed **91 tests**. Existing policy fixtures use
ordinary MIDI when their subject is queue density or packet size; dedicated tests
assert the intentional unsupported-message refusals. Obsolete private semantic
index tests were replaced with independent event-list/frontier oracles.

The warning-denying workspace build, formatting, test-attribute policy and
dependency audit passed. The latter covers Linux, Windows, macOS and Wasm graphs;
it is distinct from executing on those platforms. The exact native feature matrix
passes: JACK, CPAL, both, neither and system-preview builds, plus the corresponding
native-host/host library tests. The direct adapter's recovery storage shares its
feature gate.

Additional local acceptance passed:

- All eleven strict real-JACK tests, fifty repeated shutdown cases and twenty
  routing/retirement/shutdown repetitions. Reset delivery is retried in a silent
  callback after cleanup and after shutdown xruns, with completed graph cycles
  required before deactivation. Ordinary xruns do not starve live MIDI.
- Both socket/shared-memory input, persistence and composite application canaries,
  exact save/reload, resumed editing and client absence. The socket workflow also
  passes with a one-MiB process stack.
- All twenty release shared-media quantum/partition measurement cases, including
  fixed-record memory accounting and finite render work.
- Sixty standalone loop-host tests and both independent native replay harnesses.
- All seventeen runtime examples, the buffered MIDI example and the draft MIDI
  loop example, with documentation links and source excerpts checked.
- ARM USB-enabled build, both firmware probe example tests and the portable USB
  bridge under address/undefined-behavior sanitizers.

Platform acceptance moved initial/frontier/cache values into prepared state banks.
Media descriptors have a compile-time 128-byte ceiling; nested writer/Grab
ownership no longer copies full state banks through the stack. Firmware control
and render tasks each reserve 512 KiB inside the existing RTOS heap. Prepared
zeroed media arenas touch backing pages without a second full byte-by-byte fill.

Key executable evidence:

- `rt_runtime/tests/midi_short.rs`: exhaustive status/length/controller policy,
  canonical sixteen-byte records, legacy-prefix migration and no-heap guards.
- `midi_state_steps.rs`, `midi_state.rs`, `midi_restore_state.rs` and loop
  restoration tests: stepped versus sequential state, notes/pedals/banks/modes,
  known values, cleanup/restoration and output refusal.
- `midi_blocks.rs`, `midi_positions.rs`, `midi_writers.rs`: independent event-list
  oracles, retained versions, sparse extent, clipped normalization, exact-boundary
  replay grants, atomic wrap publication, failure freeze and complete retirement.
- Resource `indexed_midi` tests: pinned Grab across eviction/cancellation, private
  wrapped patches, old exports, shifted raw windows, density/byte/owner pressure,
  long suffixes, advancing frontiers and silent-history copy avoidance.
- Loop/engine workflows: scheduled/immediate actions, raw windows, preplay,
  simultaneous writers, complete paired cuts, input-loss fencing and unrelated
  audio progress. Typed/encoded routes retain their application oracles.
- Native bridge/direct-host tests: whole-batch refusal, reset-head retries, stale
  producer epochs, input loss, recovery after acknowledged reset, and output
  density beyond 1,024 messages.
- Codec tests: atomic unsupported-content refusal, exact state/order, reported
  prefix migration and bounded worker-envelope transfer.

## Actual browser and firmware boundaries

The rebuilt worklet/codec artifact contract and hosted Chromium/Web MIDI workflow
pass with zero xruns. Checks include actual device ingress, exact note
recording/playback, output-send failure/reset acknowledgement, unsupported-message
refusal, bounded JavaScript ingress loss, endpoint reconnect, files, input edits
and all three hosted processor families. The active browser CI workflow enables
these MIDI checks. Worklet framing, profiling, shutdown and no module-memory
growth after preparation are covered by its separate artifact contract.

The initial unoptimized local browser attempt exhausted its overall five-minute
deadline while loading the last processor; the completed local run used the
existing `WORKFLOW_TIMEOUT_MS` override. Per-step checks and the core CI deadline
were unchanged. The combined composite workflow has a seven-minute overall
allowance. The previously agreed web-debug workflow qualification remains in
place; release CI runs the actual browser workflows.

Firmware USB distinguishes loss epochs from recorded messages, establishes a new
baseline on device-generation changes and preflights its fixed 256-message output
queue. Backpressure uses an acknowledged reset FSM. The generic C USB
decoder/packetizer keeps its independent transport vectors; engine ingress applies
the common basic-message classifier. Firmware CI exercises the actual FreeRTOS
guest, including USB, network deployment and signed SD/A-B recovery under the
QEMU and Zedboard memory profiles. This is emulation evidence, not new physical
board qualification.

## Platform CI and review

All implementation-commit workflows passed:

- [Build and Test ShoopDaLoop](https://github.com/SanderVocke/shoopdaloop/actions/runs/43561242658):
  Linux debug/release, Windows debug/release, macOS ARM debug/release, WebAssembly
  debug/release, native JACK acceptance and feature matrix, full native coverage,
  fixed shared media, bare-metal runtime and all desktop process checks.
- [Firmware](https://github.com/SanderVocke/shoopdaloop/actions/runs/43561242640):
  firmware, TinyUSB MIDI, QEMU/Zedboard profiles, network deployment and SD recovery.
- [CodeQL](https://github.com/SanderVocke/shoopdaloop/actions/runs/43561242627):
  Rust, C/C++, JavaScript/TypeScript and Python.
- [Hosted processors](https://github.com/SanderVocke/shoopdaloop/actions/runs/43561242676):
  all four feature combinations, actual worklet DSP and Miri ownership checks.
- [Documentation](https://github.com/SanderVocke/shoopdaloop/actions/runs/43561242680).

The implementation-commit Linux debug job passed the complete native workspace:
3,244 tests passed, 11 skipped, in 317 seconds.

The scheduled/manual nightly-only matrix is skipped for pull requests by its
existing condition. The automated coverage report records 91.58% patch coverage,
above the configured 80% floor, and 89.40% project coverage within the existing
one-percentage-point tolerance. There are no unresolved review requests. The
configured coverage gates remain unchanged.

These runs test the verified implementation above. The subsequent P11 closure
commit changes only Markdown records. No merge is authorized.

## Resource and hardware limits

Event density is retained; profiles name record pages, directories, leases, state
banks and event ownership separately. Desktop sizing conservatively reserves a
page per event credit for sparse layouts; dense pages use fewer pages. Retained
exports, edit preparation and pending retirement all count against finite pools.

Cold replay is linear in prefix events. Immediate operations obtain a complete
checked finite grant before musical mutation; background preparation advances by
explicit grants. This is not a constant-time seek or a physical-device deadline
certification. Work counters expose directory/record operations, state-cell
updates/copies and retirement, with explicit saturation reporting.

The logical FSMs and fixed-record encoding form the future RTL reference. The
native typed state bank is not the proposed packed hardware layout. No RTL, RAM
port arbiter, FPGA clock/sample-rate guarantee or physical-board qualification is
delivered here; the design's timing/port feasibility gate precedes those claims.
