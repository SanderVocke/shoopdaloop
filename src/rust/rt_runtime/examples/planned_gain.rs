//! Prepared decisions shared with a signal consumer, with exact-frame automation.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, GraphRuntime, PreparedNode};
use rt_runtime::graph::{
    compile_planned, Connection, Endpoint, GraphDescription, PortCounts, PortType,
};
use rt_runtime::planner::{
    ConsumerAccess, DecisionBinding, FinishContext, PlanContext, Planner, PlannerSpec,
};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::schedule::Action;
use rt_runtime::transport::Lane;
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId};

struct AlternatingGain {
    decision: [f32; 1],
    id: PlannerId,
}
impl Planner for AlternatingGain {
    type Decision = f32;
    type Observation = usize;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: self.id,
            decision_slots: 1,
            observation_capacity: 1,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[f32] {
        &self.decision
    }
    fn plan(&mut self, context: PlanContext<'_>) -> Result<Frame, Error> {
        self.decision[0] = 1.0 + ((context.frame.0 / 8) % 2) as f32;
        Ok(context
            .frame
            .checked_add(8 - (context.frame.0 % 8) as usize)?
            .min(context.ceiling))
    }
    fn finish(&mut self, context: FinishContext<'_, usize>) -> Result<(), Error> {
        if context.observations.len() != 1
            || context.observations[0].value as u64
                != context.interval.end.0 - context.interval.start.0
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
}
struct Gain {
    parameter: f32,
}
impl Processor<AlternatingGain> for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            decision_required: true,
            observations: 1,
            operation_bytes: 0,
        }
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter != 0 {
            return Err(Error::InvalidParameter);
        }
        self.parameter = value;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, AlternatingGain>) -> Result<(), Error> {
        let gain = *block.decision.as_ref().ok_or(Error::InvalidState)?.value * self.parameter;
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * gain;
        }
        block.observations.push(block.frames)
    }
}
fn main() -> Result<(), Error> {
    let config = Config {
        max_frames: 32,
        max_nodes: 1,
        max_ports: 4,
        max_edges: 2,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        resource_pages: 4,
        page_bytes: 64,
        budgets: Budgets {
            events: 1,
            subranges: 0,
            validation_steps: 128,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let node = NodeId::from_parts(0, 1)?;
    let planner_id = PlannerId::from_parts(0, 1)?;
    let planner = AlternatingGain {
        decision: [1.0],
        id: planner_id,
    };
    let (mut control, mut runtime) = GraphRuntime::prepare_with_planner(
        config,
        vec![PreparedNode::new(node, Gain { parameter: 1.0 })],
        planner,
    )?;
    let port = Endpoint::node(node, PortType::Audio, 0);
    let host = Endpoint {
        node: None,
        kind: PortType::Audio,
        port: 0,
    };
    let graph = GraphDescription {
        nodes: vec![node],
        connections: vec![
            Connection {
                source: host,
                destination: port,
            },
            Connection {
                source: port,
                destination: host,
            },
        ],
    };
    let plan = compile_planned(
        runtime.catalog(),
        &graph,
        &[DecisionBinding {
            planner: planner_id,
            consumer: node,
            decision_slot: 0,
        }],
    )?;
    let _commit = control
        .submit(Lane::Normal, Command::commit(Epoch(0), plan))
        .map_err(|error| error.error)?;
    let _event = control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(13),
                action: Action::Parameter {
                    node,
                    parameter: 0,
                    value: 0.5,
                },
                payload: Box::new([]),
            },
        )
        .map_err(|error| error.error)?;
    let input = [1.0; 32];
    let mut output = [0.0; 32];
    runtime.process(
        AudioBlock {
            frame: Frame(0),
            frames: 32,
            inputs: &[&input],
            outputs: &mut [&mut output],
        },
        MidiBlock {
            inputs: &[],
            outputs: &mut [],
        },
    )?;
    for (frame, sample) in output.iter().enumerate() {
        assert_eq!(
            *sample,
            (1 + (frame / 8) % 2) as f32 * if frame < 13 { 1.0 } else { 0.5 }
        );
    }
    let execution = control.try_execution().ok_or(Error::InvalidState)?;
    assert_eq!(execution.actual_frame, Frame(13));
    assert_eq!(execution.result, Ok(()));
    assert_eq!(runtime.diagnostics().subranges, 5);
    while control.try_completion().is_some() {}
    println!("planned gain: 32 checked samples, five intervals, one exact-frame event");
    Ok(())
}
