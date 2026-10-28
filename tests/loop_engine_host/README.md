# Independent loop-host fixture

This test-only workspace depends on `rt_runtime` and the actual `shoop_loop_engine`,
not the old backend, application or devices. Its own lockfile pins the same `rtrb`
version as the repository and host-only `wasm-bindgen` 0.2.127. The Python browser
runner reuses the generic host's HTTP/WebDriver utilities and decoder fallback;
the generic runtime fixture does not depend on loop semantics.

## Run native replay

From the repository root, inside the repository development environment:

```sh
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --release --bin replay
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --wire
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --two-thread
```

For complete native-only evidence without requiring Node, Chromium or Wasm tools:

```sh
python3 scripts/run_loop_engine_hosts.py --native-only --profile debug
python3 scripts/run_loop_engine_hosts.py --native-only --profile release
```

This runs primitive direct/wire/two-thread replay, all six graph/export/staging/
dense/stall/media direct/actor pairs and the planner control. Each Rust replay
checks its independent oracle; Python additionally compares complete actor reports.
The dense report checker compares the complete semantic byte stream, then indexes
its event tuples off-render for supplementary membership assertions. Event order,
multiplicity, all samples/MIDI/receipts and every replay partition remain checked;
the index is not callback storage or a musical-work limit.

Reports go to `target/loop-engine-host/reports/native-<profile>/`, separately from
actual browser results, and explicitly identify `native-replay`. Native mode refuses
actual-worklet failure-gate flags; it cannot substitute for those sessions.

Add `--release` before `--bin` for optimized wire/two-thread replay. Each mode
compares all four partitions with the independently checked direct result before
printing a summary. No device is needed. The threaded driver uses one-packet
channels between distinct control/render threads; blocking channel operations and
join occur outside guarded callbacks. This is offline replay, not live scheduling
or deadline evidence.

The native test suite also includes an independent descriptor-bank state model:

```sh
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --lib \
  graph_transfer::model::seeded_descriptor_bank_model_preserves_complete_tables_and_exact_owners_after_every_action -- --exact
```

Four seeds run512 mixed actions each, comparing every descriptor/planning table,
sequence/epoch/generation and retained owner's storage after each action. Interleaved
banks, partial/aborted payloads, stale/foreign/malformed/replayed records, capacity
refusal, wrong owning returns and generation/sequence exhaustion preserve prior
state. Mutating receiver calls are heap-guarded; the allocating reference stays
outside callbacks. This is payload/ownership validation, not proof that Seal admits
a semantically valid runtime graph or a fresh browser/Miri execution.

### 1. Prepare separate playback, FX and capture roles

Preparation creates one primitive loop and a graph that routes live stereo/MIDI
and loop playback through a mix/half-gain processor to the host and capture role.
All runtime buffers, media metadata and two capture banks exist before rendering.
The render configuration permits 128 frames per callback and sets the obsolete
subrange budget to zero. The native driver allocates its result report outside
callbacks. Its semantic bytes come from a separate fixed-capacity boundary observer;
the allocating report itself is not callback storage.

### 2. Record, play and stop at exact frames

Submit Record at frame 0, Play at frame 4 and Stop at frame 12 before the first
callback. The four live left samples are `2, 4, 6, 8`; right samples are ten times
left. There is no live input after frame 3. MIDI contains note 60 on at frame 0
(velocity 100) and off at frame 2 (release velocity 45).

The FX output recorded during `[0,4)` is `1, 2, 3, 4`. Playback passes through the
same half-gain FX, so `[4,12)` is two copies of `0.5, 1, 1.5, 2`. Frame 12 is
silent. These expectations are calculated independently from frame numbers,
not from resolver decisions or recorded output.

### 3. Check complete output and ownership

The driver compares every stereo sample and all MIDI packets across `[13]`,
thirteen one-frame calls, `[3,5,2,3]`, and `[0,4,0,8,0,1,0]`. Zero calls do not
settle or advance a boundary. The three receipts must each have identical requested,
delivered and terminal frames: 0, 4 and 12.

