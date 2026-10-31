# 5. Recording over existing content

<!-- rust-source: ../examples/overdub_loop.rs -->

[Contents](../README.md) · [Previous: Seeking and iterations](04-seeking-and-iterations.md) · [Next: Syncing loops](../part-3-syncing-loops/06-syncing-loops.md)

So far every Record started from silence. This chapter records a four-frame take, plays it, then records **over** it while still hearing the old take — an overdub:

| Absolute frame | Transition | Effect |
|---|---|---|
| 0 | Record | Fresh take, no monitoring. |
| 4 | Play from position 0 | Finish the take and play it. |
| 8 | Record with `monitor_previous: true` | Start a new take while the old one keeps playing. |
| 12 | Play from position 0 | Finish the overdub and play the new take. |
| 18 | Stop | Stop playback. |

The complete [overdub example](../examples/overdub_loop.rs) checks this offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin overdub_loop
```

## 1. Monitor the previous take while recording

The only new idea is `monitor_previous` on Record:

```rust
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (
            8,
            Transition::Record {
                monitor_previous: true,
            },
        ),
        (
            12,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (18, Transition::Stop),
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

| `monitor_previous` | Output while recording |
|---|---|
| `false` | Silent (no live route, as in chapter 3). |
| `true` | The previous take keeps playing, so you hear what you are replacing. |

The graph does not change. The planner's decision is what routes the old media to the playback node during the new recording; the capture node writes the new take to fresh storage.

## 2. Two takes, one loop

First take: input frames 0–3 (`1,2,3,4`). Overdub: input frames 8–11 (`5,6,7,8`):

```rust
    let left_in = [1.0, 2.0, 3.0, 4.0, 9.0, 9.0, 9.0, 9.0, 5.0, 6.0, 7.0, 8.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let right_in = [10.0, 20.0, 30.0, 40.0, 90.0, 90.0, 90.0, 90.0, 50.0, 60.0, 70.0, 80.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let mut left_out = [0.0; 20];
    let mut right_out = [0.0; 20];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 20,
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

The expected samples show both takes:

```rust
    assert_eq!(
        left_out,
        [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 5.0, 6.0, 0.0, 0.0]
    );
    assert_eq!(
        right_out,
        [0.0, 0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 40.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 50.0, 60.0, 0.0, 0.0]
    );
    assert_eq!(runtime.next_frame(), Frame(20));
```

```text
Absolute frame:  4   5   6   7   8   9  10  11  12  13  14  15  16  17  18 ...
Mode:                playing       recording+monitor   playing          stopped
Output (left):       1   2   3   4   1   2   3   4   5   6   7   8   5   6   —
```

Frames 8–11 play the **old** take (1,2,3,4) because of `monitor_previous`, even though the new take (5,6,7,8) is being captured at those same frames. At frame 12, Play finalizes the recording: the new take replaces the old one, and playback restarts from it. The loop's length stays four frames — both takes are four frames here, but a new take may also have a different length; recording sets the length to however many frames were captured.

## 3. Replacing content without recording

`Transition::Replace { length, revision, preserve_position }` swaps the loop's content metadata without capturing anything — for example after loading a file into the resource store:

| Field | Meaning |
|---|---|
| `length` | New content length in frames; 0 stops the loop. |
| `revision` | Version tag of the new content. |
| `preserve_position` | Keep the current position if it still fits; otherwise the position resets to 0. Fails with `InvalidFrame` if the position would not fit. |

Replace is refused while recording (`InvalidState`). Use Record + `monitor_previous` when the new content comes from a live input, and Replace when it comes from storage.

[Previous: Seeking and iterations](04-seeking-and-iterations.md) · [Next: Syncing loops](../part-3-syncing-loops/06-syncing-loops.md)
