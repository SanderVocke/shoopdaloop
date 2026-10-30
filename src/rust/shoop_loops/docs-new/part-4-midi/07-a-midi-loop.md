# 8. A MIDI loop

<!-- rust-source: ../examples/midi_loop.rs -->

[Contents](../README.md) · [Previous: Sync-scheduled recording](../part-3-syncing-loops/07-sync-scheduled-recording.md) · [Next: MIDI state and restoration](08-midi-state-and-restoration.md)

Audio loops carry samples; a **MIDI loop** carries events. The mechanics are the same — record, play, wrap, stop — but the signal is a stream of timestamped packets instead of sample buffers. This chapter records two events (a note-on at frame 0 and a sustain-pedal CC at frame 1), plays them back from frame 4, and stops at frame 7.

The complete [MIDI example](../examples/midi_loop.rs) checks this offline:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin midi_loop
```

## 1. Prepare a MIDI-only loop

A MIDI loop has no audio channels: `channels: 0`, `midi: true`, and the graph routes MIDI ports instead of audio ports. The store also needs MIDI indexing (used for seeking and state restore) and a capture group whose staging area holds MIDI events:

```rust
    let config = Config {
        max_frames: 8,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: 256,
        midi_bytes_per_port: 1024,
        max_nodes: 2,
        max_ports: 8,
        max_edges: 2,
        resource_slots: 16,
        resource_pages: 32,
        page_bytes: 64,
        command_slots: 8,
        scheduled_events: 8,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 4,
            replay_events: 128,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 8192,
        },
    )
    .unwrap();
```

## 2. Bind with MIDI capacities

| `Binding` field | Value here | Meaning |
|---|---|---|
| `channels` | `0` | No audio; the loop is MIDI-only. |
| `midi` | `true` | The loop also owns a MIDI capture/playback resource. |
| `midi_events` | `128` | Max events per captured take. |
| `midi_bytes` | `512` | Max packet bytes per take. |
| `packet_bytes` | `512` | Scratch size for one packet during render; must be at least `midi_bytes.max(3)`. |

```rust
    let binding = Binding {
        target,
        playback,
        capture,
        channels: 0,
        midi: true,
        gain: 1.0,
        smoothing: Smoothing::None,
        midi_events: 128,
        midi_bytes: 512,
        packet_bytes: 512,
    };
    let graph = GraphDescription {
        nodes: vec![playback, capture],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Midi, 0),
                destination: Endpoint::node(capture, PortType::Midi, 0),
            },
            Connection {
                source: Endpoint::node(playback, PortType::Midi, 0),
                destination: Endpoint::host(PortType::Midi, 0),
            },
        ],
    };
```

## 3. Record, play, stop

Transitions are identical to audio loops. The input is a `MidiBuffer` — a list of `(offset, payload)` events:

```rust
    let mut input = [MidiBuffer::new(256, 1024).unwrap()];
    input[0].push(0, 8, &[0x90, 60, 100]).unwrap();
    input[0].push(1, 8, &[0xB0, 64, 127]).unwrap();
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 8,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
        .unwrap();
```

The recording is two events over frames 0–1, so its length is 2 frames. Play at frame 4 replays them at offsets 4 and 5, and wraps: positions 0 and 1 again at frames 6 and 7. Stop at frame 7 then appends MIDI cleanup (next chapter), so the block ends with:

```rust
    let events = (0..output[0].len())
        .map(|i| output[0].get(i).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events[events.len() - 4], (4, &[0x90, 60, 100][..]));
    assert_eq!(events[events.len() - 3], (5, &[0xB0, 64, 127][..]));
    assert_eq!(events[events.len() - 2], (7, &[0xB0, 64, 0][..]));
    assert_eq!(events[events.len() - 1], (7, &[0x80, 60, 0][..]));
```

(The earlier events in the block are the reset-all-controllers/all-notes-off preamble the playback node emits at the play boundary; the example asserts the tail because that is the recorded content.)

[Previous: Sync-scheduled recording](../part-3-syncing-loops/07-sync-scheduled-recording.md) · [Next: MIDI state and restoration](08-midi-state-and-restoration.md)
