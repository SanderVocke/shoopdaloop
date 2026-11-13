# 7. Sync-scheduled recording

<!-- rust-source: ../examples/sync_record.rs -->

[Contents](../README.md) · [Previous: Syncing loops](06-syncing-loops.md) · [Next: A MIDI loop](../part-4-midi/07-a-midi-loop.md)

Chapter 6 scheduled a Play on a tick. The same works for Record — which is how you record a fixed number of bars: start recording at one sync tick, stop at a later one. This chapter's [example](../examples/sync_record.rs) records a **bar loop** of 4 frames, then records another loop for exactly 4 bars between the bar loop's ticks:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin sync_record
```

## 1. The requests

```rust
    submit(0, bar, Timing::At, Transition::Record { monitor_previous: false }, /* ... */);
    submit(4, bar, Timing::At, Transition::Play { position: 0, retrigger: false }, /* ... */);
    submit(
        0,
        take,
        Timing::AfterTicks {
            source: bar,
            skip: 0,
        },
        Transition::Record {
            monitor_previous: false,
        },
        /* ... */
    );
    submit(
        0,
        take,
        Timing::AfterTicks {
            source: bar,
            skip: 4,
        },
        Transition::Stop,
        /* ... */
    );
    submit(26, take, Timing::At, Transition::Play { position: 0, retrigger: false }, /* ... */);
    submit(30, bar, Timing::At, Transition::Stop, /* ... */);
    submit(31, take, Timing::At, Transition::Stop, /* ... */);
```

The bar loop records frames 0–3 and plays from frame 4, wrapping at 8, 12, 16, 20 — each wrap is one bar tick. The `take` loop's Record waits for tick 1 (`skip: 0`) and its Stop for tick 5 (`skip: 4`), so it records exactly 4 bars.

## 2. The execution frames are the proof

The interesting part is not the requested frames but where the tick-scheduled requests actually executed:

```rust
    let record_start = applied
        .iter()
        .find(|(sequence, _, _)| *sequence == sequences[2].0)
        .copied();
    let record_stop = applied
        .iter()
        .find(|(sequence, _, _)| *sequence == sequences[3].0)
        .copied();
    assert_eq!(
        record_start,
        Some((sequences[2].0, Frame(0), Outcome::Applied { frame: Frame(0) }))
    );
    assert_eq!(
        record_stop,
        Some((sequences[3].0, Frame(16), Outcome::Applied { frame: Frame(16) }))
    );
```

The recording ran from frame 0 to frame 16 — 16 frames = 4 bars of 4 — and the resulting take plays back at frame 26 (the example asserts its first captured sample appears there). If the bar loop's length ever changes, the same requests record a different number of frames automatically; that is the point of scheduling on ticks instead of absolute frames.

## 3. Rules

- `AfterTicks` requires the target loop's `sync_source` to be the named source, otherwise the request fails with `MissingSync`.
- The request's `at` frame is a lower bound; the tick decides the rest.
- Stop cancels pending tick-scheduled requests for the same loop that were submitted earlier (they report `Cancelled`).

[Previous: Syncing loops](06-syncing-loops.md) · [Next: A MIDI loop](../part-4-midi/07-a-midi-loop.md)
