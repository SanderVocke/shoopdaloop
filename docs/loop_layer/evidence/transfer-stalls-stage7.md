# Stage 7: actual transferred-result buffer exhaustion

## Scope and source

This milestone extends the dense independent loop-host script with an actual
MessagePort response-buffer stall and newly acknowledged playback. The original
LLC1/LLR1 primitive codec, production libraries, manifests and lockfiles are
unchanged. Native replay/bindings and actual browser transfers are distinct proof.

Recorded source base: `c14ce6407b0d2ba2636f25250a69e33b45fe02ee`, with the milestone
implementation dirty (`source_dirty: true`). Both normal runner invocations rebuild
native/Wasm artifacts; the Wasm binaries are byte-identical to the preceding dense
milestone because the Rust changes are native-only support/oracles. This is local
milestone verification, not final-head CI, Miri, performance or deadline evidence.

Raw logs/reports: [`transfer-stalls-stage7-logs.tar.gz`](transfer-stalls-stage7-logs.tar.gz),
3,156,908 bytes, SHA-256
`da6e23c174a5ebcec0b5c83b67418fad8be99fa01560dd3a1e811d23894e392d`.
Archive root: `loop-layer-stalls-stage7/`. `final-reports/{debug,release}` holds
complete native, Node, actual browser and failure-child reports. The parser fixture
`scripts/tests/data/loop_stalls_native.json.gz` (36,092 bytes) is mutation-test input,
not an execution substitute.

## Separate Rust and transfer credits

1. Run the existing dense script through131 frames: four normal credits held
   through128+1 frames, independent recovery Stop129, and Seek130 domain refusal261.
2. Main accepts all ten Rust admission/execution receipts. Rust client/worklet
   outstanding counts become zero. Instead of sending their transport `credit`
   messages, main holds all ten actual96-byte ArrayBuffers with their original IDs.
   The worklet reports zero available response buffers. Rust command credits are
   free; transport buffer credits are exhausted.
3. With every response buffer still held, admit Play131 and Stop135. Acknowledge
   one five-frame call. The retained one-frame take plays four samples; Stop is
   silent, suppresses the due wrap and emits MIDI cleanup. B/C clocks and composite
   cursors continue. These commands arrive after the live context has started;
   explicit engine pauses/acknowledgements, not assumed message deadlines, fix timing.
4. Poll with no transport buffers available. It cannot consume/transfer the new
   replies. Require exactly engine136, worklet outstanding2, main client outstanding2,
   main receipts still10, zero available response buffers and all ten original buffers
   held at96 bytes. Successful stop must refuse15 without consuming the engine.
5. Return every original buffer/ID and check detachment. A new poll delivers the
   four additional receipts. Return their buffers too, then require zero credits,
   all five input/ten response buffers present, successful cleanup and256 resource
   pages/16 versions/16 snapshots/zero media references recovered.

The native direct/capacity-one actor and Node/Chromium binding scripts reproduce
the same engine commands with unpolled returns; they do not claim actual MessagePort
transfers. Native actors keep channel waits/joins outside guarded callbacks. The
browser retains the real transferred ArrayBuffers, not a synthetic counter.
`polled.availableResponses` exposes transport buffers separately from Rust credits;
`transferStall` retains all six mandatory stalled-state fields.

## Complete media, semantic and actual-output checks

Independent native closed-form/scalar tables check every byte, not self-hashes:

| Observation | Expected |
| --- | --- |
| Stereo | 136 samples; original131-frame dense script, `[0.5,5]` at131–134 and silence at135 |
| MIDI | 8,713 complete packets /130,695 bytes, including restart restoration/original note, three wrap cleanups and final Stop cleanup |
| Semantics | 2,042 LSJ1 records /326,760 bytes:136 boundaries,680 complete decisions,1,226 ordered events |
| Wire | Seven64-byte requests/fourteen96-byte results; exact sequence/slot/phases/clocks/status/error fields |
| Ownership | Four final report counts zero;256/16/16/0 stopped recovery |

Native whole/single/irregular/zero partitions match the full report in both actor
modes. The shared semantic oracle is parameterized for the added five frames;
it does not call the production resolver or encoder. Restart resets A's cycles,
while B/C and clocked cursors retain their independent progression.

