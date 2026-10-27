//! Owning returns, bounded recovery priority, cancellation and execution receipts.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{compile, Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Error, Frame, NodeId};
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

fn render(runtime: &mut GraphRuntime<Gain>, output: &mut [f32; 8]) -> Result<(), Error> {
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 8,
                inputs: &[&[1.0; 8]],
                outputs: &mut [output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
}

fn demonstrate(publications: usize, cancel_recovery: bool) -> Result<(), Error> {
    let config = Config {
        max_frames: 8,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 1,
        max_ports: 4,
        max_edges: 2,
        command_slots: 2,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        budgets: Budgets {
            commands: 2,
            validation_steps: 1,
            publications,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let node = NodeId::from_parts(0, 1)?;
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
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, Gain(0.5))],
        NoPlanner,
        ResourceStore::new(config)?,
        &description,
        &[],
    )?;
    let first = control
        .submit(
            Lane::Normal,
            Command::commit(runtime.epoch(), compile(runtime.catalog(), &description)?),
        )
        .map_err(|refusal| refusal.error)?;
    let later = control
        .submit(Lane::Normal, Command::Remove { node })
        .map_err(|refusal| refusal.error)?;
    let refused = control
        .submit(
            Lane::Normal,
            Command::commit(runtime.epoch(), compile(runtime.catalog(), &description)?),
        )
        .err()
        .unwrap();
    assert_eq!(refused.error, Error::Full);
    assert!(
        matches!(&refused.value, Command::Commit { plan: Some(plan), .. } if plan.contains(node))
    );
    drop(refused);

    let mut output = [0.0; 8];
    render(&mut runtime, &mut output)?;
    assert!(control.try_completion().is_none());
    first.cancel();
    let recovery = control
        .submit(Lane::Recovery, Command::ResetMidi)
        .map_err(|refusal| refusal.error)?;
    if cancel_recovery {
        recovery.cancel();
    }
    for _ in 0..64 {
        render(&mut runtime, &mut output)?;
        assert_eq!(output, [0.5; 8]);
    }
    let refused = control
        .submit(Lane::Normal, Command::Remove { node })
        .err()
        .unwrap();
    assert_eq!(refused.error, Error::Full);
    drop(refused);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, recovery.sequence());
    assert_eq!(completion.frame, Frame(8));
    assert_eq!(
        completion.outcome,
        if cancel_recovery {
            Outcome::Cancelled
        } else {
            Outcome::Applied
        }
    );
    drop(completion);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, first.sequence());
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(
        &completion.request,
        Command::Commit { plan: None, .. }
    ));
    assert!(matches!(&completion.retired, Some(Retired::Plan(_))));
    drop(completion);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, later.sequence());
    assert_eq!(completion.outcome, Outcome::Rejected(Error::Busy));
    drop(completion);
    assert_eq!(control.outstanding(), 0);

    let cancelled = control
        .submit(Lane::Normal, Command::Remove { node })
        .map_err(|refusal| refusal.error)?;
    cancelled.cancel();
    render(&mut runtime, &mut output)?;
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, cancelled.sequence());
    assert_eq!(completion.outcome, Outcome::Cancelled);
    drop(completion);

    let at = runtime.next_frame().checked_add(2)?;
    let scheduled = control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at,
                action: Action::Parameter {
                    node,
                    parameter: 0,
                    value: 0.25,
                },
                payload: Box::default(),
            },
        )
        .map_err(|refusal| refusal.error)?;
    render(&mut runtime, &mut output)?;
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, scheduled.sequence());
    assert_eq!(
        completion.outcome,
        Outcome::Scheduled {
            effective_frame: at,
            late_by: 0
        }
    );
    drop(completion);
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.sequence, scheduled.sequence());
    assert_eq!(execution.actual_frame, at);
    assert_eq!(execution.result, Ok(()));
    assert_eq!(output, [0.5, 0.5, 0.25, 0.25, 0.25, 0.25, 0.25, 0.25]);
    assert_eq!(control.outstanding(), 0);
    assert!(control.try_completion().is_none());
    assert!(control.try_execution().is_none());
    drop(runtime);
    drop(control);
    Ok(())
}

fn main() -> Result<(), Error> {
    for publications in [1, 2] {
        for cancel_recovery in [false, true] {
            demonstrate(publications, cancel_recovery)?;
        }
    }
    println!("owning commands: refused owners, recovery/FIFO, cancellation and both result streams verified");
    Ok(())
}
