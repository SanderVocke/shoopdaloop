# Milestone 1 composition demo

This small standalone workspace answers one question before any production code
is written: can a DSP component, built as a separate crate, run inside the loop
engine's signal graph and be controlled with typed, sample-accurate requests,
without changing the loop engine or adding a second scheduler?

The answer it demonstrates is yes. The design shown here was later built out
properly in [`shoop_example_dsp`](../../src/rust/shoop_example_dsp/README.md)
and [`shoop_engine_service`](../../src/rust/shoop_engine_service/README.md).
Use those crates for real work. This workspace is a reference and a regression
check for the idea, not an alternative engine.

## What it does

The demo builds one stereo signal graph with three nodes:

```text
host input L/R ──► DSP (live L/R)
loop playback  ──► DSP (playback L/R)
DSP output     ──► loop capture
               └─► host output L/R
```

The loop records host frames 1–3 and starts playing them back at frame 4. The
DSP mixes the live input with the loop playback and has two prepared presets:

| Preset | Live gain | Playback gain | Stereo |
| --- | --- | --- | --- |
| 0 (initial) | 1 | 1 | as is |
| 1 | 2 | 0.5 | L and R swapped |

A client asks the DSP to switch to preset 1 at frame 5. That frame falls in the
middle of a host callback and in the middle of a loop playback cycle. The switch
happens on exactly that sample, the loop keeps playing and wrapping as normal,
and the client receives a result naming the frame at which the switch actually
happened and the new revision (1 → 2).

## How it is designed

There are two crates:

- `dsp/` (`shoop_m1_spike_dsp`) holds the component. Its only dependency is
  `rt_runtime`; it knows nothing about loops.
- The root crate holds the tests. These act as the host: they assemble the loop
  engine, the graph and the DSP through public APIs only.

`prepare()` creates everything up front and returns three handles:

- **`Client`**: the control side. `select(at, preset, expected_revision)`
  requests a preset switch at a given frame. `cancel_pending(target)` cancels a
  request the audio thread has already picked up. `try_completion()` collects
  results.
- **`RenderControl`**: the render-side half of the control channel. The host
  calls `begin_callback` before each engine callback and `finish_callback`
  afterwards, even if the engine returned an error.
- **`Dsp`**: the processor that is placed in the graph.

Requests travel over the runtime's existing transport, which has two lanes:

- The **normal** lane has room for one request. Until its result has been
  collected, the next `select` returns `Full`.
- The **recovery** lane has its own slot, so a cancellation can always get
  through even while the normal lane is full.

During `begin_callback`, `RenderControl` checks each request. If the requested
frame falls inside the current callback, it hands the request to the DSP through
a small shared mailbox. A request for a later frame stays in the transport until
its callback comes around. The DSP applies the switch at the exact sample inside
`process` and records what it did. `finish_callback` then turns that record into
the client's result. The client never touches the mailbox or the DSP's state.

All storage is allocated in `prepare()`. Nothing on the audio path allocates,
frees, locks or waits.

## Results and refusals

Each request ends with exactly one `Reply`:

| Reply | When |
| --- | --- |
| `Applied { frame, previous_revision, revision, preset }` | The switch happened. |
| `Refused(InvalidEvent)` | The requested frame had already passed. |
| `Refused(InvalidId)` | The preset slot or generation does not exist. |
| `Refused(InvalidState)` | `expected_revision` is stale: another switch happened first. |
| `Refused(Disconnected)` | The client went away before the request ran. |
| `Cancelled` | The request was cancelled before it took effect. |
| `CancelRegistered { target }` | Result for a `cancel_pending` request. |

A refused request leaves the audio output unchanged.

## Running it

From the repository root, inside the development environment (`nix develop`):

```sh
export CARGO_TARGET_DIR="$PWD/target"
cargo test --locked --manifest-path experiments/milestone_1_composition/Cargo.toml
cargo test --locked --release --manifest-path experiments/milestone_1_composition/Cargo.toml
```

This workspace is not part of the root Cargo workspace, so a plain
`cargo test --workspace` does not run it.

The tests check full sample values against independently computed expectations.
They cover:

- the switch at frame 5, with the same eight frames split into host callbacks
  in five different ways (including zero-length callbacks);
- a client that does not collect its result: further requests get `Full`, but
  audio and loop wraps carry on;
- cancellation, both before and after the audio thread has picked up the
  request;
- refusal of unknown presets, stale revisions and late requests;
- dropping the client while a request is still pending;
- no heap allocation or free during any callback, and all loop resources
  returned after shutdown.

## Limits

The demo covers audio only. It does not cover MIDI, latency compensation,
performance measurement, shutdown of a full service, or WebAssembly test
execution.
