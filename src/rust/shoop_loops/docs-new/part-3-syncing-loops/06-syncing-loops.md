# 6. Syncing loops

<!-- rust-source: ../examples/sync_loops.rs -->

[Contents](../README.md) · [Previous: Recording over existing content](../part-2-steer-one-loop/05-recording-over-existing-content.md) · [Next: Sync-scheduled recording](07-sync-scheduled-recording.md)

So far every request used `Timing::At`: the transition happens at an absolute frame you choose. A **sync connection** changes that: a loop with `sync_source: Some(other)` receives a **tick** each time the source wraps, and `Timing::AfterTicks { source, skip }` waits for those ticks instead of a frame number — the transition happens when the source delivers its `(skip + 1)`-th tick after the request.

This chapter runs a **bar loop** (four frames) and a child synced to it. The child records at frame 0; its Play is scheduled `AfterTicks { source: bar, skip: 0 }` — so it starts on the bar loop's **first wrap**, without naming a frame at all:

| Request | Scheduled | Actually executes |
|---|---|---|
| bar Record at 0, Play at 4 | `Timing::At` | frames 0 and 4 |
| child Record at 0 | `Timing::At` | frame 0 |
| child Play | `AfterTicks { source: bar, skip: 0 }` | frame 4 (bar's first tick) |
| both Stop at 14 | `Timing::At` | frame 14 |

The complete [sync example](../examples/sync_loops.rs) checks this offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin sync_loops
```

## 1. Declare the sync edge

Only the child's spec changes relative to chapter 3:

```rust
    let bar = LoopId::from_parts(0, 1).unwrap();
    let child = LoopId::from_parts(1, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 2,
            pending_requests: 6,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![
                LoopSpec {
                    id: bar,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                },
                LoopSpec {
                    id: child,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: Some(bar),
                },
            ],
            ..TopologyDescription::default()
        },
    );
```

## 2. Schedule on ticks, not frames

```rust
    submit(0, bar, Timing::At, Transition::Record { monitor_previous: false }, /* ... */);
    submit(4, bar, Timing::At, Transition::Play { position: 0, retrigger: false }, /* ... */);
    submit(0, child, Timing::At, Transition::Record { monitor_previous: false }, /* ... */);
    submit(
        4,
        child,
        Timing::AfterTicks {
            source: bar,
            skip: 0,
        },
        Transition::Play {
            position: 0,
            retrigger: false,
        },
        /* ... */
    );
    submit(14, bar, Timing::At, Transition::Stop, /* ... */);
    submit(14, child, Timing::At, Transition::Stop, /* ... */);
```

| `Timing` | Meaning |
|---|---|
| `At` | Execute at the request's absolute frame. |
| `AfterTicks { source, skip }` | Execute when `source` delivers its `(skip + 1)`-th tick after the request. The target loop's `sync_source` must be `source`, otherwise the request fails with `MissingSync`. |

The `at` frame on a tick-scheduled request is only a lower bound: the request cannot fire before that frame even if enough ticks arrive earlier.

## 3. Watch the child start on the first wrap

The bar loop plays frames 4–7 and wraps at frame 8 — but its Play at frame 4 already delivered the first tick, so the child's `skip: 0` Play executes at frame 4. Full asserts:

```rust
    assert_eq!(
        bar_left_out,
        [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 0.0, 0.0]
    );
    assert_eq!(
        child_left_out,
        [0.0, 0.0, 0.0, 0.0, 5.0, 6.0, 7.0, 8.0, 5.0, 6.0, 7.0, 8.0, 5.0, 6.0, 0.0, 0.0]
    );
```

The execution stream reports the real boundary, which is the point of checking `actual_frame` rather than the requested frame:

```rust
    let child_play = applied
        .iter()
        .find(|(sequence, _, _)| *sequence == sequences[3].0)
        .copied();
    assert_eq!(
        child_play,
        Some((sequences[3].0, Frame(8), Outcome::Applied { frame: Frame(8) }))
    );
```

In a live setup this is how you launch a loop "on the next bar": you do not need to know the absolute frame, only the sync source and how many of its ticks to wait.

[Previous: Recording over existing content](../part-2-steer-one-loop/05-recording-over-existing-content.md) · [Next: Sync-scheduled recording](07-sync-scheduled-recording.md)
