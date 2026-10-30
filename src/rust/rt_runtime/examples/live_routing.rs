//! Two-node fan-out/fan-in, incremental live commits and checked replacement IDs.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{compile, Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Completion, Lane, Outcome};
use rt_runtime::{Budgets, Config, Error, NodeId};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

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

    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter != 0 || !value.is_finite() {
            return Err(Error::InvalidParameter);
        }
        self.0 = value;
        Ok(())
    }
}

fn audio(source: Endpoint, destination: Endpoint) -> Connection {
    Connection {
        source,
        destination,
    }
}

fn render(runtime: &mut GraphRuntime<Gain>, output: &mut [[f32; 8]; 2]) -> Result<(), Error> {
    let [left, right] = output;
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 8,
                inputs: &[&[1.0; 8]],
                outputs: &mut [left, right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
}

fn apply(
    control: &mut Control<Gain>,
    runtime: &mut GraphRuntime<Gain>,
    command: Command<Gain>,
    output: &mut [[f32; 8]; 2],
    before: [f32; 2],
) -> Completion<Command<Gain>, Retired<Gain>> {
    let ticket = control
        .submit(Lane::Normal, command)
        .map_err(|refusal| refusal.error)
        .unwrap();
    for _ in 0..128 {
        render(runtime, output).unwrap();
        if let Some(completion) = control.try_completion() {
            assert_eq!(completion.sequence, ticket.sequence());
            return completion;
        }
        assert_eq!(*output, [[before[0]; 8], [before[1]; 8]]);
    }
    panic!("live edit exceeded the example's finite validation bound");
}

fn main() -> Result<(), Error> {
    let config = Config {
        max_frames: 8,
        audio_inputs: 1,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 2,
        max_ports: 8,
        max_edges: 8,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        budgets: Budgets {
            validation_steps: 1,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let a = NodeId::from_parts(0, 1)?;
    let b = NodeId::from_parts(1, 1)?;
    let host = |port| Endpoint::host(PortType::Audio, port);
    let node = |id| Endpoint::node(id, PortType::Audio, 0);
    let graph = GraphDescription {
        nodes: vec![b, a],
        connections: vec![
            audio(host(0), node(a)),
            audio(host(0), node(b)),
            audio(node(a), host(0)),
            audio(node(b), host(0)),
            audio(node(b), host(1)),
        ],
    };
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![
            PreparedNode::new(a, Gain(0.5)),
            PreparedNode::new(b, Gain(0.25)),
        ],
        NoPlanner,
        ResourceStore::new(config)?,
        &graph,
        &[],
    )?;
    let initial_epoch = runtime.epoch();
    let mut output = [[0.0; 8]; 2];
    render(&mut runtime, &mut output)?;
    assert_eq!(output, [[0.75; 8], [0.25; 8]]);
    let mut reversed = graph.clone();
    reversed.nodes.reverse();
    reversed.connections.reverse();
    let canonical = compile(runtime.catalog(), &reversed)?;
    assert_eq!(
        canonical
            .descriptor()
            .steps
            .iter()
            .map(|step| step.node)
            .collect::<Vec<_>>(),
        [a, b]
    );
    drop(canonical);
    let mut cyclic = graph.clone();
    cyclic
        .connections
        .extend([audio(node(a), node(b)), audio(node(b), node(a))]);
    assert!(matches!(
        compile(runtime.catalog(), &cyclic),
        Err(Error::Cycle)
    ));

    let only_b = GraphDescription {
        nodes: vec![b],
        connections: vec![
            audio(host(0), node(b)),
            audio(node(b), host(0)),
            audio(node(b), host(1)),
        ],
    };
// Compile a candidate off-render; committing it validates incrementally over callbacks.
    let plan = compile(runtime.catalog(), &only_b)?;
    let ticket = control
        .submit(Lane::Normal, Command::commit(runtime.epoch(), plan))
        .map_err(|refusal| refusal.error)?;
    render(&mut runtime, &mut output)?;
    assert_eq!(output, [[0.75; 8], [0.25; 8]]);
    assert_eq!(runtime.epoch(), initial_epoch);
    assert!(control.try_completion().is_none());
    let mut applied = false;
    for _ in 0..128 {
        render(&mut runtime, &mut output)?;
        if let Some(completion) = control.try_completion() {
            assert_eq!(completion.sequence, ticket.sequence());
            assert_eq!(completion.outcome, Outcome::Applied);
            assert!(matches!(&completion.retired, Some(Retired::Plan(_))));
            drop(completion);
            applied = true;
            break;
        }
        assert_eq!(output, [[0.75; 8], [0.25; 8]]);
    }
    assert!(applied);
    assert_eq!(output, [[0.25; 8]; 2]);
    let at = runtime.next_frame().checked_add(1024)?;
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::Schedule {
            at,
            action: Action::Parameter {
                node: a,
                parameter: 0,
                value: 9.0,
            },
            payload: Box::default(),
        },
        &mut output,
        [0.25; 2],
    );
    assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
    drop(completion);

    let completion = apply(
        &mut control,
        &mut runtime,
        Command::Remove { node: a },
        &mut output,
        [0.25; 2],
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(
        &completion.retired,
        Some(Retired::Processor(Gain(0.5)))
    ));
    drop(completion);
    let replacement = a.checked_next_generation()?;
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::Install {
            prepared: Some(PreparedNode::new(replacement, Gain(0.125))),
        },
        &mut output,
        [0.25; 2],
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    drop(completion);

    let plan = compile(runtime.catalog(), &only_b)?;
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::commit(initial_epoch, plan),
        &mut output,
        [0.25; 2],
    );
    assert_eq!(completion.outcome, Outcome::Rejected(Error::StaleEpoch));
    assert!(matches!(
        &completion.request,
        Command::Commit { plan: Some(_), .. }
    ));
    assert!(completion.retired.is_none());
    drop(completion);
    let updated = GraphDescription {
        nodes: vec![replacement, b],
        connections: vec![
            audio(host(0), node(replacement)),
            audio(node(replacement), host(0)),
            audio(host(0), node(b)),
            audio(node(b), host(0)),
            audio(node(b), host(1)),
        ],
    };
    let plan = compile(runtime.catalog(), &updated)?;
    let epoch = runtime.epoch();
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::commit(epoch, plan),
        &mut output,
        [0.25; 2],
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    drop(completion);
    assert_eq!(output, [[0.375; 8], [0.25; 8]]);
    let mut executed = false;
    for _ in 0..256 {
        render(&mut runtime, &mut output)?;
        assert_eq!(output, [[0.375; 8], [0.25; 8]]);
        if let Some(execution) = control.try_execution() {
            assert_eq!(execution.actual_frame, at);
            assert_eq!(execution.result, Err(Error::InvalidId));
            executed = true;
            break;
        }
    }
    assert!(executed);
    assert_eq!(control.outstanding(), 0);
    drop(runtime);
    drop(control);
    println!("live routing: fan-in, incremental/stale commits, retired owners and replacement generations verified");
    Ok(())
}
