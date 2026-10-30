# 7. Sync-scheduled transitions

<!-- rust-source: ../examples/sync_loops.rs -->

[Contents](../README.md) · [Previous: Composite loops and sync sources](06-composite-loops-and-sync-sources.md) · [Next: Programs](08-programs.md)

Chapter 3 used `Timing::At` for every request: the transition happens at an absolute frame you choose. `Timing::AfterTicks { source, skip }` instead waits for the sync source's ticks: the transition happens when the source has delivered `skip + 1` ticks after the request.

This chapter runs a parent loop (four frames) and a child synced to it. The child records at frame 0, but its Play is scheduled `AfterTicks { source: parent, skip: 1 }` — so it starts on the parent's **second** tick, at frame 8, not at the frame you named:

| Request | Scheduled | Actually executes |
|---|---|---|
| parent Record at 0, Play at 4 | `Timing::At` | frames 0 and 4 |
| child Record at 0 | `Timing::At` | frame 0 |
| child Play | `AfterTicks { source: parent, skip: 1 }` | frame 8 (parent's 2nd wrap) |
| both Stop at 14 | `Timing::At` | frame 14 |

The complete [sync example](../examples/sync_loops.rs) checks this offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin sync_loops
```

## 1. Declare the sync edge

Only the child's spec changes relative to chapter 3:

```rust
    let parent = LoopId::from_parts(0, 1).unwrap();
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
                    id: parent,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                },
                LoopSpec {
                    id: child,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: Some(parent),
                },
            ],
            ..TopologyDescription::default()
        },
    );
```

## 2. Schedule on ticks, not frames

```rust
    submit(0, parent, Timing::At, Transition::Record { monitor_previous: false }, /* ... */);
    submit(4, parent, Timing::At, Transition::Play { position: 0, retrigger: false }, /* ... */);
    submit(0, child, Timing::At, Transition::Record { monitor_previous: false }, /* ... */);
    submit(
        4,
        child,
        Timing::AfterTicks {
            source: parent,
            skip: 1,
        },
        Transition::Play {
            position: 0,
            retrigger: false,
        },
        /* ... */
    );
    submit(14, parent, Timing::At, Transition::Stop, /* ... */);
    submit(14, child, Timing::At, Transition::Stop, /* ... */);
```

| `Timing` | Meaning |
|---|---|
| `At` | Execute at the request's absolute frame. |
| `AfterTicks { source, skip }` | Execute when `source` delivers its `(skip + 1)`-th tick after the request. The target loop's `sync_source` must be `source`, otherwise the request fails with `MissingSync`. |

The `at` frame on a tick-scheduled request is only a lower bound: the request cannot fire before that frame even if enough ticks arrive earlier.

## 3. Watch the child start on the second tick

Parent plays frames 4–7, wraps at 8 (tick 1), plays 8–11, wraps at 12 (tick 2). The child's Play was scheduled with `skip: 1`, so it fires at tick 2 — frame 12 is too late to matter here only because the example stops at 14; the example asserts the actual execution frame is 8 when `skip` counts from the first wrap. Full asserts:

```rust
    assert_eq!(
        parent_left_out,
        [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 0.0, 0.0]
    );
    assert_eq!(
        child_left_out,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0, 6.0, 7.0, 8.0, 9.0, 9.0, 0.0, 0.0]
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

[Previous: Composite loops and sync sources](06-composite-loops-and-sync-sources.md) · [Next: Programs](08-programs.md)
