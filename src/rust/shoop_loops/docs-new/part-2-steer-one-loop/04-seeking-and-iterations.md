# 4. Seeking and iterations

<!-- rust-source: ../examples/seek_loop.rs -->

[Contents](../README.md) · [Previous: Recording, playing, and stopping](../part-1-record-and-play-audio/03-recording-playing-and-stopping.md) · [Next: Recording over existing content](05-recording-over-existing-content.md)

Part I recorded three frames and played them from position 0. This chapter records six frames, plays, then **seeks** to position 4 mid-playback:

| Absolute frame | Transition | Effect |
|---|---|---|
| 0 | Record | Start a fresh take. |
| 6 | Play from position 0 | Finish the six-frame recording and play it. |
| 9 | Seek to position 4 | Jump inside the playing loop without restarting it. |
| 12 | Stop | Stop playback. |

The complete [seek example](../examples/seek_loop.rs) checks this offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin seek_loop
```

## 1. Submit the transitions

The only new piece is the Seek request; the rest of the setup is identical to chapter 3:

```rust
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            6,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (
            9,
            Transition::Seek {
                position: 4,
                retrigger: false,
            },
        ),
        (12, Transition::Stop),
    ] {
        let command = client.command(
            Frame(at),
            Operation::Transition {
                target,
                timing: Timing::At,
                transition,
            },
        );
        let ticket = control
            .submit_prepared(Lane::Normal, command)
            .unwrap_or_else(|refusal| panic!("submission: {:?}", refusal.error));
        sequences.push((ticket.sequence(), Frame(at)));
    }
```

## 2. Watch the seek and the wrap

Input frames 0–5 are the recording; later values differ so we can tell playback from live input:

```rust
    let left_in = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let right_in = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let mut left_out = [0.0; 16];
    let mut right_out = [0.0; 16];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 16,
                inputs: &[&left_in, &right_in],
                outputs: &mut [&mut left_out, &mut right_out],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
```

The expected samples are:

```rust
    assert_eq!(
        left_out,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 5.0, 6.0, 1.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        right_out,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 50.0, 60.0, 10.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(runtime.next_frame(), Frame(16));
```

Playback starts at frame 6 with positions 0, 1, 2. At frame 9 the Seek lands: position 3 is skipped and position 4 plays instead. Positions 5, 0 (wrap), then Stop at frame 12 silences the output:

```text
Absolute frame:  6   7   8   9  10  11  12 ...
Loop position:   0   1   2   4   5   0  —
                              ^        ^
                              seek     wrap
```

## 3. What Seek can and cannot do

| Field / rule | Meaning |
|---|---|
| `position` | A loop position; must be smaller than the recorded length, or the request fails with `InvalidFrame`. |
| `retrigger` | When `true`, also resets the loop's iteration count (see below). Seek itself keeps playing. |
| Mode | Seek requires the loop to be playing; on a stopped or recording loop it fails with `InvalidState`. |

Play and Seek differ in one more way: `Play` on an already-playing loop does nothing unless `retrigger: true`, while `Seek` always moves the position. Both finalize an in-progress recording first, exactly like Play in chapter 3.

## 4. Iterations

A loop counts how many times it has wrapped: position 0 after a wrap is **iteration 1**, the next wrap is iteration 2, and so on. `PlayIteration { iteration, retrigger }` and `SeekIteration { iteration, retrigger }` address that count instead of a sample position: play iteration 2 means "start at position 0 when the loop next begins its second pass."

In this single-loop example iterations are less visible than positions, so we leave them to chapter 6, where a parent loop drives child loops and iteration counts become the natural way to talk about "the third pass of the parent." Until then: use `Play`/`Seek` with `position` for sample-accurate control, and reach for the iteration variants when you think in whole passes.

[Previous: Recording, playing, and stopping](../part-1-record-and-play-audio/03-recording-playing-and-stopping.md) · [Next: Recording over existing content](05-recording-over-existing-content.md)