Actual worklets acknowledge128/1/2/5-frame engine calls. A separate mono probe
marker identifies all four rendered quanta, including the original silent recovery
quantum; it neither modifies nor computes stereo samples. The downstream probe
checks all512 actual samples per channel, including short-call silent tails. Offline
additionally checks all1280 output samples per channel from OfflineAudioContext.
Only actual session IDs normalize in native comparisons; no musical content or
logical clocks normalize.

| Profile/context | Fixed ordinary worklet bytes | Actual quantum origins |
| --- | ---: | --- |
| debug offline | 5,963,776 | 128,384,640,896 |
| debug live | 5,963,776 | 1024,1536,2560,3072 |
| release offline | 5,898,240 | 128,384,640,896 |
| release live | 5,898,240 | 1536,2048,2560,3072 |

Main memory is independently checked unchanged at the same size per profile.
Both memories are ordinary ArrayBuffers in a non-isolated page; main marker
`0x51a17` and worklet marker0 prove separate instances. Heap counts remain0/0/0,
no MIDI/semantic overflow/loss occurs, and all successful sessions recover fully.
Prepared storage remains the dense fixture's finite capacities; no larger musical
quota or buffer resizing is introduced by this milestone.

Unchanged final Wasm SHA-256:

- debug: `076c3f67bf4c9d6846585d888edb4b8db68b9f4a097654810cd0e516aec2ad74`
- release: `9a715720d7352d09b5c8d6548272bf20bfb55816e31854b7222d445b317cfd35`

## Tests and commands

New native case:
`browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled`.
Both profiles pass54 fixture tests and eighteen shared binding groups in each of
Node and Chromium. Each profile passes twelve successful actual contexts
(primitive/graph/export/staging/dense/stall, offline/live), plus four existing exact
Rust-origin intentional failure gates. The latter remain processor/heap canaries,
not planner-specific canaries: exit1, processor code10/origin1/heap0, primitive heap
code`0xffff0001`/origin2/counts4/20/4, graph heap same code/origin/counts1/5/1.

Nineteen Python tests pass, including two new cases rejecting missing stall fields,
new command/result/media/semantic content, restart output, short tails, contexts,
final clocks, memory and recovery. Workspace regression passes2141 tests/four
existing skips in34.408s warm nextest time; this is not callback/cold-cache timing
or a deadline measurement. Warning-denied builds/rustdoc, loop doctests, formatting,
test-attribute policy, dependency boundaries and tracing inventory (207 modules)
pass. Both documented native modes and exact test filter pass in debug/release.

Main logs: `initial-native.log`, `initial-hosts.log`, `release-hosts.log` and
`final-native-workspace.log`; `versions.log` records the toolchain/browser pins.
There was no failing musical/runtime test in this milestone.

Cargo/Python commands run through `nix develop --command ...`, with
`RUSTFLAGS="-D warnings"` and `RUSTDOCFLAGS="-D warnings"`:

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo build --locked --workspace
cargo build --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps
cargo doc --locked --manifest-path tests/loop_engine_host/Cargo.toml --no-deps
cargo test --locked -p shoop_loop_engine --doc
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 scripts/check_tracing_coverage.py --require-closed
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci
python3 scripts/run_loop_engine_hosts.py --profile debug --verify-failures
python3 scripts/run_loop_engine_hosts.py --profile release --verify-failures
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --stall-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --stall-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled -- --exact
```

The last three commands also run with `--release` before `--bin`/the test filter.
The pinned environment remains rustc1.97.1 (`8bab26f4f`), cargo1.97.1 (`c980f4866`),
nextest0.9.116, wasm-pack0.15.0/wasm-bindgen0.2.127, Node22.22.2 and matching
Chromium/ChromeDriver147.0.7727.137, using the existing Nix store browser paths.
No browser absence, replay-only substitution or unrelated exception satisfies gates.

## Remaining scope

This proves cooperating response-buffer exhaustion, newly acknowledged playback,
and eventual complete recovery. It does not bound the browser's underlying
MessagePort queue, arbitrary hostile traffic, JavaScript allocation/GC or live
message deadlines. Engine pauses are driver instrumentation, not runtime quanta.

Stage7 remains5/8. Prepared playable replacement/capture failure, broader interactions,
planner canaries, CI artifacts and consolidated Miri remain open. Thirty
applicable graph/export/staging/dense/stall native cases plus ingress/observer debt
are not yet registered/interpreted; no new Miri invocation is claimed. Stages8–10,
including models/performance/audit, final examples/contracts, source-matched final
verification, independent audit, unmerged PR, required final-head CI and review,
remain mandatory.
