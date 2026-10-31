# Stage 7: resource uploads and prepared nested-program staging

Implementation milestone: `test(loop-host): exercise resource and program staging across actual hosts`.
Base during verification: `b1e43cb7db7924b976639b1a9cb987568043e416`, dirty implementation
subsequently included in the containing milestone commit. This is not final-head
CI, a performance measurement, Miri, production migration or Stage7 completion.
The prepared ingress/payload/program/cancellation checklist row is verified;
Stage7 is now **5/8**, with Stages7–10 still incomplete.

## Implementation and owning protocol

`Controller::resource(lane, Request)` forwards prepared owning generic resource
commands through existing control capacity. The runtime and loop library retain
their dependency boundaries; no production browser/device dependency or unsafe
code is added. No manifests, lockfiles or dependencies change in this milestone.
The fixture prepares optional topology/source banks before the first callback.

`LPS1` requests are128 bytes and `LPR1` results256 bytes: canonical little-endian
fields, full-width frame/sequence arguments, checked generations, two local
resource/two program banks, one independent command credit and one reusable64-byte
owning chunk buffer. No pointer, Rust layout, scoped handle or owner crosses
memories. Generic resource completion is distinct from scheduled admission/domain
execution. Actual returned Write and program payload bytes are checked before
client credit/mirror changes. Read cursor/EOF and next nonwrapping generations
must correlate. Invalid/foreign/duplicate responses preserve state and credit.

The shared native/direct/threaded/binding/actual script performs:

- Record0/Play4/Stop12, retaining the primitive independent sample/MIDI reference.
- Create/upload/finish/commit stereo audio10–17/right×10, revision1. Retain its
  snapshot across a second upload100–107/right×10, revision2; read every old byte
  again before release, then every new byte. Validate/release a real audio role source.
- Upload/read complete64-byte raw MIDI, including note-on, frame/length records
  and37-byte SysEx. Raw uploads do not have playback checkpoint indexes:
  RoleSnapshot must fail with domain522 and retained flags5, without a renderer
  fault or fabricated playable media. Explicitly release/remove both resources.
- Upload a precompiled64-byte LTP1 header plus15 records; validate with increasing
  progress, activate graph/topology epochs1/0→1/1 and recycle the displaced owner.
- Play explicit outer→regular inner→recorded primitive at6784, explicitly seek
  outer iteration1 at6788, and Stop at6792. All commands are admitted before their
  deadlines. These composites have no sync source and **no nominal fallback clock**;
  this is not autonomous composite-clock evidence.
- Require56 requests/82 staging results/six primitive results,137 mapped stereo
  samples,3990 MIDI bytes and8200 semantic bytes. A separate manual byte table
  checks all seven boundaries/21 decisions/23 events, including inherited seeks/
  stops, source pulses and capture revision1 already present at Record0. All
  resource/program banks reach generation2 and counts0; recovery is256/16/16/0.

The fixture README and WIRE guide document commands, every offset, metadata,
error/phase, cancellation/owner return, clocks and limitations. The loop runtime
guide documents the new resource forwarding API and generic versus playable media.

## Native and actual host evidence

Native direct and two capacity-one actor channels run the same script. Renderer
owners are constructed/retained locally. Only copied envelopes/fixed observations
cross threads; waits, joins and variable diagnostic copying remain outside guards.
Complete direct/threaded reports agree, without scope/time normalization.

Node and Chromium execute the same public-binding script and compare complete
reports, not only check names. Additional actual non-isolated offline/live worklets
instantiate the same Wasm module in independent ordinary memories, with main marker
0x51a17/worklet0 and no SAB/COOP/COEP. One128-byte request and two256-byte response
transfer buffers detach and return at their exact sizes, separately from command
credit. Malformed magic2 and foreign session9 retry the original request unchanged.
Every actual result also undergoes foreign9/valid0/duplicate8 credit checks.

Each request advances one acknowledged engine call:53 calls of128 frames, then
1/4/4 frames. Total6793 frames/56 callbacks, program start6784. The engine pauses
between steps; browser silence during a wait is not engine advancement. A separate
AudioWorklet probe checks all512 samples/channel of four audible quanta and maps
137 real engine samples. Short-call tails must be silent. Offline additionally
checks both complete14592-frame channels, including every query/startup/stopped
sample. Live muting is after the probe. Only actual session nonces normalize
against the fixed-partition native report; musical time, metadata, content,
MIDI, semantics and generations never normalize.

| Profile/context | Actual browser starts | Fixed worklet bytes | Heap alloc/realloc/dealloc |
| --- | --- | ---: | --- |
| debug offline |128,13696,13952,14208|2,031,616|0/0/0|
| debug live |1024,28160,28672,29184|2,031,616|0/0/0|
| release offline |128,13696,13952,14208|1,966,080|0/0/0|
| release live |1536,28672,29184,29696|1,966,080|0/0/0|

Main memory is also fixed, at the corresponding prepared byte count. Every successful
stop checks ten primitive/two staging result buffers, all inputs, zero commands/
retained owners, no journal loss and complete resource recovery. Stop with a
retained resource returns15 without consuming the engine. These fixed Wasm and
component counts are not total native RSS, JavaScript/GC guarantees or deadlines.

