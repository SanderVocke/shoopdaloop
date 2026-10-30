# shoop_loops

Draft opening of the replacement book. The existing documentation remains unchanged.

## Part I — Record and play audio

- [1. The loop library and the host](part-1-record-and-play-audio/01-the-loop-library-and-the-host.md)
- [2. Preparing an audio loop](part-1-record-and-play-audio/02-preparing-an-audio-loop.md)
- [3. Recording, playing, and stopping](part-1-record-and-play-audio/03-recording-playing-and-stopping.md)

## Part II — Steer one loop

- [4. Seeking and iterations](part-2-steer-one-loop/04-seeking-and-iterations.md)
- [5. Recording over existing content](part-2-steer-one-loop/05-recording-over-existing-content.md)

## Part III — Syncing loops

- [6. Syncing loops](part-3-syncing-loops/06-syncing-loops.md)
- [7. Sync-scheduled recording](part-3-syncing-loops/07-sync-scheduled-recording.md)

## Part IV — MIDI

- [8. A MIDI loop](part-4-midi/07-a-midi-loop.md)
- [9. MIDI state and restoration](part-4-midi/08-midi-state-and-restoration.md)

## Part V — Composites and programs

- [10. Composite loops](part-5-composites-and-programs/09-composite-loops.md)
- [11. Programs](part-5-composites-and-programs/10-programs.md)

## Part VI — Content and graph

- [12. Exporting and importing media](part-6-content-and-graph/11-exporting-and-importing-media.md)
- [13. Editing the graph around loops](part-6-content-and-graph/12-editing-the-graph-around-loops.md)

## Part VII — Operating

- [14. Capacities, refusals, and results](part-7-operating/13-capacities-refusals-and-results.md)
- [15. Shutdown](part-7-operating/14-shutdown.md)

## Run the examples

The [first loop example](examples/first_loop.rs) records three stereo frames, plays them immediately, wraps, and stops. It checks the samples, request outcomes, and resource recovery without an audio device.

Run it from the repository root:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin first_loop
```

Every chapter's example runs the same way: `--bin seek_loop`, `--bin overdub_loop`, `--bin sync_loops`, `--bin sync_record`, `--bin program_loop`, or `--bin midi_loop`.

Add `--release` for the optimized build. The examples have a separate Cargo manifest so this draft does not change the library's existing examples or workspace configuration.

## Terms used in this book

| Term | Meaning |
|---|---|
| Loop | One musical timeline that can record, play, and stop. |
| Frame | One sample position on the host's absolute clock. |
| Loop position | A position inside recorded content, relative to the loop's start. |
| Topology | The description of the loops and their musical relationships. |
| Planner | The part that owns loop timing and recorded content, and decides each loop's mode and position. |
| Media | A loop's recorded content: a handle in the planner's ledger (its fixed-size table of media slots) referencing sample pages in the runtime's shared storage, not a private buffer. |
| Capture / playback | A loop's two signal nodes: capture receives what is recorded, playback sends recorded content into the graph. |
| Binding | The link between a loop and its two signal nodes. |
| Control endpoint / runtime | The control side submits requests and collects results; the render side owns the running loops and processes buffers. |
| Admission | The runtime's report that a request was accepted for delivery. |
| Execution | The musical result of a request, reported at its actual boundary. |
| Seek | Move the play position inside the current content without restarting playback. |
| Iteration | How many times a loop has wrapped; iteration 1 begins at the first wrap. |
| Overdub / `monitor_previous` | Record a new take while the previous take keeps playing. |
| Sync source / tick | A loop whose wraps tick its followers; a tick is delivered at the wrap frame. |
| Composite loop | A loop with no content of its own that runs a program. |
| Program | A composite loop's arrangement: entries that start/stop child loops per iteration. |
| Iteration | One pass of a composite's program (or a loop's wrap count); addressed by `PlayIteration`/`SeekIteration`. |
| Export | An immutable handle to a take, for saving, selecting, or querying. |
| Cleanup (MIDI) | The note-off/pedal-release packets emitted when a MIDI loop stops. |
