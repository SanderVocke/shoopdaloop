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

This is a worklet prerequisite, **not** actual AudioWorklet evidence. Stage 6 remains in progress.
