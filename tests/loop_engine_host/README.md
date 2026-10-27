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

Add `--release` before `--bin` for optimized wire/two-thread replay. Each mode
compares all four partitions with the independently checked direct result before
printing a summary. No device is needed. The threaded driver uses one-packet
channels between distinct control/render threads; blocking channel operations and
join occur outside guarded callbacks. This is offline replay, not live scheduling
or deadline evidence.

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

Each normal runner invocation requires four successful sessions: primitive and
graph-change scenarios in both offline and live contexts. `--verify-failures`
additionally checks four intentionally failing sessions.

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

Fourteen shared public-binding groups execute in Node and Chromium: constructor
refusal, full-width identities/frames and independent credits, rendering and
receipt validation, fixed-memory diagnostic exhaustion without changing audio,
successful recovery, actual processor/heap failure witnesses, semantic boundary
records, an eventless same-position seek, two payload checks and three graph command
checks: full-width/credits, exact activation/retirement and128 real owner returns.
These are binding-level Wasm checks, not a claim that the43 native Rust tests
were executed as Wasm test functions.

The observer prepares128 fixed160-byte records. It records event-bearing boundaries,
playback discontinuities and changes in resolver journal loss. It omits ordinary
callback cuts and never invents a transition or pulse for an eventless seek. If a
whole boundary's decisions/events do not fit, it drops that entire diagnostic
batch, increments saturating loss counters and leaves music and receipts alone.
Native tests vary observer capacity and resolver journal capacity across72 runs;
Wasm checks also exhaust the observer while verifying all audio and receipts.

`--verify-failures` launches four intentionally failing **real worklet** sessions:
processor and heap faults in both primitive and graph hosts. The FX processor either
returns an error or allocates/reallocates/deallocates. Individual gates can be run
with `--failure-canary processor`, `rust-heap`, `graph-processor` or `graph-rust-heap`.
Each child must exit nonzero with the specific diagnostic, matching Rust processor
witness and heap counts. An unrelated exception or matching text without witnesses
cannot pass. These failed sessions do not stand in for successful disposal.

For successful sessions, collect all six admission/execution records, return all
request/response buffers, and stop invoking the engine before explicit cleanup
and destruction. Both contexts check complete pool recovery. The live context is
then closed; the offline context renders its remaining silent tail.

Reports are under `target/loop-engine-host/reports/{debug,release}/`. `result.json`
uses `LLH2` and contains complete sample arrays, wire/MIDI/semantic records, tools,
memory, ownership and failure-gate results. See [the report formats](WIRE.md#browser-report-llh2).
The runner rejects malformed or incomplete semantic data, then compares every byte
with the native independent table. This journal is extracted after callbacks stop;
it is not live telemetry or an immutable media export.

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
replay. Program/resource payload staging, immutable exports,
broader loop/composite/seek/replacement/exhaustion scenarios and live in-flight
transfer stalls also remain to be added to the host.
No loop benchmark, production adapter or hard-deadline guarantee is supplied.