The semantic journal records four musical boundaries at0,4,8,12, not ordinary
callback cuts. It contains four decision snapshots and seven ordered events:
Record/Pulse, Play/Pulse, Wrap/Pulse, then Stop. Stop at12 suppresses the otherwise
due wrap/pulse. An independent byte table checks the complete journal across all
partitions and drivers. See [LSJ1](WIRE.md#semantic-boundary-journal-lsj1).

MIDI restoration at frames 4 and 8 precedes the original note-on. The captured
initial state has neutral pitch and released CC64/69/66 on all 16 channels;
the independent expected packet list includes those 128 restoration packets,
not merely the six original/replayed note packets. The total is 134 packets.

Every engine callback is heap-guarded. In wire mode, guards also cover ingress,
prepared submission, owning completion collection and response-slot recycling.
Positive allocator instrumentation and three expected-panic tests prove allocation,
reallocation and deallocation are detected in both native profiles. They do not certify JavaScript allocation or
browser callback behavior.

After Stop, drop control and extract the stopped engine off-render. Explicit
cleanup and bounded reclamation must recover all 256 pages, 16 versions and
16 snapshots, with zero media references.

## Submit through the bounded bridge

The [wire guide](WIRE.md) describes the fixed request/response records and shows
the `Client` → `Bridge` → `Client` cycle. Four normal credits and one independent
recovery credit retain admission and terminal results separately. Request sequence,
session, slot and target generation checks prevent accidental reuse or retargeting.
The implemented commands are absolute-frame Record, Play, Stop and Seek; recovery
admits only a nonfuture Stop. Transport cancellation is best effort before dequeue.

Tests cover complete packet layout/high frame bits, malformed/truncated records,
foreign/duplicate/reordered receipts, stale targets, cancelled submissions, late
execution, 512 requests across 128 slot-reuse rounds, and future-request teardown.
With all four normal responses unread, music continues and the independent Stop
credit still works. A callback error permanently faults this bridge: extract and
clean up off-render rather than replaying frames or inventing terminal results.

## Transfer prepared graph descriptors (payload foundation)

Run the native receiver→real-facade route-cut test in either profile:

```sh
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  graph_transfer::tests::receiver_descriptors_stage_activate_and_recycle_real_facade_graph_owners -- --exact
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release \
  graph_transfer::tests::receiver_descriptors_stage_activate_and_recycle_real_facade_graph_owners -- --exact
```

1. Prepare `graph_transfer::Receiver::new(session, config)` before rendering. Its
   two local banks own full-capacity descriptors; `id(slot)` supplies the current
   slot/generation and `capacities(id)` reports the actual available storage.
2. Encode `Record` values with Begin, ordered node steps/edges, explicit planning
   metadata/bindings and Seal. Pass exactly96 bytes per `Receiver::ingest` call.
   [LGP1](WIRE.md#prepared-graph-descriptor-payload-lgp1) defines every offset,
   canonical zero and target-width check. Seal means complete payload, not valid graph.
3. `take(id)` hands a local prepared owner to `Controller::stage_graph`. Actual
   runtime validation returns either the candidate or a Ready scoped ID. Only
   then request `GraphActivate` at an admitted future frame.
4. Match owning returns to their local handoffs and call `recycle(id, plan)`.
   It advances the generation without destroying owners on the callback. A returned
   old graph may have smaller capacities; the next Begin checks those actual bounds.
   Rejected returns retain their input for off-render quarantine.

The native test stages routes at frame0, observes Ready after one sample, then
activates a host-output bypass at frame4. Capture still receives half-gain FX;
host playback now goes directly from the playback role. All13 left samples must be
`1,2,3,4,1,2,3,4,1,2,3,4,0`, with right ten times left, the same134 MIDI packets,
exact receipts/epochs, unchanged semantic timeline and complete resource recovery.
Three partitions after the common one-frame staging acknowledgement must agree.

Seven guarded native tests additionally check canonical full-width records, every
truncation/padding byte, bank ordering, capacity/identity/sequence/generation limits,
128 candidate cancellation/refusal/release rounds, unread owning returns while music
continues, full ingress refusal and queued/Ready disconnect disposal.

`BrowserGraphReceiver` runs the same materializer through public Node/Chromium
bindings. Its extra checks reject oversized u64 indices rather than truncating
on Wasm32 and perform128 local owner exchanges without memory growth. These are
**not** graph commands or runtime graph activation through the worklet. `LLC1`/`LLR1`
still carry only primitive transitions. The separate native envelopes below now
connect graph ownership to bounded results. Public graph command bindings below
now execute that same runtime path in Node/Chromium; MessagePort dispatch remains open.

## Use the native graph command bridge

Run its complete wire/route-cut case in either profile:

```sh
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  graph_wire::tests::actual_bridge_graph_staging_activation_and_retirement_match_all_media_and_receipts -- --exact
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release \
  graph_wire::tests::actual_bridge_graph_staging_activation_and_retirement_match_all_media_and_receipts -- --exact
```

1. Prepare `Bridge::with_graphs(session)` and `graph_wire::Client::new(session)`
   off-render. This opts into two runtime graph candidates, two descriptor owners
   and two graph result credits; the default primitive bridge stays unchanged.
2. Send the ordered `LGP1` records to `Bridge::graph_payload`, then prepare a
   `graph_wire::Operation::Stage` and pass its96-byte envelope to `graph_ingest`.
   The payload Begin epoch must match the envelope. The runtime separately checks
   its actual current epoch and complete signal/planner catalog.
3. Poll `next_graph_response` and validate each128-byte record with the graph
   client's `receive`. Staged supplies a scoped Ready ID, not permission to assume
   routing is active. Activate uses that ID and a future absolute frame, with
   separate admission and execution results.
4. Release each staged/taken graph and consume its owning result. It carries the
   original bank's next checked generation and actual capacities. Before normal
   stopped cleanup, verify both clients' command counts and both sides' retained
   graph counts are zero. Abandonment/disconnect is a separate off-render path.

[LLG1/LLS1](WIRE.md#native-graph-commandsresults-llg1--lls1) specify every field,
outcome and owner handoff. All graph commands use the normal lane; primitive Stop
recovery remains independently credited. One callback collects at most the fixed
seven external credits per stream, never a renewed quota per loop interval.

Eight additional native tests check independent complete codec tables, malformed
response/identity/phase/epoch preservation, nonwrapping sequences, real serialized
activation and retirement across three partitions, all four normal primitive and
two graph credits stalled while music/recovery continues,128 candidate lifecycle
rounds, queued/Ready disposal, local Full refusal and intact owning quarantine.
The route-cut script has the same13 samples/134 MIDI packets described above and
retains exact graph/primitive receipts, semantic bytes and complete pool recovery.

The graph protocol now executes natively and through public Node/Chromium bindings.
Native bounded two-thread graph transport now executes the same complete graph
script through capacity-one channels. The original replay modes remain primitive;
`--graph-json`, `--graph-two-thread-json` and additional actual worklet contexts
execute the acknowledged graph-change scenario below.

## Submit graph commands through public Wasm bindings

Build and run the browser command below first, then run the same public-binding
checks directly in Node:

```sh
node target/loop-engine-host/debug/node_checks.mjs
node target/loop-engine-host/release/node_checks.mjs
```

1. Create `BrowserGraphClient(session)` in the control instance and
   `BrowserBridge.new_graphs(session, 0)` in the rendering instance before measuring
   fixed memory. The client compiles this fixture's host-output-bypass descriptor
   through the public controller API off-render; it is not a general graph editor.
2. Iterate `payload_count()`. Call `prepare_payload(index, bankSlot, generation,
   epoch, sequence)` and copy exactly96 bytes from local `payload_ptr()` to the
   render instance's local `graph_payload_ptr()`. Use BigInt for all64-bit arguments;
   payload sequences are separate from the client's automatic command sequences.
   `ingest_graph_payload()` must return0. This alone neither admits nor activates.
3. Call `stage(bankSlot, generation, runtimeSlot, epoch, cancelled)`. Copy96 bytes
   from local `request_ptr()` to local `graph_request_ptr()` and require
   `ingest_graph() === 0`. Process a real callback, poll `poll_graph_response()`
   (0 none/1 ready/high-bit protocol error), copy128 bytes from local
   `graph_response_ptr()` to client `response_ptr()`, then require `receive() === 0`.
4. A Staged response supplies the Ready scoped ID. `activate(scope, slot, generation,
   at, graphEpoch, topologyEpoch, cancelled)` retains credit until both admission
   and execution are consumed. The shared binding case first renders one sample
   for Ready, then activates at4 and checks every route-cut sample/packet/journal byte.
5. `release(scope, slot, generation, cancelled)` returns the candidate/retired graph.
   Validate its next bank generation and actual capacities before the next Begin.
   The shared check repeats128 real stage/release cycles on the smaller returned
   owner, with fixed memory and guarded preparation-free Rust paths throughout.
6. Require both sides' command/retained counts zero before successful `stop()`.
   Stop returns15 without consuming the engine if a graph remains Ready/Taken,
   even when command credits are empty. Fault abandonment stops callbacks and frees
   real owners off-render; it does not manufacture client returns.

[The binding recipe](WIRE.md#public-graph-command-bindings) is runnable after the
runner builds assets. These binding checks are distinct from the additional
MessagePort/downstream Web Audio graph scenario below. Native binding-buffer
coverage also compares three partitions and actual processor/heap fault disposal.

## Run actual Web Audio

Inside `nix develop`, with Node, wasm-pack, pinned Chromium/ChromeDriver on `PATH`
and the `wasm32-unknown-unknown` target installed:

```sh
RUSTFLAGS="-D warnings" python3 scripts/run_loop_engine_hosts.py --profile debug --verify-failures
RUSTFLAGS="-D warnings" python3 scripts/run_loop_engine_hosts.py --profile release --verify-failures
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
```

The shared browser launcher currently accepts Chromium/ChromeDriver
147.0.7727.117 or 147.0.7727.137. `CHROME_BIN` can select the browser; the matching
`chromedriver` must be on `PATH`. wasm-pack provisions the matching binding CLI;
no separate `wasm-bindgen` executable is required. Missing tools are failures,
not skipped browser evidence. `--no-build` deliberately reuses that profile's
existing artifacts; omit it for fresh verification.

Each normal runner invocation requires sixteen successful sessions: primitive,
graph-change, immutable-export, resource/program-staging, dense musical, transferred
result stalls, prepared media and planner-control scenarios in both offline/live
contexts. `--verify-failures` additionally checks six intentionally failing sessions.

### 1. Prepare two ordinary Wasm instances

The main instance owns `BrowserClient` (and `BrowserGraphClient` for graph sessions); the AudioWorklet instance owns
`BrowserBridge` and the actual loop facade. Both instantiate the same compiled
module, with separate ordinary memories. A main-instance marker is absent in the
worklet; both memories must be non-shared. The server sets no COOP/COEP headers
and the page must not be cross-origin isolated.

Pointers returned by the binding exports address **only their own instance**.
JavaScript copies the fixed wire records through five prepared 64-byte request
buffers and ten 96-byte response buffers. Transfers detach the sender's buffer;
returned buffers are checked before reuse. Request-buffer return is merely a copy
acknowledgement, not admission or execution success. The Rust client's wire credit
remains occupied until its reliable terminal response.

### 2. Arm an acknowledged engine clock

Stage all three commands before arming the engine. Startup Web Audio callbacks
are silent and do not call the engine. After the arm acknowledgement, the first
actual render callback establishes the mapping from engine frame zero to the
browser's `currentFrame`. Offline startup uses one silent quantum. Live startup
makes no assumption about MessagePort delivery time or browser frame zero.

Both contexts render stereo through a separate downstream AudioWorklet probe.
It records the first non-silent quantum and the following one into fixed arrays;
its start must match the source's acknowledged clock mapping. Compare every
sample with the scalar oracle, including the silent remainder. The offline case
also checks all 512 samples per channel from `OfflineAudioContext` itself, not
only the probe or a Rust self-hash. Live output is muted **after** the probe.
The runner independently compares complete MIDI packets and execution frames
with native replay. This short live test stages commands before clock start;
it is not yet a campaign of dynamically arriving live commands.

### 3. Change routing through an audible Ready checkpoint

The graph scenario uses the same Record0/Play4/Stop12 music, but transfers all20
compiled descriptor records and Stage before the first engine callback. It first
rejects a reserved-byte payload and a foreign-session command, then retries their
original sequences; neither refusal may lose the prepared owner or credit.

1. `prime` requests **one real engine sample**, not a warm-up. Its audio is heard
   and captured by the downstream probe: left1/right10 at engine frame0. The
   `checkpoint` acknowledges engine frame1 and that browser quantum. Engine
   advancement then pauses; Web Audio callbacks during the wait output silence.
2. Poll and consume the actual Staged/Ready result. Submit Activate for engine
   frame4 while the acknowledged engine clock is still1. Arm/resume only after
   its envelope acknowledgement. The next128-frame engine callback covers1–128;
   activation is three samples inside that callback, not delayed to a quantum.
3. The probe retains the two non-silent Web Audio quanta in fixed storage. Require
   both browser starts to match source clock markers. The first contains only
   engine sample0 followed by127 silent pause samples; the second contains real
   engine samples1–128. Together their129 engine samples must match native replay:
   left `1,2,3,4,1,2,3,4,1,2,3,4,0`, then zeros; right ten times left. Compare all134
   MIDI packets, all semantic bytes, primitive receipts and graph publication epoch2.
4. Release the taken graph. Its ingress acknowledgement names the current engine
   frame; the owning completion must report that same frame. Verify next bank
   generation2 and actual retired capacities3 steps/12 edges/2 bindings, zero
   command/retained counts and every input/response buffer back before stop.
5. Offline additionally compares every sample of both complete768-frame output
   channels, including startup, pause and stopped silence. Live mute remains after
   the probe. Both contexts keep ordinary memories fixed and recover256/16/16/0.

This is an intentional **test-driver engine pause**, not continuous wall-clock
live editing, device capture or a MessagePort deadline guarantee. It consumes no
hidden musical frames, drops no admitted transition and does not impose a runtime
interaction quantum. The separate primitive worklet still verifies contiguous
128-frame processing after its initial arm.

Run the native reference (replay, not a worklet) and partition test in either profile:

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --graph-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --graph-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_graph::tests::acknowledged_graph_worklet_script_has_complete_partition_invariant_native_reference -- --exact
```

Add `--release` before `--bin` or before the test filter. The reference records129
samples,20 payloads, three graph commands, four graph results, six primitive results,
full MIDI/semantic bytes, five actual zero ownership counts and resource recovery.
Whole/single/irregular/zero partitions agree in both direct and capacity-one native
actor transports. Graph descriptors/commands/results cross the native control/render
boundary as copied protocol bytes; the render thread constructs and retains every
graph owner. Native step/sample messages coordinate the test driver. Channel
waits/joins and variable-sized post-stop diagnostic copying happen outside guarded
callbacks. The runner requires complete direct/threaded report equality (only
opaque local graph scope normalizes) and retains both raw reports. See [transport and clock rules](WIRE.md#graph-messageport-transport-and-clock).

### 4. Check memory, failures and shutdown

Rust guards cover preparation-free ingress, source filling, planner/render work,
MIDI observation and return recycling. Memory length must stay fixed after
preparation. JavaScript allocation/GC, structured cloning and browser scheduling
are outside these Rust heap guarantees; there is no deadline certification.

Twenty shared public-binding groups execute in Node and Chromium: constructor
refusal, full-width identities/frames and independent credits, rendering and
receipt validation, fixed-memory diagnostic exhaustion without changing audio,
successful recovery, actual processor/heap failure witnesses, semantic boundary
records, an eventless same-position seek, two payload checks and three graph command
checks: full-width/credits, exact activation/retirement and128 real owner returns.
The export, resource/program-staging and dense musical groups also retain complete native-matched
reports. A separate restart group checks the engine script used during actual
response-buffer exhaustion. The media group checks prepared playable replacement,
tick-delayed recording and coherent incomplete-prefix export. These are binding-level
Wasm checks, not a claim that the59 native Rust tests were executed as Wasm test functions.

The observer prepares128 fixed160-byte records by default, or4096 for the dense and media scenarios. It records event-bearing boundaries,
playback discontinuities and changes in resolver journal loss. It omits ordinary
callback cuts and never invents a transition or pulse for an eventless seek. If a
whole boundary's decisions/events do not fit, it drops that entire diagnostic
batch, increments saturating loss counters and leaves music and receipts alone.
Native tests vary observer capacity and resolver journal capacity across72 runs;
Wasm checks also exhaust the observer while verifying all audio and receipts.

`--verify-failures` launches six intentionally failing **real worklet** sessions:
processor and heap faults in both primitive and graph hosts, plus planner error and
planner heap faults. The FX processor or test-only resolver planner returns an error
or allocates/reallocates/deallocates. Individual gates can be run with
`--failure-canary processor`, `rust-heap`, `graph-processor`, `graph-rust-heap`,
`planner` or `planner-rust-heap`.
Each child must exit nonzero with the specific diagnostic, matching Rust processor
witness and heap counts. An unrelated exception or matching text without witnesses
cannot pass. These failed sessions do not stand in for successful disposal.

For successful primitive sessions, collect all six admission/execution records, return all
request/response buffers, and stop invoking the engine before explicit cleanup
and destruction. Both contexts check complete pool recovery. The live context is
then closed; the offline context renders its remaining silent tail.

Reports are under `target/loop-engine-host/reports/{debug,release}/`. `result.json`
uses `LLH2` and contains complete sample arrays, wire/MIDI/semantic records, tools,
memory, ownership and failure-gate results. See [the report formats](WIRE.md#browser-report-llh2).
The runner rejects malformed or incomplete semantic data, then compares every byte
with the native independent table. This journal is extracted after callbacks stop;
it is not live telemetry or an immutable media export.

## Retain and read immutable capture exports

The default runner additionally executes this full scenario through native direct/
capacity-one actor replay, Node/Chromium bindings and actual offline/live worklets:

1. Record0/Play4/Stop12; Export Latest at8. Its settled request cut is8, but both
   original source snapshot cuts remain4, with coherent capture stamps0–4.
2. Keep that export alive while Record128/Play132/Stop140 creates revision2.
   Input128–131 yields stereo10,20,30,40 (right×10), distinct from revision1's
   1,2,3,4. Export Latest at136 retains source cuts132 and stamps128–132.
3. Read both retained exports through local cursors and a reusable64-byte owning
   resource buffer. Require exact32-byte stereo data per snapshot; MIDI is the
   original note-on0/note-off2 for revision1 and empty for revision2. Query all16
   initial channel projections per snapshot, including every key/controller and
   program/bank/pressure/pitch/reset/parameter-history field—not just channel0.
4. A successful stop must refuse15 while either export remains retained. Release
   both explicitly, require next local generations2, all command/owner counts0,
   every transfer buffer returned and256/16/16/0 resource recovery.

Actual worklets use44 explicitly acknowledged128-frame steps. Between steps the
engine pauses and Web Audio emits silence; no hidden engine time advances. A probe
observes the two audible quanta and requires source-clock agreement. Offline also
checks both complete11520-frame channels, including every idle/query/stopped sample.
This is not continuous wall-clock live editing or a MessagePort deadline claim.

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --export-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --export-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_export::tests::immutable_exports_hold_both_revisions_across_native_direct_and_bounded_threads -- --exact
```

Repeat with `--release` before `--bin`/the filter. The native reports retain44
requests/84 export results/twelve primitive results,256 samples,3930 MIDI bytes,
4840 semantic bytes,5632 frames/44 callbacks and actual ownership/recovery counts.
The storage tuple `[16384,64,96,1024,2,1]` reports prepared resource backing, read/
request/response bytes, retained banks and command credits. It is not total native
RSS; native callback heap counts are guarded zero. Resource append revisions can
vary with callback partitions; public content revisions/audio/MIDI/timeline do not.
See [wire/lifecycle details](WIRE.md#immutable-export-transport).

## Upload resources and stage a nested program

The default runner executes the same complete script through native direct/capacity-one
actors, Node/Chromium bindings and actual separate-memory offline/live worklets:

1. Record0/Play4/Stop12 supplies the existing independent stereo/MIDI reference.
   Create a separate resource, upload64 bytes of stereo10–17/right×10, finish,
   commit revision1 and retain a snapshot/cursor. Upload100–107/right×10 as
   revision2 while that snapshot remains alive. Read every old byte again, release
   the old snapshot, then read every new byte. Validate/release an audio role source.
2. Upload a separate64-byte MIDI resource with note-on and a37-byte SysEx, then
   check every packet/timestamp/byte. Raw byte uploads have no playback checkpoint
   index: `RoleSnapshot` must return domain failure522, not a renderer fault or a
   playable source. Release/remove both resources; local generations become2.
3. Upload a precompiled `LTP1` header and15 ordered64-byte records, validate with
   observable progress, activate graph/topology epochs1/0→1/1, and recycle the
   displaced timing owner. All topology storage exists before rendering.
4. Play an explicit outer program containing a regular inner program and the
   recorded primitive. These composites have **no sync source and no fallback
   clock**. Admit Play at6784, SeekIteration1 at6788 and Stop at6792 before their
   deadlines. Real engine calls of1/4/4 frames produce eight playback samples and
   the silent Stop sample; they do not simulate autonomous composite clocking.
5. Require56 commands/82 staging results/six primitive results,137 mapped samples,
   3990 MIDI bytes and8200 semantic bytes (seven boundaries/21 decisions/23 events),
   6793 engine frames/56 callbacks, zero command/retained counts and256/16/16/0 recovery.

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --staging-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --staging-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_staging::tests::prepared_resource_uploads_immutable_reads_and_nested_programs_match_bounded_actors -- --exact
```

Repeat with `--release` before `--bin`/the filter. `BrowserStagingClient` compiles
only this fixture's template off-render; it is not an editor. Prepare it alongside
`BrowserBridge.new_staging(session)` before measuring memory. One128-byte input,
two256-byte transfer results, two local resource banks, two program banks, one
command credit and one64-byte owning chunk buffer are fixed. See [LPS1/LPR1](WIRE.md#resource-and-program-staging-lps1--lpr1).

Each accepted request is followed by an explicitly acknowledged engine step.
The first53 calls have128 frames; the final three have1/4/4. The engine pauses
between calls while Web Audio emits silence. A separate probe checks all512
samples/channel of the four audible quanta, including every short-call silent
tail. Offline additionally checks all14592 frames/channel. These are deliberate
host-driver partitions/pauses, not a runtime interaction quantum or continuous
live-edit/MessagePort-deadline evidence. Every envelope, immutable read, musical
sample, MIDI packet and semantic byte agrees with native reference execution;
only actual session nonces normalize. Native tables independently check all
samples/MIDI and every semantic record, including inherited seeks/stops.

Four additional native cases cover this complete script, owning Write/Read/Begin/
Append cancellation and128 real program reuses in both actor modes, all record
truncations/padding/full-width values, and transactional client/nonwrapping identities.
Successful stop refuses15 while a resource/program/source remains retained.
Fault abandonment is off-render and does not fabricate returned credits.

## Dense wraps, cascades and clocked composites

Prepare `BrowserBridge.new_dense(session)` (native: `Bridge::with_dense`) before
rendering. This opt-in scenario uses the unchanged primitive LLC1/LLR1 protocol,
four normal credits and one independent recovery credit:

1. Admit Record0, Play1, Seek64 and Seek130 before rendering. A fifth normal request
   refuses Busy10 without consuming a sequence. Record one stereo/MIDI frame;
   playback starts at1 and the same-position seek at64 is a real discontinuity.
2. A's Record-start pulse starts timing-only B→C in the same frame. B's one-frame
   clock also drives a regular→explicit composite pair. The regular cursor makes
   32 complete passes; the explicit cursor ends at4, while C continues playing.
   This checks the simultaneous trigger/program ordering, not an audible composite
   stop or a nominal fallback clock.
3. Render128 frames and then one more without polling any results. All four normal
   bridge credits remain occupied; successful stop refuses15. Submit recovery Stop
   at129 and render two frames. Stop suppresses A's due wrap and emits MIDI cleanup;
   the already-admitted Seek130 now fails with domain InvalidState261, not a renderer
   fault. B/C and their clocks still progress. All five command credits remain held
   until polling returns all ten admission/execution records.
4. Compare all131 stereo samples,8449 MIDI packets/126735 bytes and1968 semantic
   records/314920 bytes against independent scalar/closed-form tables. The journal
   contains131 boundaries,655 complete decisions and1182 ordered events. Native
   whole/single/irregular/zero-call partitions and both actor modes agree on every
   report byte. Cleanup recovers256 pages/16 versions/16 snapshots/zero references.

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --dense-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --dense-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_dense::tests::dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled -- --exact
```

Add `--release` before `--bin` or the test filter for release. The ordinary browser
runner runs this scenario in both actual contexts and shared Node/Chromium bindings.
The actual driver acknowledges128/1/2-frame calls with engine pauses between them;
this is not a runtime interaction quantum or continuous live scheduling guarantee.
A separate mono **probe marker output** marks each rendered quantum, including the
silent recovery quantum. It does not modify the stereo signal. The downstream probe
checks all384 stereo samples/channel, matching all three clocks and zero short-call
tails; offline additionally checks all1024 output samples/channel. Both ordinary
Wasm memories stay fixed and all five input/ten response buffers return before stop.

The dense configuration prepares16384 MIDI events/65536 payload bytes per runtime
port and host output,262144 MIDI journal bytes and655400 semantic bytes. Those are
finite component capacities, not total native RSS. Larger diagnostic storage does
not cap or throttle musical progression. The standard scenarios keep their smaller
capacities. See [the clock/report protocol](WIRE.md#dense-musical-host-clock-and-report).
This case stalls Rust result consumers; it does not yet prove in-flight transferred
response-buffer exhaustion or capture exhaustion/replacement in an actual worklet.

## Transferred response buffers can stall independently

The browser runner also executes a variant of the dense scenario in both actual
contexts. After the original131 frames, it accepts all ten Rust receipts but
**retains all ten transferred96-byte ArrayBuffers on the main side** instead of
returning their transport credits. The worklet reports zero available response
buffers; command credits have already returned to the Rust client.

With that transport still exhausted, admit Play131 and Stop135 and acknowledge a
five-frame call. The first four samples play the retained take; Stop is silent and
emits cleanup. Polling cannot transfer either command's replies: at engine136,
worklet/client outstanding counts are both2, main received count is still10, all
ten real buffers are held, and successful stop refuses15. Return each buffer with
its original ID and check detachment. Polling now delivers all four additional
receipts, and successful stop requires all credits and resource pools recovered.

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --stall-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --stall-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled -- --exact
```

Add `--release` before `--bin` or the test filter. Native direct/capacity-one and
Node/Chromium hosts reproduce the engine script with unpolled replies; they do not
pretend to transfer MessagePort buffers. All seven requests/fourteen replies,
136 stereo samples,8713 MIDI packets/130695 bytes and2042 semantic records/326760
bytes match independent native tables. Actual downstream probes check512 samples
per channel, including all four marked quanta and zero short-call tails. Offline
also checks all1280 output samples per channel. Memory capacities stay those of the
dense fixture; all buffers/credits and256/16/16/0 resources recover. The extra live
commands arrive after the context has started, but engine pauses and explicit
acknowledgements—not assumed message deadlines—determine their timing.

`stallReports` and `bindingChecks.stalls` remain distinct evidence. See the
[wire/report contract](WIRE.md#transferred-response-buffer-exhaustion).
This proves cooperative response-buffer exhaustion, not a bound on the browser's
underlying MessagePort queue, hostile input pressure or production live deadlines.

## Replace playable media and export an exhausted capture

The media profile prepares two indexed stereo/MIDI revisions off-render. It uses
the real facade, a two-entry receiver-local media catalog, two loops and42 resource
pages; uploaded raw MIDI is still not an indexed playable source.

1. Admit Play0 and Seek6(position1), then explicitly start the one-frame clock loop
   at0. Select catalog entry1 at4: revision0's four frames become revision7's three
   frames, with exact cleanup/indexed MIDI restoration. No Rust media handle crosses
   the wire. The target declares that clock as its sync source.
2. Deliver Record7 with `AfterTicks { skip: 2 }`. Its reliable receipt distinguishes
   requested/effective/delivered7 from actual9. Recording does not monitor previous
   playback. Stereo live/FX output remains1/10 after recording starts.
3. At49, preflight cannot reserve the next interval. Four imported pages, five
   audio-prefix pages, one MIDI-prefix page and the32-page worst-case MIDI reserve
   exhaust the42-page preparation. Capture stops **before** sample49, retaining a
   common40-frame incomplete prefix. Unrelated live audio and the clock continue.
4. Query settled state at64: stopped/incomplete, capture revision8/40 frames and
   Capacity. Playback content remains revision7/length3; failure does not invent a
   successful playback replacement. Export latest at65 and check source cuts49,
   both capture stamps9–49, all320 audio bytes, both MIDI packets and all16 initial
   channel projections, including held notes62/70. Explicit Release returns generation2.

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --media-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --media-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml browser::native_media::tests::replacement_countdown_prefix_exports_and_bounded_actors_match_complete_oracles -- --exact
```

Add `--release` before `--bin` or before the test filter. Native direct/capacity-one
actors compare the complete report across whole/single/irregular/zero capture-call
partitions. Only callback counts and positive raw resource append revisions may vary;
logical content revisions, timeline state, media and receipts never normalize.

Node/Chromium and actual offline/live hosts run the same acknowledged script:
1/6/57-frame calls then27 one-frame owning query/export/read/release calls,91 frames
in30 callbacks. Independent tables check every sample,274 MIDI packets/4110 bytes,
552 semantic records/88360 bytes, two primitive/four staging/26 export requests and
all4/8/46 results. The downstream marker probe checks all3840 samples/channel and
all short-call tails; offline additionally checks all7936 output samples/channel.
Ordinary worklet memory remains3,342,336 debug/3,276,800 release bytes, with zero
Rust heap activity and full42page/16version/16snapshot/0reference recovery. All five
primitive/one staging/one export input buffers and ten/two/two result buffers return.

These are deliberately paused, acknowledged driver steps, not mandatory engine
quanta or continuous live/deadline proof. [Wire details](WIRE.md#prepared-playable-media-and-capture-failure)
keep catalog selection, delayed terminal frames and capture-status metadata explicit.

## Planner lifecycle controls and deliberate failures

A separate `BrowserPlannerCanary` uses a test-only generic planner around the real
loop library's Resolver. It is not an injection seam in the production LoopPlanner
or a replacement for the full facade scenarios above. Startup prepares a one-frame
loop with Play0, a decision-bound stereo half-gain consumer and all journal/storage.
There are no external command buffers or owning credits in this small profile.

```sh
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --planner-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml planner_canary::tests::resolver_planner_control_is_partition_invariant_with_complete_boundary_tables -- --exact
```

Add `--release` before `--bin` or the test filter. Native whole/single/irregular/zero
partitions compare every stereo sample and all512 semantic records/81960 bytes with
independent tables:128 settled boundaries,128 decisions and256 events at0–127.
Each interval is one frame because of the admitted loop length, not a work cap.
Stop does not invent the wrap at128. Planner and consumer destructors assert that
they run outside guarded rendering; counters saturate without retries.

Node and Chromium check the complete native control report plus both fault modes.
Actual offline/live controls render128 frames in one driver call, verify all256
probe samples/channel (128 active then128 silent), all512 offline samples/channel,
all semantic bytes,128 plan/finish calls and explicit8page/4version/4snapshot recovery.
Independent ordinary worklet memory stays1,376,256 debug/1,310,720 release bytes;
Rust heap counts remain zero. Main/worklet markers come from their actual separate
instance-local static, not a constant identity claim.

| Intentional fault | Render code | Rust planner witness | Plan/finish calls | Heap alloc/realloc/dealloc |
| --- | ---: | ---: | --- | --- |
| `planner` | 10 | 3 | 1/0 | 0/0/0 |
| `planner-rust-heap` | 4294901761 | 4 | 128/128 | 1/5/1 |

The error is injected inside `Planner::plan` after a real resolver preview and
before commit/consumer execution. The heap violation deliberately grows/drops a Vec
there, then finishes the control workload; the Rust allocator guard rejects it.
Both require specific diagnostics, nonzero runner exits, integer Rust-origin/phase
witnesses, processor witness0 and unchanged actual worklet memory. The runtime's
callback cursor is128 in both cases: it advances before planner work. In the error
case, zero finishes and zero output prove that this is **not**128 rendered or
musically committed frames. A fault is permanent; explicit abandonment is not
counted as the separate successful control's stop/disposal proof.

## Selected interpreter checks

Inspect the standalone selection without running Miri:

```sh
python3 scripts/run_rt_runtime_miri.py --list --package loop_engine_host_fixture
```

The pinned nightly-2026-05-04 selection names all59 library tests exactly, plus
primitive-wire and graph-actor tests at seeds1/2 (63 executions). It uses this
workspace's manifest/lockfile, full native workloads and the existing allocator/
destructor guards. This interprets native Rust, not JavaScript or AudioWorklets.

At the applicable consolidated stage gate, commit the intended inputs and launch
one supervised immutable snapshot from the repository root:

```sh
python3 scripts/run_rt_runtime_miri_jobs.py start --stage stage-7 --commit HEAD
```

The current default includes retained runtime/loop regressions and the Stage8
sparse-projection/ordered-insert models:145 commands/298 executions across all
three packages. The original Stage7 snapshot's144/291 remains historical. Do not launch it after each helper edit. Continue other
work and collect its terminal result using the returned job directory; launch,
native dispatch checks and partial output are not passes. The900-second per-command
watchdog is unchanged. See the [operational guide](../../docs/loop_layer/MIRI_BACKGROUND.md)
for source isolation, cancellation, collection and failure handling, and the
[job history](../../docs/loop_layer/MIRI_JOBS.md) for actual status. A concrete
failure may justify a declared one-command shard; adding `--measureme` profiles
that same full command without reducing its workload or raising the watchdog.
Diagnostic-profile artifacts are not a complete stage run or native performance
results; the CI complete-set collector rejects them.

## CI evidence

The build workflow runs both libraries, this fixture's native tests/replays and
public coordination examples in Linux/Windows/macOS debug/release cells. Both web
profiles run the complete Node/Chromium bindings, actual offline/live profiles and
six specific Rust-origin failure gates. Loop source/fixture changes trigger the
repository Chromium policy suite, and loop Wasm dependencies are checked explicitly.
Native and worklet reports have separate required artifact uploads.

Miri CI uses16 disjoint batches with a source/selection-matched complete-set verifier;
missing batches/logs/tests and wrong counts/seeds cannot pass. This scheduling does
not raise the900s command watchdog or certify expensive commands. See the
[operational reference](../../docs/loop_layer/MIRI_BACKGROUND.md#ci-batches).
Workflow configuration/local tests are not remote platform results; final-head green
required CI and review remain separate delivery obligations.

## Current scope

Native direct/wire/two-thread replay and real non-isolated offline/live worklets
now exercise this primitive scenario. This is **not Stage 7 completion**.
Prepared graph payload materialization has native facade and public-Wasm binding
coverage. Opt-in native graph commands now stage/activate/release and recycle
owners through bounded wire results. Public graph command bindings execute in
Node/Chromium with complete native-oracle comparisons. Actual offline/live graph
changes now transfer descriptors/commands/owning results through MessagePort and
check downstream samples, acknowledged clocks and successful recovery. Native
bounded two-thread graph payload/command/return transport also matches direct
replay. Immutable snapshot/read/release now additionally verifies both retained
capture revisions through native and actual worklet hosts. Resource upload/immutable
reads and prepared nested-program activation/iteration-seek/stop now also run through
all hosts. Dense one-sample playback, same-frame cascades, clocked regular/explicit
composites and unread-result recovery now also run through all hosts. Prepared playable
replacement, tick-delayed recording and common-prefix capture exhaustion/export now
also pass all hosts. Actual held response
buffers now also stall independently while newly acknowledged playback continues;
raw MIDI uploads are not checkpointed playable sources.
Separate real-resolver planner controls and exact error/heap canaries now also pass
native, bindings and actual hosts; they do not add a production fault-injection API.
No loop benchmark, production adapter or hard-deadline guarantee is supplied.