## Focused tests and regressions

Four new standalone native tests (unchanged workloads in both profiles):

- `browser::native_staging::tests::prepared_resource_uploads_immutable_reads_and_nested_programs_match_bounded_actors`
- `browser::native_staging::tests::cancelled_owning_chunks_and_128_program_reuses_preserve_real_buffers_and_generations`
- `staging_wire::tests::canonical_staging_records_preserve_full_width_and_reject_all_truncations_and_padding`
- `staging_wire::tests::staging_client_rejections_preserve_credit_and_nonwrapping_identity_watermarks`

Cancellation covers real owning Write/Read/ProgramBegin/Append returns, unchanged
cursor/receiver progress, a subsequent successful read/complete program, and128
Begin/Cancel reuses **in each direct/threaded mode**, finishing generation130.
Codec tests cover all24 kinds, every request/result truncation, canonical padding,
full-width values, exhausted sequence/generation retirement and invalid-reply
transactionality. Native and both Wasm binding/actual scripts reject record-count,
2^31 and u32::MAX indices without Wasm32 addition wrap or credit consumption.

Two new Python mutation cases require both actual contexts, complete contents/
returns, clocks, nonzero-gap alignment/order, short-call silent tails, memory,
credits and full offline output. `scripts/tests/data/loop_staging_native.json.gz`
(3672 bytes) is compressed **parser data**, not an execution substitute.

Fresh final gate results:

| Gate | Observed result |
| --- | --- |
| Standalone native debug/release |51 passed/profile; fixture doctests0 |
| Shared Node/Chromium bindings |16 groups/runtime/profile, complete reports agree |
| Actual successful contexts |8/profile: primitive, graph, export and staging × offline/live |
| Intentional real failure gates |4/profile, exact diagnostic/exit1/Rust witness |
| Python host tests |15 passed |
| Prescribed nextest workspace |2141 passed, four existing skips;33.925s warm run |
| Warning-denied build/docs, loop doctests, formatting, test-usage/boundaries/tracing |passed |
| Both documented staging CLI modes/profiles and exact scenario filter; coordinated_loop example/profiles |passed |

All Cargo/Python commands used `nix develop --command`, RUSTFLAGS/RUSTDOCFLAGS
`-D warnings`, prescribed nextest0.9.116. Native Rust/cargo1.97.1, Node22.22.2,
wasm-pack0.15.0, wasm-bindgen0.2.127; Chromium/ChromeDriver147.0.7727.137.
The final index-boundary refinement reran both native profiles, all eight actual
contexts/four canaries per profile, bindings, Python, builds/docs and policies.
The workspace result precedes that fixture-only refinement; its production inputs
are unchanged. No warm timing is a cold-cache, speedup or CI-margin claim.

Intentional failures remain primitive/graph processor and Rust-heap canaries,
not newly claimed planner/staging-specific failures. In each profile processor
returns10/origin1/heap0, primitive heap returns0xffff0001/origin2/4,20,4, graph heap
returns0xffff0001/origin2/1,5,1. Failed sessions do not replace successful disposal.

Representative commands (full outputs/commands also in the archive):

```sh
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --staging-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --staging-two-thread-json
python3 scripts/run_loop_engine_hosts.py --profile debug --verify-failures
python3 scripts/run_loop_engine_hosts.py --profile release --verify-failures
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
```

## Provenance, failed attempts and limits

Raw logs and complete final debug/release reports, native/direct/threaded JSON,
Node/browser output, tools/source metadata, ingress/checkpoints/audio/journals and
all canary reports are in `staging-stage7-logs.tar.gz` (592315 bytes), SHA256
`61e6d8df4065252bba3facc1b0bde0853a0935071590f6abc2593402f0489321`.
Final Wasm SHA256:

- debug `156014f192265f4e0c752270c26232899a7c4fc0631c9f239c4a2b17d719dd10`
- release `c21a9e9aa67e1580ba79df72cd3aba62cde9e28bc0361e328ca059ab1aa163cd`

Logs preserve initial compile/API errors, wrong malformed/duplicate expectations,
actual raw-MIDI source refusal, wrong recycle-reply matching, the initial incorrect
fallback-clock audio oracle, a correctly rejected descendant-clock instantaneous
cycle, and the manual semantic table's initial wrong capture revision at Record0.
The fixes use actual typed replies and documented timing/ownership semantics;
production musical rules, guards and interpreter workloads/watchdogs were not
relaxed. The checked-index fix prevents a real Wasm32 overflow rather than masking
it with a smaller test value; all final hosts reran with u32::MAX probes.

No new Miri invocation occurred:27 graph/export/staging standalone cases plus
applicable ingress/observer debt remain for the consolidated Stage7 selection.
Broader clocked composites, dense one-sample wraps/cascades, playable replacement,
capture failure, live transfer/result stalls, planner canaries, CI artifacts,
benchmarks and Stages8–10 remain required. No final PR/CI/review/delivery claim is made.
