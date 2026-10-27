# 2. Your first audio graph

<!-- rust-source: ../../examples/first_graph.rs -->

[Contents](../README.md) · [Previous: The runtime and the host](01-the-runtime-and-the-host.md) · [Next: Writing processors](03-writing-processors.md)

This graph multiplies a mono input by `0.5`:

```text
host audio input 0 ──► Gain(0.5) ──► host audio output 0
```

The complete [first graph example](../../examples/first_graph.rs) renders eight samples offline and asserts the result. Run it from the repository root:

```sh
cargo run -p rt_runtime --example first_graph
```

## Prepare the limits

The example configures one host input, one host output, and one processor. `max_ports` includes both host ports and the processor's input/output:

```rust
    let config = Config {
        max_frames: 8,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 1,
        max_ports: 4,
        max_edges: 2,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        ..Config::default()
    }
    .prepare()?;
```

`Config::prepare()` checks limits and arithmetic. Runtime construction then allocates its pools and scratch storage. Both belong outside rendering. The small resource pool is unused here; the configuration retains positive resource capacities even without stored media.

## Describe the connections

`Gain` is a processor supplied by the example. Its implementation is covered in [Writing processors](03-writing-processors.md).

```rust
    let node = NodeId::from_parts(0, 1)?;
    let graph = GraphDescription {
        nodes: vec![node],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: Endpoint::node(node, PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(node, PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, 0),
            },
        ],
    };
```

The `NodeId` identifies slot 0, generation 1. Generations distinguish a processor from a later replacement in the same slot.

Port numbers are local to their endpoint and direction. A host source is an input to the graph; a host destination is an output. A node source is a processor output, and a node destination is an input.

## Install the initial graph

Compile and activate the initial graph during preparation:

```rust
    let resources = ResourceStore::new(config)?;
    let (control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, Gain(0.5))],
        NoPlanner,
        resources,
        &graph,
        &[],
    )?;
    assert_eq!(runtime.epoch(), Epoch(1));
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(control.outstanding(), 0);
```

`PreparedNode` transfers the processor into the runtime. `NoPlanner` and empty decision bindings select ordinary signal processing; [Planning processing intervals](../part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md) introduces the optional planner.

This constructor returns an active graph at epoch 1 and frame 0. It does not run a warm-up callback or consume command/result slots. Invalid graph descriptions fail during preparation. Other constructors start without an active graph and require a later commit, as described in [Routing and live graph edits](../part-2-control-a-running-graph/06-routing-and-live-graph-edits.md).

## Render borrowed buffers

```rust
    let input = [1.0, 0.5, -1.0, 0.0, 0.25, -0.5, 2.0, -2.0];
    let mut output = [0.0; 8];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: input.len(),
                inputs: &[&input],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })?;
    assert_eq!(output, [0.5, 0.25, -0.5, 0.0, 0.125, -0.25, 1.0, -1.0]);
    assert_eq!(runtime.next_frame(), Frame(8));
```

Audio is planar `f32`: one slice per channel. The runtime borrows these buffers only for the call. This graph has no MIDI ports, so its MIDI arrays are empty.

`assert_no_heap` is example verification support, not part of the processing API. It checks allocation, reallocation, and freeing during this scope; it does not detect locks or external I/O.

The example has no callback running after this call, so it drops the runtime and control endpoint outside rendering. A device-backed host must stop callbacks first.

[Next: Writing processors](03-writing-processors.md)
