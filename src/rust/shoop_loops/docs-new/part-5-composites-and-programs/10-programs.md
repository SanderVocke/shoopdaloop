# 11. Programs

<!-- rust-source: ../examples/program_loop.rs -->

[Contents](../README.md) · [Previous: Composite loops](09-composite-loops.md) · [Next: Exporting and importing media](../part-6-content-and-graph/11-exporting-and-importing-media.md)

A **program** is a small arrangement attached to a composite loop: a list of **entries** that say which child loop plays (or records, or stops) during which part of the parent's timeline. When the composite starts playing, the planner advances the program on each tick of its sync source and issues the entries' transitions to the children for you.

The [program example](../examples/program_loop.rs) from chapter 10 uses this program:

```rust
    programs: vec![ProgramDescription {
        source: arranger,
        nominal_unit_frames: 4,
        sections: vec![Section {
            entries: vec![
                Entry {
                    target: bass,
                    delay: 0,
                    duration: Some(1),
                    repetitions: 1,
                    mode: None,
                },
                Entry {
                    target: drums,
                    delay: 1,
                    duration: Some(2),
                    repetitions: 1,
                    mode: None,
                },
            ],
        }],
    }],
```

## 1. Read an entry

| Field | Meaning |
|---|---|
| `target` | The child loop the entry controls. |
| `delay` | Wait this many units after the previous entry ends before this one starts. |
| `duration` | How long the entry lasts, in units. If omitted, the child's own length (rounded up to whole units) is used. |
| `repetitions` | Repeat the entry this many times, back to back. |
| `mode` | `None` means play; `Some(Mode::Recording)` records; `Some(Mode::Stopped)` stops. Mixing `None` and `Some(..)` in one program is rejected. |

Units come from `nominal_unit_frames`: the number of frames one unit lasts. With the 4-frame bar loop and `nominal_unit_frames: 4`, one unit is exactly one bar (one sync tick). The program above therefore means: play the 1-bar bass for 1 bar, then after a 1-bar gap play the 2-bar drums for 2 bars.

## 2. Start and stop the composite

The composite itself is started like any loop; `retrigger: true` makes the start authoritative at the boundary:

```rust
    submit(0, arranger, Transition::Play { position: 0, retrigger: true }, /* ... */);
    submit(12, arranger, Transition::Stop, /* ... */);
```

`Play`/`Seek`/`PlayIteration`/`SeekIteration` on a composite select an **iteration** of the program (see chapter 4): iteration 0 is the first pass through the entries. The composite's compiled length is the total program duration in frames, which is why its `length` field in the topology is 0 — `compile` fills it in.

## 3. What the output shows

With the bar loop ticking every 4 frames, the example's outputs are:

```text
frame:      0 1 2 3   4 5 6 7   8 9 10 11
bass out:   1 2 3 4   0 0 0 0   0 0 0  0     iteration 0, bars 1: bass (1 bar)
drums out:  0 0 0 0   5 6 7 8   9 10 11 12  iteration 0, bars 2-3: drums (2 bars)
```

(Full per-channel asserts are in the example.) The children play their imported content, starting at position 0 on each entry, because a regular program restarts each segment.

## 4. Program rules worth knowing

- A composite must have exactly one program; a program's source must be composite (`compile` rejects otherwise).
- Programs can nest: an entry's target may itself be a composite with its own program, addressed by iteration (`PlayIteration`/`SeekIteration`).
- A stopped or idle sync source stalls the program: iterations only advance on delivered ticks.
- Program entries are compiled into segments at `compile` time; the render side never interprets the description.

[Previous: Composite loops](09-composite-loops.md) · [Next: Exporting and importing media](../part-6-content-and-graph/11-exporting-and-importing-media.md)
