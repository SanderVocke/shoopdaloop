# Independent loop-host fixture

This test-only workspace depends on `rt_runtime` and the actual `shoop_loop_engine`,
not the old backend, application, devices or the generic runtime host fixture.
Its own lockfile currently pins the same `rtrb` version as the repository.

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

## Current scope

This is native direct/wire/two-thread offline replay, **not Stage 7 completion**.
The library builds for `wasm32-unknown-unknown`, but there are no browser bindings
or actual Wasm/Web Audio executions yet. The fixed codec is transport-neutral;
native threads exchanging copied bytes do not prove independent Wasm memories.

Still to implement: graph/program/resource payload staging and owning return
formats, actual non-isolated offline/live AudioWorklet, fixed-memory/cross-instance
checks, semantic journals and immutable exports, broader loop/composite and
adversarial scenarios, browser failure canaries, and the unified host runner.
No loop benchmark, production adapter or hard-deadline guarantee is supplied.
