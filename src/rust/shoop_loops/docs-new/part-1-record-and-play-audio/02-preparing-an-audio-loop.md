# 2. Preparing an audio loop

<!-- rust-source: ../examples/first_loop.rs -->

[Contents](../README.md) · [Previous: The loop library and the host](01-the-loop-library-and-the-host.md) · [Next: Recording, playing, and stopping](03-recording-playing-and-stopping.md)

Our loop has two stereo routes:

```text
host input 0 ──► capture input 0       playback output 0 ──► host output 0
host input 1 ──► capture input 1       playback output 1 ──► host output 1
```

Capture and playback belong to the same loop, but there is no signal connection between them. The planner makes the recorded content available when recording ends.

The complete [first loop example](../examples/first_loop.rs) includes preparation, processing, result checks, and cleanup. Run it from the repository root:

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin first_loop
```

This chapter prepares the loop and its runtime. The next chapter submits Record, Play, and Stop and checks the output.

## 1. Reserve runtime storage

`rt_runtime::Config` sets host buffer shapes and prepared capacities:

```rust
    let config = Config {
        max_frames: 8,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 2,
        max_ports: 8,
        max_edges: 4,
        resource_slots: 8,
        resource_pages: 16,
        page_bytes: 64,
        command_slots: 4,
        scheduled_events: 4,
        ..Config::default()
    }
    .prepare()
    .unwrap();
```

There are two processor nodes: playback and capture. The eight audio ports are four host ports, two playback outputs, and two capture inputs. Four edges connect them. Host calls may contain up to eight frames; MIDI port counts are zero.

The data pool has sixteen 64-byte pages. Resource identities, versions, and snapshot leases each have eight slots. Those are separate pools: enough sample bytes alone is not enough to start a recording. This example leaves room for its capture reservations and retained content rather than trying to find minimum capacities.

Prepare capture storage alongside the data pool:

```rust
    let mut resources = ResourceStore::with_capture_groups(
        config,
        None,
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 64,
        },
    )
    .unwrap();
```

The planner uses two capture banks per media-bearing loop so a new take can be prepared before an old one ends. Each group here has one member: a stereo audio resource, not one resource per channel. Its staging area holds `8 frames × 2 channels × 4 bytes = 64 bytes`. The `None` disables MIDI indexing, which this audio-only loop does not need.

All construction in this chapter happens outside rendering.

## 2. Describe the musical loop

A **topology** describes loops and their musical relationships. This one has a single empty primitive loop with no sync source:

```rust
    let target = LoopId::from_parts(0, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 4,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: target,
                kind: LoopKind::Primitive,
                length: 0,
                sync_source: None,
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
```

`LoopId` identifies logical slot 0, generation 1. It is the target of Record and Play, not a signal port. Generations distinguish a loop from a later loop reusing its slot.

`length: 0` means there is no initial content. Recording will determine its length. `compile` validates the topology and prepares its timing metadata; it does not create an audio route or record samples.

## 3. Bind playback and capture

Give the two signal nodes distinct runtime node identities and bind them to the loop:

```rust
    let playback = NodeId::from_parts(0, 1).unwrap();
    let capture = NodeId::from_parts(1, 1).unwrap();
    let binding = Binding {
        target,
        playback,
        capture,
        channels: 2,
        midi: false,
        gain: 1.0,
        smoothing: Smoothing::None,
        midi_events: 0,
        midi_bytes: 0,
        packet_bytes: 0,
    };
```

`NodeId` addresses a physical signal node; `LoopId` addresses the musical loop. They are different types even when their slot numbers happen to match.

Playback has two outputs and capture has two inputs. The library constructs both processors and connects them to its timing decisions. We need no custom DSP. Unity gain and no boundary smoothing keep the expected samples exact; abrupt starts and wraps may click with real audio. The three MIDI capacity fields are unused because `midi` is false.

## 4. Connect the signal graph

The **graph** describes signal flow, separately from the musical topology:

```rust
    let mut graph = GraphDescription {
        nodes: vec![playback, capture],
        connections: Vec::new(),
    };
    for channel in 0..2 {
        graph.connections.extend([
            Connection {
                source: Endpoint::host(PortType::Audio, channel),
                destination: Endpoint::node(capture, PortType::Audio, channel),
            },
            Connection {
                source: Endpoint::node(playback, PortType::Audio, channel),
                destination: Endpoint::host(PortType::Audio, channel),
            },
        ]);
    }
```

A host source is an input to the graph; a host destination is an output. Node sources are processor outputs, and node destinations are inputs. Each connection preserves its channel number.

Input goes only to capture. Output comes only from playback, so stopping the loop will silence both host outputs even if host input remains nonzero.

## 5. Prepare the loop parts

`LoopPlanner::prepare` validates the loop definitions and creates its capture resources in our store:

```rust
    let Prepared {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 3,
            media_slots: 4,
            export_slots: 0,
        },
        vec![binding],
        vec![],
        Options::default(),
    )
    .unwrap();
```

The planner identity names the loop's timing authority inside the runtime. Three normal request slots cover our three transitions; the topology's fourth pending slot is reserved for recovery. Media slots hold content metadata separately from sample pages. We reserve no external exports in this example. The empty list supplies no imported media, and the default options enable no staged changes.

## 6. Install the parts in a runtime

The runtime owns the processors. Its node type here is the loop's own `LoopNode`, because we have no processors of our own; an application with other processors wraps both in its own enum. The initial graph needs the loop's decision bindings for the nodes it contains:

```rust
    let nodes = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, node))
        .collect();
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        planner,
        resources,
        &graph,
        &client.bindings_for(&graph),
    )
    .unwrap();
    control.prepare_submissions().unwrap();
    assert_eq!(runtime.next_frame(), Frame(0));
```

Preparation validates and installs the graph without a warm-up callback. The planner refuses a graph that lacks the loop's nodes or bindings. The loop is stopped at frame zero, and the runtime owns the planner, the processors and the storage. Preparing submissions reserves the control endpoint's cancellation flags so that later submissions do not allocate them. Next, we will ask the loop to record.

[Next: Recording, playing, and stopping](03-recording-playing-and-stopping.md)
