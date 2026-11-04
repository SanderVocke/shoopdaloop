# Buffered processor

`rt_buffered` provides a stereo audio/MIDI node with fixed input batching and
scheduled output. The development backend `InlineTestProcessor` runs on the render thread:
it doubles audio samples and moves each MIDI channel-voice message to the next
channel, wrapping channel 16 to channel 1. System messages pass through unchanged.
The graph still supplies and consumes exactly its prepared quantum Q.

```rust
use rt_buffered::{BufferedProcessor, MidiLimits, Timing};
use rt_runtime::Quantum;
# fn main() -> Result<(), rt_runtime::Error> {
let node = BufferedProcessor::prepare(
    Quantum::new(16)?,
    Timing { batch_quanta: 2, slack_quanta: 1 },
    MidiLimits::default(),
)?;
assert_eq!(node.timing().batch_frames(), 32);
assert_eq!(node.latency_frames(), 48);
# Ok(())
# }
```

## Timing

For batch size M quanta and extra slack S quanta, input collected at ticks b through
b+M-1 is computed once at b+M-1 and begins output at b+M+S. The exact stream delay
is `(M+S)*Q` frames. Even M=1, S=0 delays output by one quantum; inline completion
does not release the result early. M can be any supported positive integer,
including 3. S can exceed M.

For Q=16, M=2, S=1, the first three output quanta are silent. Tick 3 emits the
processed input from tick 0; tick 4 emits input from tick 1. A note-on
`[0x90, 60, 100]` at input frame 15 becomes `[0x91, 60, 100]` at output frame 63.
MIDI offsets within each quantum and equal-time packet ordering are preserved.
Note-off and velocity-zero note-on packets receive the same channel mapping.

This latency is measured between node ports. Host buffering adds its own declared
delay; the standard buffered host adds Q frames, giving 64 total frames in the
example. There is no graph latency compensation. Several graph ticks can execute
back-to-back inside one host callback, so stream delay does not guarantee elapsed
worker execution time. This backend neither simulates nor certifies remote timing.

## Storage and rendering

Preparation validates all sizes before allocating and touches the backing storage.
Supported limits are 65,536 frames per processing block, 262,144 frames of node
latency, and 32 MiB of calculated slot/scratch storage, including MIDI recovery
reserves and slot metadata. Requests exceeding any limit return `Error::Capacity`.

The adapter prepares `ceil(latency_frames/batch_frames)+1` slots: a collecting
block plus enough storage for completed blocks spanning the delay window. A slot
cannot be reused until its final output quantum is emitted. Each completion is
tagged with stream generation, block sequence, and source frame. The inline
backend completes synchronously through a private submission/completion seam.
There are no transport queues, threads, locks, waits, I/O, or render allocations.

`MidiLimits` bounds events and bytes per quantum independently. Each batch reserves
M times those limits. Rotation preserves count and payload sizes, so output needs
the same per-quantum capacity as input. Capacity is checked before a tick mutates
the adapter or publishes output. Capacity exhaustion returns `Error::Capacity`;
runtime-owned terminal audio fault and MIDI recovery handle it. Note-off messages
are never silently dropped. The runtime's `Error::Full` overflow policy is not
used for these adapter failures.

The node implements the runtime's unified `Processor::process` interface through
one core. `process_tick(frame, quantum, ...)` also accepts caller-owned audio/MIDI
views. Both entry points require exact stereo/Q shape, the prepared quantum,
contiguous timestamps, and an empty ordinary MIDI output buffer. Processor operation families are left to
the composition; this dummy exposes no live parameters.

## Lifecycle

Changing timing requires preparing a new node. `reset(origin)` clears all buffered
audio/MIDI, advances the stream generation, resets the completed-batch counter,
and starts at an aligned origin with the same initial silence. Use it only after
callbacks stop and downstream MIDI recovery is settled. Frame gaps and replayed
ticks are refused before mutation. Graph epoch changes alone do not clear audio
history. Live removal/reactivation and asynchronous readiness/deadline policies
are outside this initial implementation.

## Development check

```sh
cargo run --locked -p rt_buffered --example inline_gain
cargo run --locked -p rt_buffered --example inline_gain -- 32 3 7
cargo test --locked -p rt_buffered
cargo test --locked -p shoop_rt_engine --lib buffered_tests
```

The example checks stereo samples, channel-wrapped MIDI note pairs, SysEx, startup,
and the delayed tail. It accepts optional positional Q/M/S arguments. The engine
test composes the node with prepared production strip DSP and typed controls;
application graphs are not changed to insert this development processor.
