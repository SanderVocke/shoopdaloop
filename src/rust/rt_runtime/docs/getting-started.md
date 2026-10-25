# Getting started

[Overview](../README.md) · [Host integration](integration.md) · [Capacity and performance](performance.md)

Use `executor::GraphRuntime` for graphs. The lower-level `RenderRuntime` only validates audio blocks, advances a clock, and produces silence; it does not run processors.

## Build a gain graph

```text
host audio input 0 ──► Gain(0.5) ──► host audio output 0
                         │
                   one stable NodeId
```

The following complete program prepares one processor and commits its routing. It drives callbacks synchronously for an offline demonstration; in a live host, the control endpoint and callback run independently.

```rust
use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, GraphRuntime, PreparedNode};
use rt_runtime::graph::{
    compile, Connection, Endpoint, GraphDescription, PortCounts, PortType,
};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Config, Error, NodeId};

struct Gain(f32);

impl Processor for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * self.0;
        }
        Ok(())
    }
}

fn main() -> Result<(), Error> {
    // Everything through submit happens outside the render callback.
    let config = Config {
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 1,
        max_ports: 4, // two host ports + two processor ports
        max_edges: 2,
        ..Config::default()
    }.prepare()?;
    let id = NodeId::from_parts(0, 1)?;
    let (mut control, mut runtime) = GraphRuntime::prepare(
        config,
        vec![PreparedNode::new(id, Gain(0.5))],
    )?;
    let description = GraphDescription {
        nodes: vec![id],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: Endpoint::node(id, PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(id, PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, 0),
            },
        ],
    };
    let plan = compile(runtime.catalog(), &description)?;
    let ticket = control.submit(
        Lane::Normal,
        Command::commit(runtime.epoch(), plan),
    ).map_err(|refusal| refusal.error)?;

    let input = [1.0; 128];
    let mut output = [0.0; 128];
    loop {
        let frame = runtime.next_frame();
        // Render callback scope: borrowed buffers, no heap work.
        runtime.process(
            AudioBlock {
                frame,
                frames: 128,
                inputs: &[&input],
                outputs: &mut [&mut output],
            },
            MidiBlock { inputs: &[], outputs: &mut [] },
        )?;
        // Back in control context; validation may take several callbacks.
        if let Some(completion) = control.try_completion() {
            assert_eq!(completion.outcome, Outcome::Applied);
            drop(completion);
            break;
        }
    }
    assert_eq!(output, [0.5; 128]);
    drop(ticket);
    // No callback can run again: final destruction is non-RT.
    drop(runtime);
    drop(control);
    Ok(())
}
```

`submit` succeeding only means the command was queued. Until the first plan activates, output is silent. Use the completion to determine whether an edit applied; production code must also handle rejected and cancelled commands and retain refused payloads when retrying.

For exact-frame gain automation and explicit removal, run the [gain graph example](../examples/gain_graph.rs).

## Write a processor

| Entry point | Context | Obligation |
|---|---|---|
| Construction / optional `PrepareProcessor::prepare` | Non-RT | Allocate and touch all working storage |
| `Processor::ports` via `PreparedNode::new` | Non-RT | Declare a fixed port signature |
| `Processor::process` | Render | Process `block.frames`, which may be shorter than a host block |
| `Processor::set_parameter` | Render, before the target sample | Bounded mutation; default implementation returns `InvalidParameter` |
| Destruction | Non-RT | Reclaim only after ownership is returned or callbacks have stopped |

Use `Box<dyn Processor>` for heterogeneous processors, or an application-defined enum for concrete dispatch. `Processor: Send` allows moving the runtime to its render owner; it does not make concurrent access to a processor safe.

Inside processing:

- Read planar audio through `AudioPorts`; `pair` borrows one input/output pair without building temporary vectors.
- Treat MIDI offsets as relative to the current **subrange**, not the original host block.
- Use `block.resources` for [bounded resource I/O](resources.md).
- Do not allocate, free, lock, wait, perform external I/O, log through an unbounded logger, or panic in normal processing.
- Continue audio work when ordinary MIDI output fills. Handle reset packets and report `Error::Full` after doing the required audio work. Other propagated processor errors fail and silence the host block.

Expected auxiliary failures, such as a full recording store, should be handled locally if audio must continue. See [capture failure handling](resources.md#capture).
