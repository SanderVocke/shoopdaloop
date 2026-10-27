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
callbacks; this report is not a wire format or callback journal.

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

### 1. Prepare two ordinary Wasm instances

The main instance owns `BrowserClient`; the AudioWorklet instance owns
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

### 3. Check memory, failures and shutdown

Rust guards cover preparation-free ingress, source filling, planner/render work,
MIDI observation and return recycling. Memory length must stay fixed after
preparation. JavaScript allocation/GC, structured cloning and browser scheduling
are outside these Rust heap guarantees; there is no deadline certification.

Seven shared public-binding checks execute in Node and Chromium: constructor
refusal, full-width identities/frames and independent credits, rendering and
receipt validation, fixed-memory diagnostic exhaustion without changing audio,
successful recovery, and actual processor/heap failure witnesses. These are
binding-level Wasm checks, not a claim that the 19 native Rust tests were executed
as Wasm test functions.

`--verify-failures` launches two intentionally failing **real worklet** sessions.
The FX processor either returns an error or allocates/reallocates/deallocates.
Each child must exit nonzero with the specific diagnostic, matching Rust processor
witness and heap counts. An unrelated exception or matching text without witnesses
cannot pass. These failed sessions do not stand in for successful disposal.

For successful sessions, collect all six admission/execution records, return all
request/response buffers, and stop invoking the engine before explicit cleanup
and destruction. Both contexts check complete pool recovery. The live context is
then closed; the offline context renders its remaining silent tail.

Reports are under `target/loop-engine-host/reports/{debug,release}/`. `result.json`
contains complete sample arrays, wire/MIDI records, tools, memory, ownership and
failure-gate results. See [the report formats](WIRE.md#browser-report-llh1).

## Current scope

Native direct/wire/two-thread replay and real non-isolated offline/live worklets
now exercise this primitive scenario. This is **not Stage 7 completion**.
Graph/program/resource payload staging and owning returns, semantic boundary
journals, immutable exports, broader loop/composite/seek/replacement/exhaustion
scenarios and live in-flight transfer stalls remain to be added to the host.
No loop benchmark, production adapter or hard-deadline guarantee is supplied.
