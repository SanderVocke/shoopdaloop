# 10. Composite loops

<!-- rust-source: ../examples/program_loop.rs -->

[Contents](../README.md) · [Previous: MIDI state and restoration](../part-4-midi/08-midi-state-and-restoration.md) · [Next: Programs](10-programs.md)

A **composite loop** is a loop with no content of its own that runs a **program** (next chapter) controlling other loops. The typical arrangement, which this chapter builds, is: one **sync loop** of one bar length, and every other loop — the composite arranger and the loops it controls — synced to that same bar loop, with lengths in whole bars:

```text
bar (primitive, 4 frames = 1 bar) ──ticks──▶ arranger (composite, synced to bar)
                       │                     │ program
                       │                     ├──▶ bass (primitive, 4 frames = 1 bar)
                       │                     └──▶ drums (primitive, 8 frames = 2 bars)
                       └──ticks────────────▶ bass
                       └──ticks────────────▶ drums
```

The complete [program example](../examples/program_loop.rs) runs this offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin program_loop
```

## 1. Describe the loops and the sync edges

The topology names each loop's kind, length, and sync source. Every loop except the bar loop shares the bar as its sync source:

```rust
    let bar = LoopId::from_parts(0, 1).unwrap();
    let arranger = LoopId::from_parts(1, 1).unwrap();
    let bass = LoopId::from_parts(2, 1).unwrap();
    let drums = LoopId::from_parts(3, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 4,
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
                    id: arranger,
                    kind: LoopKind::Composite,
                    length: 0,
                    sync_source: Some(bar),
                },
                LoopSpec {
                    id: bass,
                    kind: LoopKind::Primitive,
                    length: 4,
                    sync_source: Some(bar),
                },
                LoopSpec {
                    id: drums,
                    kind: LoopKind::Primitive,
                    length: 8,
                    sync_source: Some(bar),
                },
            ],
            programs: vec![ /* ... chapter 11 ... */ ],
            ..TopologyDescription::default()
        },
    );
```

| Field | Meaning |
|---|---|
| `LoopKind::Primitive` | A loop with recorded content (audio/MIDI). |
| `LoopKind::Composite` | A loop with no content of its own; it runs a **program** (chapter 11) and gets a compiled length. |
| `sync_source` | The loop whose wraps tick this loop. Sharing one sync source is what keeps all loops aligned to the same bar boundaries. |

Only primitive loops get a **binding** (signal nodes). The composite arranger is timing-only: it has no capture or playback node and no host ports, so `max_nodes` counts only the three primitives' six nodes.

## 2. Give the children content without recording

Instead of recording, the example **imports** audio into the resource store before rendering and hands it to the planner (`load_stereo` uploads sample bytes and returns a `media::Source`; the full function is in the example):

```rust
    let imports = vec![
        media::Input {
            description: media::Description {
                owner: bass,
                revision: Revision(0),
                frames: 4,
                channels: 2,
                midi: false,
            },
            audio: Some(bass_audio),
            midi: None,
        },
        media::Input {
            description: media::Description {
                owner: drums,
                revision: Revision(0),
                frames: 8,
                channels: 2,
                midi: false,
            },
            audio: Some(drums_audio),
            midi: None,
        },
    ];
```

An import with `revision: Revision(0)` becomes the loop's initial playback content. The primitive loops' `length` in the topology must match the imported frame count — that is the check the library performs at prepare time.

## 3. Start the bar loop; the arranger follows its ticks

Record the bar loop for one bar and play it; it wraps every 4 frames, ticking all three followers at each bar boundary:

```rust
    submit(0, bar, Transition::Record { monitor_previous: false }, /* ... */);
    submit(4, bar, Transition::Play { position: 0, retrigger: false }, /* ... */);
    submit(0, arranger, Transition::Play { position: 0, retrigger: true }, /* ... */);
    submit(12, bar, Transition::Stop, /* ... */);
    submit(12, arranger, Transition::Stop, /* ... */);
```

The bar loop's output is its one recorded bar repeating. The children's output shows the arrangement (full asserts in the example):

```text
frame:      0 1 2 3   4 5 6 7   8 9 10 11
bar out:    0 0 0 0   1 1 1 1   1 1 1  1     (wraps at 8 — tick 1)
bass out:   1 2 3 4   0 0 0 0   0 0 0  0     (1-bar bass plays during bar 1)
drums out:  0 0 0 0   5 6 7 8   9 10 11 12  (2-bar drums play during bars 2-3)
```

The arranger's program starts the bass at the first bar boundary and the drums one bar later, for two bars — everything lands on bar boundaries because the arranger itself is synced to the bar loop. Stopping the bar loop at frame 12 stops the ticks; stopping the arranger ends the program.

## 4. What sync means (and does not mean)

A sync source's ticks are delivered to its followers at the source's wrap frame, in the same render pass — there is no one-block delay. Ticks do not themselves start or stop anything; they make a follower *eligible* for its scheduled requests (chapter 6) and drive its program iterations. A follower with no pending work simply keeps its current mode.

[Previous: MIDI state and restoration](../part-4-midi/08-midi-state-and-restoration.md) · [Next: Programs](10-programs.md)
