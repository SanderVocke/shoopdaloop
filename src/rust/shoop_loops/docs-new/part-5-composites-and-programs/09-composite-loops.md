# 10. Composite loops

<!-- rust-source: ../examples/program_loop.rs -->

[Contents](../README.md) · [Previous: MIDI state and restoration](../part-4-midi/08-midi-state-and-restoration.md) · [Next: Programs](10-programs.md)

A **sync source** is a loop whose wraps act as a metronome. Any loop with `sync_source: Some(other)` receives a **tick** (a pulse) each time the source wraps. This chapter builds that: a one-frame **clock** loop plays continuously and wraps every frame, so it ticks every frame. An **arranger**, a composite loop synced to the clock, runs a program that starts and stops child loops on those ticks.

```text
clock (primitive, 1 frame)  ──ticks──▶  arranger (composite, synced to clock)
                                            │ program
                                            ├──▶ bass (primitive, 3 frames)
                                            └──▶ drums (primitive, 5 frames)
```

The complete [program example](../examples/program_loop.rs) runs this offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin program_loop
```

## 1. Describe the loops and the sync edges

The topology names each loop's kind and sync source:

```rust
    let clock = LoopId::from_parts(0, 1).unwrap();
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
                    id: clock,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                },
                LoopSpec {
                    id: arranger,
                    kind: LoopKind::Composite,
                    length: 0,
                    sync_source: Some(clock),
                },
                LoopSpec {
                    id: bass,
                    kind: LoopKind::Primitive,
                    length: 3,
                    sync_source: Some(arranger),
                },
                LoopSpec {
                    id: drums,
                    kind: LoopKind::Primitive,
                    length: 5,
                    sync_source: Some(arranger),
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
| `LoopKind::Composite` | A loop with no content of its own; it runs a **program** (chapter 11) and has a compiled length. |
| `sync_source` | Another loop whose wraps tick this loop. The source of a composite must be a composite too is not required — here the arranger ticks from the clock, and the children tick from the arranger. |

Only primitive loops get a **binding** (signal nodes). The composite arranger is timing-only: it has no capture or playback node and no host ports, so `max_nodes` counts only the three primitives' six nodes.

## 2. Give the children content without recording

Instead of recording, the example **imports** audio into the resource store before rendering and hands it to the planner (`load_stereo` uploads sample bytes and returns a `media::Source`; the full function is in the example):

```rust
    let imports = vec![
        media::Input {
            description: media::Description {
                owner: bass,
                revision: Revision(0),
                frames: 3,
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
                frames: 5,
                channels: 2,
                midi: false,
            },
            audio: Some(drums_audio),
            midi: None,
        },
    ];
```

An import with `revision: Revision(0)` becomes the loop's initial playback content. The primitive loops' `length` in the topology must match the imported frame count — that is the check the library performs at prepare time.

## 3. Start the clock; the arranger follows its ticks

Recording a one-frame clock and playing it makes it tick every frame:

```rust
    submit(0, clock, Transition::Record { monitor_previous: false }, /* ... */);
    submit(1, clock, Transition::Play { position: 0, retrigger: false }, /* ... */);
    submit(0, arranger, Transition::Play { position: 0, retrigger: true }, /* ... */);
    submit(12, clock, Transition::Stop, /* ... */);
    submit(12, arranger, Transition::Stop, /* ... */);
```

The clock's output is just its one recorded frame repeated. The children's output shows the arrangement (full asserts in the example):

```text
frame:      0    1    2    3    4    5    6    7    8    9   10   11
clock out:  0    1    1    1    1    1    1    1    1    1    1    1
bass out:   1    0    0    1    0    0    1    0    0    1    0    0
             └─ tick 0: play ──┘        └─ tick 3 ──┘
drums out:  0    5    6    0    5    6    0    5    6    0    5    6
                  └─ tick 1: play ──┘          └─ tick 4 ──┘
```

The bass (3 frames) starts at tick 0 and plays one frame per tick for 3 ticks; the drums (5 frames) start at tick 1 and play two frames per tick. Both stop when the arranger's program ends. Stopping the clock at frame 12 stops the ticks, and stopping the arranger ends the program.

## 4. What sync means (and does not mean)

A sync source's ticks are delivered to its followers at the source's wrap frame, in the same render pass — there is no one-block delay. Ticks do not themselves start or stop anything; they make a follower *eligible* for its scheduled requests (next chapter) and drive its program iterations. A follower with no pending work simply keeps its current mode.

[Previous: MIDI state and restoration](../part-4-midi/08-midi-state-and-restoration.md) · [Next: Programs](10-programs.md)
