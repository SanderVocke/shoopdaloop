# Runtime implementation evidence

## Stage 0: planning baseline

- Planning commit: `76d1f3d9` (parent proposal `704fbb72`). Implementation branch: `feat/generic-rt-runtime`.
- Existing untracked `.pi/` is harness state and is not staged or modified.
- Read repository index, mandates, coding style, build/test guidance, and CI-debug guidance.
- `docs/rt_runtime/CONTRACTS.md` defines the API/ownership/timing table, A1–A8 test mapping, tiny-capacity cases, and benchmark workloads.
- Workspace membership is already `src/rust/*`. The test policy requires the shared dev-only `shoop_test` attribute. Wasm package discovery uses `[package.metadata.shoop-wasm-test]`, not package-name prefixes.
- Native CI already covers Linux x86_64, Windows x86_64, and macOS arm64 in debug/release. Wasm package discovery is automatic, but dependency auditing and browser-change filtering require explicit additions during stage 7.
- The existing Wasm asset server sends COOP/COEP headers. The independent AudioWorklet fixture needs an explicitly non-isolated origin; ordinary browser unit tests do not prove this.

### Baseline results

Commands executed through `nix develop --command` on NixOS x86_64, before Rust implementation changes:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass. |
| `python3 scripts/check_shoop_test_usage.py` | Pass. |
| `RUSTFLAGS="-D warnings" cargo build --workspace` | Pass. |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci` | 1800 passed, 4 skipped (existing suite). |

The current flake supplies Rust 1.97.1, nextest 0.9.143, Node 22.23.2, and wasm-pack 0.15.0. The baseline nextest result above uses 0.9.143, not the repository-documented 0.9.116; obtain the prescribed version for final verification. Both native and `wasm32-unknown-unknown` targets are installed. The browser runner pins Node 22.22.2 and Chrome/ChromeDriver 147; matching Node/Chromium/driver packages exist in the local Nix store but are not on the development shell PATH. Use those tools explicitly inside the selected shell for subsequent Wasm tests; do not weaken the runner's checks. No baseline browser execution is claimed here.

Raw baseline command logs were retained locally as `/tmp/rt-runtime-baseline-build.log` and `/tmp/rt-runtime-baseline-tests.log`. Future milestone entries must record committed commands/results rather than relying on these temporary paths as permanent evidence.

## Stage 1: core contracts and instrumentation

Implemented validated configuration/budgets, checked IDs and counters, borrowed audio/event buffers, processor preparation/process interfaces, and an empty-graph render endpoint. No graph or control/resource capability is claimed yet.

Verification (through the development shell):

- `cargo test -p rt_runtime`: 12 passed.
- `cargo test -p rt_runtime --release`: 12 passed, including direct allocator/reallocator/deallocator canaries, real collection lifetime detection, and a deliberately allocating callback rejected by the assertion harness.
- `RUSTFLAGS="-D warnings" cargo build -p rt_runtime --target wasm32-unknown-unknown`: pass.
- `RUSTFLAGS="-D warnings" cargo build --workspace`: pass.
- `cargo fmt --all` and `python3 scripts/check_shoop_test_usage.py`: pass.
- `python3 scripts/check_rt_runtime_boundary.py`: pass for Linux, Windows, macOS, and Wasm production/build trees (including all features).
- `python3 -m unittest scripts/tests/test_rt_runtime_boundary.py`: 2 passed; forbidden dependency canaries cover domain, UI, driver, and browser libraries.
- `python3 scripts/check_wasm_test_dependencies.py --package rt_runtime`: pass.
- `python3 scripts/run_wasm_tests.py --runtime node --package rt_runtime`: 10 passed.
- `python3 scripts/run_wasm_tests.py --runtime chrome --package rt_runtime`: 10 passed.

Wasm commands used the cached Node 22.22.2 and matching Chromium/ChromeDriver 147.0.7727.137 inside `nix develop`; reports are in `target/wasm-tests/dev/reports/{node,chrome}`. Native-only unwind tests account for the two-test difference. These are browser unit tests, not the stage-6 AudioWorklet fixture.

Render audit: `process` only checks fixed shape/counts, clears at most configured output channels × frames, checks the frame counter, and updates plain state. Event append validates capacity/order before bounded slice copying; rejected appends leave both arenas unchanged. No reachable locks, waits, heap owners, formatting/logging, I/O, or unsafe code exist in the production crate. The test allocator's unsafe forwarding is isolated to test support with safety comments. Third-party processor behavior remains a documented obligation and will be guarded in fixtures.

## Stage 2: bounded ownership transport

Implemented generic SPSC control/render endpoints using `rtrb`, separate normal/recovery admission lanes, sequence IDs, best-effort cancellation tickets, owning terminal completions with retirement bundles, bounded coherent Copy telemetry, and per-cycle dequeue/publication limits. The transport is independent of the render endpoint until graph/runtime operations are defined. Explicit host stop/join followed by non-RT endpoint destruction is the shutdown contract; there is no internal thread or blocking drain.

Verification through `nix develop`:

- `RUSTFLAGS="-D warnings" cargo test -p rt_runtime`: 24 passed (1 unit, 12 core-contract, 11 lifecycle).
- `RUSTFLAGS="-D warnings" cargo test -p rt_runtime --release`: same 24 passed, including RT heap guards and owning destructor probes.
- `RUSTFLAGS="-D warnings" cargo build --workspace`: pass; Wasm production build also passed during this stage.
- `cargo fmt --all`, shared-test usage, four-target production/build boundary, and Wasm dependency isolation checks: pass.
- Node and Chromium package runners: 20 passed each with the same pinned tools as stage 1. Four native-only tests cover unwind handling, two-thread stress, and Loom.
- Deterministic admission model: 5,000 cycles at each normal capacity 1–4, stalled completions, cancelled/normal requests, FIFO and credit conservation checks.
- Two-thread stress: 10,000 ordered operations, coherent telemetry payload/complement pairs, and per-callback heap assertions. Shutdown exercised in both endpoint orders with queued/current/completed owners.
- Loom explores the standard one-way cancellation flag observation race. It does not model `rtrb` internals; publication and queue ownership use the existing queue implementation rather than a new atomic protocol.

Audit: admitted ownership count bounds the return queue (normal + recovery credits). Credits are released only on control-side receipt, not when commands leave the ingress queues. Unexpected return saturation retains the packet in a fixed slot rather than freeing or retrying indefinitely. Cancellation drops nothing on render; cancellation flags stay in the returning packet even if their tickets are gone. Recovery may reorder against normal operations, intentionally and explicitly; FIFO holds within each lane. RT `complete` misuse returns ownership in its error. All production code remains safe Rust; no mutex, wait, log, format, or I/O call is reachable from render transport methods.

## Stage 3, milestone 1: compiler and descriptor validator (stage incomplete)

Added typed audio/MIDI endpoints and processor signatures, deterministic DAG compilation, immutable relocatable plan envelopes, and a preallocated incremental receiver validator. Port capacities include host and registered node ports. The receiver checks node generations, ordering, duplicates, input ranges, typed endpoints and dependencies without accepting compiler output on trust. Descriptor envelope creation only checks storage bounds; it is explicitly not validation or permission to execute.

Verification through `nix develop`:

- Warning-denying native debug/release tests: 31 passed each (1 unit + 12 core + 7 graph + 11 lifecycle).
- Node and Chromium package runners: 27 passed each with the same pinned tools as earlier stages.
- Warning-denying workspace build, formatting, shared-test usage, and four-target dependency boundary: pass.
- Graph tests cover deterministic declaration-order independence, host passthrough and empty DAGs, typed fan-in/out descriptions, duplicate/cyclic/stale/missing identities, capacity/port errors, malformed descriptor spans, and 256 seeded eight-node DAGs.
- All receiver validation advances run under the heap guard at budgets including one step per callback. Step counts do not depend on the selected batch budget and stay within the linear declared graph-size bound. Production code still forbids unsafe code.

No sample-rendering/reference-interpreter comparison, graph activation, or concurrent rewiring result is claimed for this compiler-only milestone. Those are covered by the following executor milestone.

## Stage 3, milestone 2: execution and transactional activation

Implemented `executor::GraphRuntime` with stable preallocated processor slots, checked same-signature generation reuse, per-slot planar scratch and bounded MIDI arenas, normal-lane structural commands, epoch-checked incremental validation, atomic activation, and displaced-plan/processor return through owning completions. Processor scratch views are contiguous and borrowed; no callback builds vectors of references. Prepared audio/MIDI storage is explicitly touched. Structural commands cannot use the recovery lane and removal refuses active references.

Verification through `nix develop`:

- Warning-denying native debug/release tests: 40 passed each (1 unit + 12 core + 9 executor + 7 graph + 11 lifecycle).
- Node and Chromium package runners: 36 passed each with the pinned tools previously recorded.
- Warning-denying workspace build, formatting, shared-test usage, and four-target dependency boundary: pass.
- Sixty-four seeded eight-node graphs match an independent scalar interpreter for four block partitions. Deterministic binary-fraction gains make comparisons exact; separate multichannel/fan-in/out tests cover summation, unclipped values and untouched output suffixes.
- A stateful counter remains sample-continuous while rewiring incrementally validates, then switches destination only at the activation block boundary. Disconnected outputs remain silent.
- MIDI merge tests preserve payloads (including SysEx) and order by offset and source, with equal-time events retained.
- Stale and malformed plans leave the valid graph audible. Two undrained retirement completions exhaust admission credits without freeing in rendering. Rendering continues after control disconnect.
- Removal of an active node is refused; after disconnecting it, its processor is destroyed only when the control completion is dropped. Stale-generation removal and incompatible-signature reinstall return explicit errors and preserve owning payloads.
- Invalid/zero host blocks do not consume pending transactions or advance graph state. All successful and failing executor callbacks above run with allocation/reallocation/deallocation assertions; processor destructors additionally assert they run outside rendering.

Audit: receiver validation is bounded per callback; activation is ownership moves plus epoch update. Active membership checks are indexed. Port routing uses validated logical IDs and disjoint safe slice borrows. MIDI descriptor insertion is bounded by prepared event capacity; it is intentionally simple rather than an optimized merge implementation. Scheduling and MIDI overload recovery remain stage 4 work; no claim of graceful overflow recovery is made yet.

## Stage 4, milestone 1: MIDI validation and reset state (stage incomplete)

Added bounded complete-message MIDI 1.0 parsing, including short/system messages, note-on velocity-zero classification, explicitly limited SysEx, and rejection of undefined statuses, running-status fragments, malformed lengths/data bytes, and interleaved realtime bytes inside a SysEx packet. `MidiBuffer` now validates messages before accepting them; the generic borrowed `EventBuffer` remains format-independent.

Defined and implemented the small `ResetState` protocol: a separate recovery path emits CC120 then CC123 for all 16 channels; a sink acknowledges only successfully admitted packets. Backpressure retains the current packet and repeated reset requests do not restart recovery. Executor-level reservation/refusal/reporting integration and scheduled operations are still pending.

Verification through `nix develop`:

- Warning-denying native debug/release tests: 45 passed each.
- Node and Chromium package runners: 41 passed each using the previously recorded pinned tools.
- Warning-denying workspace build, formatting, shared-test policy, and four-target dependency boundary: pass.
- Five new guarded tests exhaust all 256 status bytes across short packet lengths, cover all channels and zero-velocity notes, SysEx size/framing errors, atomic writer refusal, and reset completion through a one-event/three-byte sink with repeated backpressure.

Those primitive-only tests are supplemented by the following executor/scheduler milestone.

## Stage 4, milestone 2: exact-frame scheduling and integrated recovery

Implemented fixed-capacity scheduling with conservative event/split reservations and a separate reliable execution-result queue. Pending and published-but-unread results retain credits; native control receipt returns credits for bounded recycling. Admission reports effective frame/lateness separately from execution. The executor splits at accepted timestamps, preserves host MIDI offsets across subranges, limits payload staging by the transfer-byte budget, and reports stale targets or unsupported processor parameters explicitly.

Integrated MIDI overload recovery with reserved 32-event/96-byte storage per arena. Route overflow delivers resets to internal processors; output propagation also queues independent host resets. Ordinary host MIDI stays suppressed while recovery is pending. A separately reserved sink drains resets under a per-port callback attempt budget, retaining unsent packets under backpressure. Repeated overflow does not restart a pending reset. Normal audio continues in tested overflow paths; processors are obligated to handle reset packets and not abandon audio on ordinary MIDI writer refusal.

Verification through `nix develop`:

- Warning-denying native debug/release tests: 60 passed each (1 unit + 12 core + 18 executor + 7 graph + 11 lifecycle + 5 MIDI + 6 scheduler).
- Node and Chromium package runners: 56 passed each, using the same pinned tools as prior milestones.
- Warning-denying workspace build, formatting, shared-test policy, four-target dependency audit, and Wasm dependency isolation: pass.
- Scheduling tests cover sample zero, same-time source/key/sequence ordering, last/next block boundaries, explicitly late frames, counters near `u64::MAX`, stale finish sequences, invalid payloads, future-capacity refusal, undrained result saturation, disconnect, and credit recovery.
- Sixteen seeded variable-block partitions render the same eight-operation parameter journal sample-identically, with exact execution frames. Scheduled SysEx and host MIDI retain their original absolute timing across processor subranges.
- A four-byte callback copy budget admits one four-byte SysEx at a time, delaying only admission of the second message; both execute at their accepted shared deadline.
- Tiny-capacity MIDI fan-in continues audio while triggering host recovery. One-event/three-byte reserved sinks drain all 32 reset packets over bounded callbacks; repeated calls cannot replenish the budget. Normal MIDI resumes after recovery.
- An internal synth fixture starts a voice, then receives a reserved reset after an overflowing input mix would otherwise lose a note-off; the voice stops and all 16 channels receive all-sound-off. This covers internal state, not merely outgoing port resets.
- Recovery-lane reset admission succeeds while the normal command queue is full.

Audit: native render and admission paths remain guarded against heap operations in all these tests. Scheduler vectors only insert within pre-reserved credits; payload pages are allocated/touched during preparation. `rtrb` transports Copy execution records and private recycle tokens; unexpected publication saturation retains a record rather than replaying or losing an operation. Per-callback scheduling is bounded by admitted slots, with at most one extra audio subrange. Ordinary MIDI work is bounded by prepared port/graph capacities; reset work adds at most 32 messages per affected internal port and the declared per-output drain budget. This is not a benchmark claim or a guarantee about arbitrary third-party processors.

## Stage 5, milestone 1: fixed-page uploads and immutable revision leases

Implemented a single-owner `resource::ResourceStore` with fixed identity/version/lease pools, pre-touched page bytes, logical upload reservations, checked metadata, generation-checked transfers and cursors, atomic revision switching, and incremental page/descriptor reclamation. MIDI records validate incrementally without allocating message-sized scratch or rescanning a whole SysEx at completion. Rejected malformed chunks preserve received content/parser state while charging validation work. Immutable uploaded revisions stay readable after replacement/removal until their leases are released.

Verification through `nix develop`:

- Warning-denying native debug/release tests: 70 passed each (the stage-4 suite plus nine resource integration tests and one generation-exhaustion unit test).
- Node and Chromium package runners: 66 passed each, with pinned Node 22.22.2, Chromium/ChromeDriver 147, wasm-bindgen 0.2.127 and wasm-bindgen-test 0.3.77.
- Warning-denying workspace build, formatting, shared-test policy, four-target production dependency audit, and Wasm dependency isolation: pass.
- A stereo audio resource 32 times the callback transfer limit round-trips exactly; a pinned old revision survives a full replacement, is reported stale, and only becomes reclaimable after lease release. Removal changes currentness without invalidating leased bytes.
- Reclamation uses at most 16 configured steps per test callback, independent of the 32-page revision length. All pages, reservations, version descriptors and snapshot slots return after removal/release.
- MIDI headers and payloads are streamed one byte at a time across page boundaries. Stored duration remains independent of the final event. Exhaustive short-message status/length comparisons match the standalone MIDI validator.
- Tests cover missing/reordered/duplicate chunks, incomplete headers, decreasing MIDI times, invalid metadata/overflow, malformed-chunk rollback, exhausted copy/validation budgets, reserved-page competition, cancellation, stale commits, lease exhaustion, stale cursors, resource-ID reuse, and checked generation exhaustion without ABA reuse.
- Resource operations and error paths in integration tests run under the same heap guard as rendering. Metadata and page arrays retain their backing allocations until non-RT store destruction; release only queues fixed-pool recycling.

That first milestone did not integrate the store with executor commands/processors or implement capture. The subsequent capture milestone follows; executor integration/shared callback budgets remain pending.

## Stage 5, milestone 2: capture and immutable prefix snapshots

Implemented atomic interleaved audio/MIDI capture blocks, contiguous checked engine-frame tracking, explicit stopped/incomplete status, and snapshots that share sealed full pages and copy only a mutable partial tail. Appends reserve their entire block's storage/copy work before writing. Exhaustion stops capture with its previous complete prefix intact; it cannot silently resume after losing a block. Snapshot cut frames report the actual captured prefix boundary, even when fetching later. Capture-status changes participate in currentness independently of byte revisions.

Verification through `nix develop`:

- Native warning-denying debug/release tests: 77 passed each; Node and Chromium: 73 passed each with the same pinned tools.
- Warning-denying workspace build, formatting, shared-test policy, and four-target production dependency audit: pass.
- Seven capture tests cover stereo byte-exact prefix snapshots while the original partial tail fills and new pages append; full-page snapshots copy no page bytes; partial snapshots charge precisely their copied tail bytes.
- MIDI capture preserves complete note/SysEx packets, relative event frames, and duration through silent trailing blocks. Old snapshots retain their original duration and bytes after later writes.
- Snapshot refusal due to exhausted copy budget or pages reserved by another upload does not consume a lease or mutate capture. Pool exhaustion preserves a complete prefix and reports its reason, stopped status, length, and cut frame.
- Tests cover mismatched frame positions/channel shapes, invalid MIDI offsets, copy/validation budget exhaustion, frame overflow, stale capture handles after replacement, and no implicit restart after capacity returns.
- Thirty-two seeded variable-length capture blocks produce pinned immutable prefixes across repeated tail mutations. Removal and release reclaim every page/version/lease under bounded maintenance work.
- Native capture, snapshot, error, and reclamation scopes run under the allocation/reallocation/deallocation guard; all page storage remains preallocated.

That milestone still required executor integration and shared callback tests. The following milestone completes those stage-5 requirements.

## Stage 5, milestone 3: executor integration and concurrent transfer fixtures

Integrated typed resource requests with the owning transport, returning write/read buffers in reliable terminal completions, including rejection/cancellation. Requests expose separate Copy replies only on successful application. Queue pending state is not a snapshot-currentness claim. Resource generations/cursors remain validated at execution, and release/cancel/stop operations can use reserved recovery admission. Added `prepare_with_resources` and a restricted processor `ResourceIo` view that cannot reset budgets or perform structural resource edits.

Copy/validation accounting is now callback-wide across scheduled-payload staging, graph validation, resource requests, DSP resource I/O, and incremental retirement. Configured processor headroom is reserved within the total budgets, leaving at least one page/validation step for control progress. Reborrowing views or processing subranges cannot replenish work. Reclamation uses only post-render remaining maintenance steps; sustained fully consumed work can delay recycling and cause explicit pool refusal.

Verification through `nix develop`:

- Native warning-denying debug/release tests: **82 passed each** (2 unit, 7 capture, 12 core, 18 executor, 7 graph, 11 lifecycle, 5 MIDI, 5 resource-executor, 9 resource-store, 6 scheduler).
- Node/Chromium: **78 passed each**, pinned Node 22.22.2 and Chromium/ChromeDriver 147; wasm-bindgen 0.2.127/wasm-bindgen-test 0.3.77.
- Warning-denying workspace build, formatting, shared-test policy, four-target production dependency audit, and Wasm dependency isolation: pass.
- Public-API fixture: four-frame callbacks, 64-byte pages, 128-byte total copy budget, 32-byte processor headroom, 32 validation/maintenance steps, 512 pages, and 16 identity/version/lease slots. An 8192-byte audio replacement (64 times the copy budget) activates atomically while playback and capture continue. Every rendered sample is checked; output changes from 1.0 to 2.0 at application without an intervening silent block.
- The same fixture fetches an immutable capture prefix while later recording continues, fetches a pinned stale audio revision during replacement playback, and round-trips a 3840-byte MIDI resource (30 times the copy budget) with explicit duration. All 512 pages, 16 version descriptors, 16 snapshot slots, and transport credits are recovered after removing processors/resources and releasing leases.
- A full lease pool keeps playback on the prior valid revision. After a lease is released, the processor pins the new revision and switches immediately without an output gap; bounded source looping is also exercised.
- With four total validation steps and one reserved for MIDI capture, every background three-byte MIDI chunk plus capture consumes exactly four steps. Copy accounting includes both serialized capture records and upload bytes. Audio remains unchanged and capture never reports incomplete.
- Paused completions retain owning payloads across 16 callbacks; excess admission refuses. A second same-callback page write reports `Budget`, preserves its bytes, and succeeds when retried. Recovery release overtakes a queued read safely; the read reports a stale lease and returns its untouched buffer. Cancellation returns the original buffer without execution, including when reusing a previously completed request. Oversized buffers are refused before admission.
- A scheduled payload too large for unreserved copy headroom is rejected rather than indefinitely blocking later control work. Dropping control with queued owning payloads and a live snapshot lease causes no callback deallocation; runtime destruction occurs outside rendering.
- Native callback scopes, including resource command failures and pool recycling, remain guarded against heap operations. Page metadata and retirement-stack backing storage are pre-touched during preparation. No resource operation introduces locks, waits, I/O, or whole-resource work in a callback.

This completes the resource milestone, not the host milestone: these Chrome results are browser unit/integration tests, **not an AudioWorklet claim**. Cross-platform CI, actual non-isolated worklet materialization/transfer credits, longer mixed adversarial runs, Miri, performance measurements, and final host equivalence remain required by stages 6–8.

## Stage 6, prerequisite: allocation-free prepared ingress

Added optional preallocated submission flags and reusable plan materialization APIs for a receiver running on an AudioWorklet thread. Ordinary control submission still allocates; `prepare_submissions` plus `submit_prepared` uses fixed, lane-separated flag pools. Held tickets/completions prevent flag reuse, giving explicit `Busy` rather than allowing stale cancellation or allocating a replacement. The endpoint retains each flag's base owner until non-render teardown. Admission scans at most the configured lane capacity, with no retries/waits.

`PreparedPlan::with_capacity` pre-touches descriptor storage. Constant-work reset advances a checked membership stamp; each push copies one typed descriptor record without reallocating. Reuse remains private to the owner of an inactive plan, and it does not bypass receiver validation. Hosts must still bound wire decoding/materialization work and validate declared counts before submitting ownership.

Verification through `nix develop`:

- Native debug/release: **86 tests passed each**; Node/Chromium: **82 passed each**, with the same pinned tools as stage 5.
- Warning-denying workspace build, formatting, shared-test policy and four-target production dependency audit: pass.
- A thousand prepared submissions reuse flags under the heap guard. Cancellation reset, retained results/tickets, and independent recovery flags are exercised; no flag is reused while an old handle exists.
- A prepared executor cycles one owning chunk buffer through 64 submissions/completions without allocation, reallocation, deallocation, or pointer change, guarding ingress as well as rendering.
- A plan is rebuilt and incrementally revalidated a hundred times under the guard. Full capacities, zero-capacity compact plans, invalid descriptors, oversized allocation arithmetic, and membership-stamp exhaustion are explicit refusals.

This prerequisite alone is **not** actual AudioWorklet evidence. The completed host verification follows.

## Stage 6: independent native and actual AudioWorklet hosts

Implemented `tests/rt_runtime_host`, a separate non-publishable helper workspace depending only on the public runtime API and target-specific test bindings. Its controller/compiler, wire codec, prepared render bridge, processor, and native examples do not use any Shoop backend/protocol/host. `scripts/run_rt_runtime_hosts.py` serves a dedicated localhost origin without COOP/COEP and drives a genuine AudioWorkletProcessor through ChromeDriver. Both actual OfflineAudioContext and live AudioContext ran successfully with `crossOriginIsolated === false` and independent non-shared Wasm instances.

Verification on Linux x86_64 (kernel 7.0.3), through `nix develop`, with rustc 1.97.1, wasm-pack 0.15.0, wasm-bindgen 0.2.127, and matching Chromium/ChromeDriver **147.0.7727.137**:

```sh
RUSTFLAGS="-D warnings" python3 scripts/run_rt_runtime_hosts.py
python3 scripts/run_rt_runtime_hosts.py --no-build
RUSTFLAGS="-D warnings" python3 scripts/run_rt_runtime_hosts.py --native-only --profile debug
RUSTFLAGS="-D warnings" python3 scripts/run_rt_runtime_hosts.py --native-only --profile release
RUSTFLAGS="-D warnings" cargo test -p rt_runtime
RUSTFLAGS="-D warnings" cargo test -p rt_runtime --release
RUSTFLAGS="-D warnings" cargo build --workspace
python3 scripts/run_wasm_tests.py --runtime node --package rt_runtime
python3 scripts/run_wasm_tests.py --runtime chrome --package rt_runtime
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_wasm_test_dependencies.py --package rt_runtime
python3 -m unittest scripts/tests/test_rt_runtime_boundary.py
cargo fmt --all -- --check
cargo fmt --manifest-path tests/rt_runtime_host/Cargo.toml -- --check
```

The pinned tool executables were prepended to PATH inside the development shell; `CHROME_BIN` selected Chromium 147. Node regression tests used Node 22.22.2. All commands passed. Core suites remain **86 native tests in each debug/release profile and 82 tests in each Node/Chromium runtime**; two boundary-script tests also pass. The helper's positive allocation/reallocation/deallocation canaries execute at bridge preparation, including release Wasm, and guarded native replay passes in both profiles.

Checked-in structured results: `evidence/hosts-stage6.json`. The golden actual-worklet journal is `tests/rt_runtime_host/data/offline.rtj.gz` (uncompressed SHA-256 `fb8ecf1fae2cb7ac7246bbb836d2ed7670f9d1341b8c005ab3d2565f07ee6237`). Fresh logs/journal/results are produced under `target/rt-runtime-host/reports`; native-only results explicitly say that the browser was not rerun.

Measured/checked evidence:

- **2,601 deterministic offline callbacks / 332,928 frames**. Every response byte and each callback's actual Web Audio sample hash matches both native hosts. Aggregate sample hash: **480331717**; MIDI hash: **3308430490**, **34 events**; independently compiled graph-stream hash: **487351092**. The entire actual offline output, including its post-disposal silent tail, also passes a separate sample-by-sample graph/gain/revision timeline reference.
- The final live run processed **13,444 callbacks / 1,720,832 frames**. Its analyser received the expected worklet audio after replacement. Live hashes/totals are intentionally not equated with offline when message delivery adds idle callbacks; effective frames drive MIDI/capture expectations.
- Both worklet instances stayed at **1,507,328 bytes** of ordinary Wasm memory after preparation. Maximum core work was **1,042 copied bytes / 512 validation-maintenance steps** against bounds **2,048 / 512**. Host wire copies are separately bounded by five fixed 512-byte frames in each direction plus fixed bridge temporaries/payload materialization; they are not hidden in the core counters.
- Four normal requests plus an independent recovery request occupy all five credits. Duplicate-credit and 513-byte-envelope probes return their offending owners. With all five returned buffers withheld, rendering advances at least **2,048 frames** without memory growth. A packet prepared before the stall is delivered afterward: offline lateness is **1,984 frames**, separately confirmed by execution reporting. Cancellation returns the unchanged owning chunk for retry.
- Graph staging, exact-frame gain/MIDI, state-preserving rewiring, stale epochs, malformed duplicate chunks, and stale stream identities are checked. The processor's retired frame counter must equal the accumulated processing duration across rewires.
- Audio/MIDI capture prefixes and old pinned revisions remain immutable while rendering continues. **32,768 audio bytes** and **38,400 MIDI bytes** are uploaded/exported with exact byte checks; MIDI's trailing duration is preserved. These are 16x and 18.75x the callback's total copy budget, not single-callback transfers.
- Shutdown verifies **4,096 free pages**, **16 free version descriptors**, **16 free snapshot leases**, zero reservations/retired pages/transfers/cursors/staging plans, and all seven plan owners accounted for (six free plus one active empty plan). Processor-owned leases are explicitly released. Worklet stop is acknowledged before Rust disposal outside `process`; live context closure and offline post-disposal silence are checked.
- Chromium's AudioWorklet global actually lacks TextDecoder; both reports record the minimal binding-error UTF-8 fallback in use. Successful rendering uses numeric bindings. Rust heap guards do not establish lock freedom or make JavaScript GC/message delivery/OS scheduling hard-real-time; fixed loops and callback ownership paths also received a source audit.

Stage 6 is complete. This is actual worklet evidence, unlike the earlier Chromium unit suites. Windows/macOS CI, longer mixed adversarial/property runs, applicable Miri checks, release performance evidence, deliberate gate-failure demonstrations, and final PR/CI/review delivery remain required by stages 7–8.

## Stage 7, milestone 1: seeded mixed/property coverage and Miri

Added three portable test groups, using the shared test attribute and release-active heap guard:

- `mixed_stress`: eight seeded fresh runtimes combine stateful graph rewiring/stale commits, exact-frame gain/MIDI, audio revision replacement, capture/prefix leases, incremental export, owning cancellation/refusal, five-credit stalls with independent MIDI recovery, and alternate graceful teardown/control-disconnect shutdown. A separate sample/event timeline checks every callback; processor destructor probes reject render-side destruction. The full run observed **22,143 callbacks, 1,008 replacements, 1,003 snapshots, 1,059 consumer stalls, 75/128 peak used pages, and 5/5 peak outstanding command owners**. All graceful shutdown pools recover; disconnected runtimes continue rendering before non-callback destruction and restart.
- `resource_properties`: **65,536 seeded state-machine steps** over small fixed pools model audio/MIDI chunk offsets, incomplete/finished uploads, expected revisions, malformed MIDI rollback, concurrent snapshots/read cursors, resource removal/generation reuse, cancellation, and bounded reclamation. Rejections must preserve pool ownership; reads must match immutable reference bytes. Observed **292 commits, 17,515 verified read bytes, 23,408 explicit refusals, 521 malformed MIDI attempts, 28 resource removals, and full 8/8 version and lease occupancy**. Every seed recovers every page/reservation/version/lease.
- `wire_properties`: the exact host-fixture codec is compiled into the core's test-only module. **16,384 generated/mutated headers** are compared with an independent scalar framing reference; **4,096 relocatable step/edge/endpoint cases** check round trips, every truncation, type/generation/reserved-field corruption, and overflowing offsets under the heap guard. Existing generated DAG/interpreter and malformed receiver tests continue to cover graph-level execution/validation.

Verification through the same pinned Nix environment: **91 native tests passed in both debug and release; 87 tests passed in each Node/Chromium runtime**. Warning-denying workspace build, formatting, and shared-test policy passed. Commands were `cargo test -p rt_runtime -- --nocapture`, `cargo test -p rt_runtime --release`, `RUSTFLAGS="-D warnings" cargo build --workspace`, `python3 scripts/check_shoop_test_usage.py`, and both `scripts/run_wasm_tests.py --runtime {node,chrome} --package rt_runtime` variants. Detailed local log: `/tmp/rt-runtime-adversarial-verified.log`.

`python3 scripts/run_rt_runtime_miri.py` pins **nightly-2026-05-04 / miri 0.1.0 (ad3a598ca4)**, uses default isolation/aliasing checks, rejects missing/zero-test results, and records individual logs plus structured results under `target/rt-runtime-miri`. **36 executions passed**: all contracts, wire/resource properties, reduced mixed stress, capture tests, nine transport lifecycle cases, owning resource cancellation/disconnect, and small two-thread SPSC runs under three interpreter seeds. Checked-in result: `evidence/miri-stage7.json`. An additional local Miri run of the original executor's state-preserving rewiring test also passed; it is not counted among the scripted 36.

On this NixOS host the shell's RUSTUP_HOME is read-only. Installing Miri there failed with an explicit read-only-filesystem error; the supported workaround was a separate writable toolchain cache, not skipping Miri:

```sh
export RUSTUP_HOME="$HOME/.cache/rt-runtime-miri-rustup"
rustup toolchain install nightly-2026-05-04 --profile minimal --component miri,rust-src --no-self-update
python3 scripts/run_rt_runtime_miri.py
```

The interpreter runs explicitly smaller seeded counts and 32 two-thread operations; normal debug/release and applicable Wasm tests retain full counts, including 10,000 native two-thread operations. Loom's generator/FFI model and the long transport admission model run in the native suite rather than the selected Miri job. Browser/Web Audio/JavaScript and external C++ tracing are not interpreted; their existing real-host/native gates remain separate. This is selected ownership/aliasing/concurrency evidence, not a whole-workspace/all-tests Miri claim. No new unsafe production code was introduced.

### Stage 7, milestone 2: release measurements and real failure gates

Added `examples/bounded_runtime.rs` and `scripts/benchmark_rt_runtime.py`. Seven workloads check scalar audio/MIDI references, capture/upload bytes, retirement, pool recovery, and per-callback heap/work bounds while collecting reproducible release timing distributions and raw callback CSVs. Three trials measured **420,000 callbacks**, with **zero observed deadline misses**, maximum work **2,048 copied bytes / 128 validation steps**, and six deliberately failing native heap probes (debug/release allocation, reallocation, deallocation; exit 101). Configuration, hardware, throughput, memory accounting, preliminary tail investigation, and limitations are in `PERFORMANCE.md`; structured evidence is `evidence/benchmark-stage7.json` and `evidence/benchmark-stage7-preliminary.json`. Timing is not a hard-real-time proof or CI threshold.

`RUSTFLAGS="-D warnings" python3 scripts/run_rt_runtime_hosts.py --verify-failures` passed the full real offline/live scenarios and both native replays, then verified two fresh browser sessions **fail with exit 1** for an injected processor exception and an actual Rust heap violation inside the worklet guard. A missing binding, unrelated exception, or timeout cannot satisfy the canary. `evidence/hosts-stage7-failure-gates.json` records both failures and the preceding successful host run; normal offline hashes and fixed memory size are unchanged.

Repeated gates through Nix: **91 tests each native debug/release, 87 each Node/Chromium**, warnings-denied workspace build, both workspace/helper formatting checks, shared-test policy, four-target dependency audit, and native debug journal replay. The first final tracing-inventory audit exposed the new crate's 17 uninventoried modules. They are now explicitly classified: this standalone library has no application tracing dependency, exposes bounded Copy diagnostics, and is not integrated into application execution. The closed inventory audit then passed; no tracing-policy rule was weakened. Logs: `/tmp/rt-runtime-performance-and-failures.log`, `/tmp/rt-runtime-bench-final-gates.log`, and `/tmp/rt-runtime-bench-followup-gates.log`.

### Stage 7, final milestone: API guide, processor overload audit, and CI scheduling

Added the executable `gain_graph` example, verified in debug/release: guarded public-API graph activation, a gain change at frame offset 64, separate admission/execution checks, empty-graph activation, owning processor removal, and non-callback destruction. `GUIDE.md` covers capacity arithmetic, shared budgets, recording/pinned-version sizing, processor obligations, explicit shutdown, host support, and exclusions. `DESIGN.md` matches the implemented architecture.

The supplied worklet processor audit found that capture exhaustion propagated into unrelated audio, and an ordinary MIDI writer refusal could precede its private frame-count update. Capture pool/budget/revision exhaustion now preserves the reported incomplete capture while audio continues; audio/state advancement finishes before returning MIDI overflow for core reset recovery. Two portable guarded regression tests import the **exact fixture processor source** and verify continued samples/state, incomplete metadata, MIDI refusal, and complete pool recovery under capture exhaustion and 32 reset packets. These tests also pass under Miri.

Final local verification: **93 native tests each debug/release; 89 tests each Node/Chromium; 38 selected Miri executions**. Both profile variants of `gain_graph`, native debug/release journal replays, the complete fresh real offline/live worklet scenario, and both expected browser failures passed. Normal offline sample/MIDI/graph hashes remain unchanged. Structured final evidence: `evidence/miri-stage7-final.json`, `evidence/hosts-stage7-final.json`. Logs: `/tmp/rt-runtime-stage7-final.log`, `/tmp/rt-runtime-stage7-miri-final.log`.

CI scheduling in `.github/workflows/build_and_test.yml` now includes:

| Acceptance | Scheduled evidence |
| --- | --- |
| A1 | Four-target dependency audit and its unit tests on native jobs; Wasm dependency audit includes the crate; existing application gates remain |
| A2 | Explicit core tests, gain example, and offline/two-thread replay on all six native debug/release cells; metadata discovery in Node/Chromium; actual non-isolated worklet on web-debug |
| A3 | Release-active guards in native/core Wasm tests and actual host path; six native violation probes in Linux-release benchmarks; two real-worklet failure probes |
| A4–A6 | Reference DAG/scheduler/MIDI/resource/capture and generated mixed/property suites through core discovery on native/Wasm; cross-host journal checks |
| A7 | Lifecycle, seeded ownership/pool stress, native Loom/two-thread tests, explicit shutdown checks, and a dedicated pinned Miri job |
| A8 | Linux-release benchmark artifacts with no timing threshold; Miri logs, native-host reports, worklet reports/failure diagnostics, and existing shared Wasm artifacts |

The Chromium change filter includes the crate, helper, and runner. Miri runs on every normal workflow invocation, with a 40-minute job limit and pinned nightly-2026-05-04. Native Linux-release records a shorter 4,096-callback benchmark trial using the same assertions; timing never decides pass/fail. Workflow syntax/action checks pass with actionlint 1.7.12 (`-shellcheck=`); its full shellcheck invocation reports four pre-existing SC2251 notices, reproduced against the committed baseline, and none in the added steps.

Repository gates also passed: formatting, warnings-denied workspace build, shared-test policy, four-target boundary and Wasm isolation, boundary unit tests, and the closed tracing inventory. Using the prescribed **cargo-nextest 0.9.116**, the complete workspace suite passed **1,893 tests with the same four pre-existing skips** (`SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`; `/tmp/rt-runtime-full-repository-stage7.log`). The pinned musl nextest release was downloaded to a separate user cache and checked against its published SHA-256; the shell's newer nextest was not used for that gate.

Existing application regressions also passed: warnings-denied Wasm application check, release production-worklet build, `trunk build`, and the hosted output-only Chromium smoke at 900×600. Commands came from the application README; no application source was integrated or altered. Log: `/tmp/rt-runtime-application-browser-regression.log`.

Stage 7 is complete as implementation, local verification, and CI scheduling. Configured jobs are not claimed as already executed.

## Stage 8: final audit and remote delivery (in progress)

PR: https://github.com/SanderVocke/shoopdaloop/pull/878. `AUDIT.md` records callback entry paths, finite loop/budget arguments, owning return/retirement/drop paths, resource and scheduler invariants, processor obligations, and the no-integration diff check. The final arithmetic audit added a preparation-time bound for aggregate diagnostic work across scheduled subranges: large individually valid capacities must not overflow wasm32 counters. Guarded contract tests cover rejecting products/aggregate counts without allocating, and accepting a large valid configuration. The prepared-plan allocation-overflow test still independently exercises plan metadata sizing using a one-frame configuration below the new work bound.

The final local pass repeats **93 native release tests**, the full **1,893-test debug workspace suite (four baseline skips)** with nextest 0.9.116, **89 Node / 89 Chromium tests**, warnings-denied workspace build, formatting/policies, all-target dependency audit, and the real offline/live host plus both failure gates. An additional isolated Miri run passed all **12 contract tests** after the arithmetic change. Native offline/two-thread responses and actual Web Audio hashes remain identical. Logs: `/tmp/rt-runtime-stage8-final-local.log`, `/tmp/rt-runtime-stage8-miri-contracts.log`. Final host result: `evidence/hosts-stage8-local.json`.

The initial PR could not start CI because upstream dependency updates conflicted in `Cargo.lock`. Merge `5f1a0676` preserved upstream dependency selections, passed the warning-denying locked build and complete 1,893-test suite, and enabled CI run **40143833102**. At that head, macOS debug/release, Linux debug/release, web release, and Miri completed successfully; other cells were still running when inspected. Those are not claims about a later head.

The first coverage job failed in the existing application test `app_backend::tests::audio_port_peak_state_is_per_poll_cycle` (expected 0.8, observed 0.0), not a runtime test. Its uploaded Perfetto trace was inspected with the repository-pinned processor: bounds 292332449352–292444342183 ns, one audio callback, and no reported lost/drop/error stats. A graph-apply span overlaps that callback; this is evidence of timing overlap, not a proven root cause. Twenty unchanged local repeats with tracing/detail enabled passed. The failure and trace/log evidence are retained under `target/rt-runtime-ci` and `/tmp/rt-runtime-coverage-*`; no application code or test was changed, no test was skipped, and no CI gate was weakened. A new-head CI result remains required.

No automated review or review request was present at the latest inspection. Final green required checks, current-head Windows/macOS evidence, and the final review check remain open; stage 8 is not complete.
