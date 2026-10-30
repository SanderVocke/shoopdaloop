//! Public-API graph preparation, exact-frame control, and owning teardown.
#[path = "../tests/support/mod.rs"]
mod support;
use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{compile, Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Completion, Lane, Outcome};
use rt_runtime::{Config, Error, Frame, NodeId};

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
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
fn render(runtime: &mut GraphRuntime<Gain>, output: &mut [f32; 128]) {
    support::assert_no_heap(|| {
        runtime
            .process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames: 128,
                    inputs: &[&[1.; 128]],
                    outputs: &mut [output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
            .unwrap()
    });
}
fn apply(
    control: &mut Control<Gain>,
    runtime: &mut GraphRuntime<Gain>,
    command: Command<Gain>,
    output: &mut [f32; 128],
) -> Completion<Command<Gain>, Retired<Gain>> {
    control
        .submit(Lane::Normal, command)
        .map_err(|refusal| refusal.error)
        .unwrap();
    for _ in 0..64 {
        render(runtime, output);
        if let Some(completion) = control.try_completion() {
            return completion;
        }
    }
    panic!("example command did not complete within its finite validation bound");
}
// Two gains fed from one input: fan-out copies, and both outputs sum their sources.
fn main() {
    let config = Config {
        max_frames: 128,
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
    .prepare()
    .unwrap();
    let node = NodeId::from_parts(0, 1).unwrap();
    let (mut control, mut runtime) =
        GraphRuntime::prepare(config, vec![PreparedNode::new(node, Gain(0.5))]).unwrap();
    let description = GraphDescription {
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
    let plan = compile(runtime.catalog(), &description).unwrap();
    let epoch = runtime.epoch();
    let mut output = [0.; 128];
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::commit(epoch, plan),
        &mut output,
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    drop(completion);
    assert_eq!(output, [0.5; 128]);

    let at = Frame(runtime.next_frame().0 + 64);
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::Schedule {
            at,
            action: Action::Parameter {
                node,
                parameter: 0,
                value: 0.25,
            },
            payload: Box::default(),
        },
        &mut output,
    );
    assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
    drop(completion);
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.actual_frame, at);
    assert_eq!(execution.result, Ok(()));
    assert_eq!(output[..64], [0.5; 64]);
    assert_eq!(output[64..], [0.25; 64]);

    let empty = compile(
        runtime.catalog(),
        &GraphDescription {
            nodes: vec![],
            connections: vec![],
        },
    )
    .unwrap();
    let epoch = runtime.epoch();
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::commit(epoch, empty),
        &mut output,
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(completion.retired, Some(Retired::Plan(_))));
    drop(completion);
    assert_eq!(output, [0.; 128]);
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::Remove { node },
        &mut output,
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(completion.retired, Some(Retired::Processor(_))));
    drop(completion);
    assert_eq!(control.outstanding(), 0);
    drop(runtime);
    drop(control);
    println!("gain graph: exact-frame output and owning teardown verified");
}
