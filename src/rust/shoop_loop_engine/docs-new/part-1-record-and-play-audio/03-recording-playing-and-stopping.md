# 3. Recording, playing, and stopping

<!-- rust-source: ../examples/first_loop.rs -->

[Contents](../README.md) · [Previous: Preparing an audio loop](02-preparing-an-audio-loop.md)

The engine is prepared, but nothing runs until the host supplies buffers. Before the first call, we will queue three transitions:

| Absolute frame | Transition | Effect |
|---|---|---|
| 0 | Record | Start a fresh take. |
| 3 | Play from position 0 | Finish the three-frame recording and play its first sample immediately. |
| 7 | Stop | Stop playback before sample 7. |

The [first loop example](../examples/first_loop.rs) checks this sequence offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loop_engine/docs-new/examples/Cargo.toml --bin first_loop
```

## 1. Submit the transitions

The example retains each ticket's sequence alongside its expected execution frame:

```rust
    let mut sequences = Vec::new();
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            3,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (7, Transition::Stop),
    ] {
        let ticket = control
            .submit(
                Lane::Normal,
                Frame(at),
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition,
                },
            )
            .unwrap_or_else(|refusal| panic!("submission: {:?}", refusal.error));
        sequences.push((ticket.sequence(), Frame(at)));
    }
```

`Frame(at)` is an absolute engine frame. `Timing::At` asks for that boundary rather than waiting for sync pulses. Queueing all three before the first callback gives them time to be admitted before they are due.

Record's `monitor_previous: false` does not play an old take during recording. Play's position is relative to the recorded content; `retrigger: false` is sufficient for this recording-to-playback transition. It does not require a separate Stop first.

The request list and sequence bookkeeping are control-side work. A full submission returns the original owning command in the refusal; a running application should retain, retry deliberately, or dispose of it there. This finite example treats unexpected refusal as a failed assertion, not something to spin on inside rendering.

## 2. Process one host block

The first three input frames are the intended recording. Later values are deliberately different so we can tell recording apart from live-input monitoring:

```rust
    let left_in = [1.0, 2.0, 3.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let right_in = [10.0, 20.0, 30.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let mut left_out = [0.0; 8];
    let mut right_out = [0.0; 8];
    engine
        .process(
            AudioBlock {
                frame: engine.next_frame(),
                frames: 8,
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

Audio buffers are planar `f32`: one slice per channel. They are borrowed only for the call. The MIDI arrays are empty because we prepared zero MIDI ports.

Use `next_frame()` for each successive call. Host lengths may vary up to `max_frames`, but their absolute frame ranges must be contiguous. This single call covers `[0,8)`.

A live host makes the same `process` call from its audio callback while the controller runs separately. It must not wait there for commands or results, allocate buffers, or print diagnostics. The example's result checks and final message happen after processing returns.

## 3. Check immediate playback and the wrap

The expected samples are:

```rust
    assert_eq!(left_out, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 1.0, 0.0]);
    assert_eq!(right_out, [0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 10.0, 0.0]);
    assert_eq!(engine.next_frame(), Frame(8));
```

Frames 0–2 are silent because the graph has no live-input route to the output. At frame 3, the recording ends with exactly three stereo frames. Its first sample plays at that same frame, with no extra silent block.

Playback reaches positions 0, 1, and 2, then wraps at absolute frame 6. Stop at frame 7 makes the final output sample zero. The later input values never appear in playback.

The engine chooses these internal intervals:

```text
[0,3) recording   [3,6) playback   [6,7) wrapped playback   [7,8) stopped
```

These are not four host callbacks. The engine splits the one supplied block itself. A transition exactly at frame 8 would instead take effect at the next nonempty call; a zero-frame call does not settle it.

## 4. Check admission and musical outcomes

The controller has two result streams. An owning completion reports whether the runtime admitted the scheduled operation:

```rust
    for &(sequence, _) in &sequences {
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.sequence, sequence);
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
    }
```

`Admission` is the example's alias for `rt_runtime::transport::Outcome`. Scheduled means accepted for delivery, not that recording or playback succeeded.

The execution stream reports the terminal result at the musical boundary:

```rust
    for &(sequence, frame) in &sequences {
        let execution = control.try_execution().unwrap();
        assert_eq!(execution.sequence, sequence);
        assert_eq!(execution.actual_frame, frame);
        assert_eq!(execution.result, Ok(()));
        let Some(Reply::Transition(completion)) = execution.operation_result else {
            panic!("expected a transition result");
        };
        assert_eq!(completion.outcome, Outcome::Applied { frame });
    }
```

Check both the runtime result and the typed transition outcome. Runtime delivery can succeed while the requested transition fails. Match requests by sequence; musical execution need not follow submission order in a larger application.

Immediate `unwrap()` is appropriate here because the offline example has already processed every target frame. In a live application, either poll can return `None` while work is pending. Collect both streams regularly: unread results keep finite request capacity occupied. Destroy returned owners on the control side, not in the callback.

## 5. Clean up the stopped engine

Stop has already executed at frame 7, and the example has collected all three outcomes. No callback is running when it consumes the engine:

```rust
    drop(control);
    let mut stopped = engine.into_stopped();
    stopped.cleanup().unwrap();
    stopped.resources().begin_callback();
    stopped.resources().reclaim();
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, 16);
    assert_eq!(usage.free_versions, 8);
    assert_eq!(usage.free_snapshots, 8);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(stopped.media_usage().references, 0);
```

`into_stopped` preserves the state needed for explicit cleanup. `cleanup` releases the engine's media references, capture groups, and internally created resources. It is permanent teardown, not a pause from which this engine can resume.

Reclamation recycles retired storage under a work allowance. One final maintenance pass suffices for this small recording; larger content may need more. Here it runs on the stopped store. During rendering the engine owns callback accounting; do not reset it per operation or internal interval.

A device-backed host must stop and join its callbacks before this extraction. Destroying storage does not itself perform a musical Stop or send device cleanup. Our audio-only example has already stopped playback and needs no MIDI reset delivery.

We have now prepared a real stereo loop, recorded content, played it immediately, observed a wrap, stopped, and recovered its storage—all without writing a custom processor.

[Previous: Preparing an audio loop](02-preparing-an-audio-loop.md) · [Contents](../README.md)
