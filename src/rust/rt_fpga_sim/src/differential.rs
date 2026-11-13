//! A common producer drives both owning transports; hardware images are never inputs.
use crate::{SimResult, bridge_sim::Probe};
use rt_audio_math::Q824;
use rt_backend_testbench::spec::{self, Scenario};
use rt_fpga_adapter::adapter::{Adapter, Controller, Device, Node, Operation, Processor, Request};
use rt_runtime::{
    Budgets, Config, Epoch, Frame, NodeId, PreparedConfig, Quantum,
    buffer::{AudioBlock, MidiBlock},
    executor::GraphRuntime,
    graph::{self, Catalog, Connection, Endpoint, GraphDescription, PortType},
    planner::NoPlanner,
    schedule::Action,
    transport::{Lane, Outcome},
};
use std::time::{Duration, Instant};

pub struct Report {
    pub quanta: usize,
    pub samples: usize,
    pub cycles: u64,
    pub compile: Duration,
    pub configure: Duration,
    pub instructions: usize,
    pub state: usize,
    pub image_bytes: usize,
    pub gain_writes: u64,
}
pub fn config() -> PreparedConfig {
    Config {
        max_frames: 32,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 4,
        max_ports: 16,
        max_edges: 16,
        budgets: Budgets {
            commands: 16,
            publications: 16,
            validation_steps: 4096,
            ..Default::default()
        },
        ..Default::default()
    }
    .prepare()
    .unwrap()
}
fn id(id: spec::Identity, generation_delta: u32) -> SimResult<NodeId> {
    Ok(NodeId::from_parts(
        id.slot,
        id.generation
            .checked_add(generation_delta)
            .ok_or("generation overflow")?,
    )?)
}
fn nodes(case: &Scenario) -> SimResult<Vec<Node>> {
    case.nodes
        .iter()
        .map(|node| {
            let spec::NodeKind::Gain { channels, initial } = node.operation else {
                return Err("queue profile supports gain nodes only".into());
            };
            Ok(Node::new(
                id(node.id, 0)?,
                Processor::new(usize::from(channels), 32, Q824(initial)),
            ))
        })
        .collect()
}
fn graph(case: &Scenario) -> SimResult<graph::PreparedPlan> {
    let specs = nodes(case)?
        .iter()
        .map(|n| {
            let mut spec = n.spec();
            spec.id = spec.id.checked_next_generation().unwrap();
            spec
        })
        .collect::<Vec<_>>();
    let graph = GraphDescription {
        nodes: specs.iter().map(|s| s.id).collect(),
        connections: case
            .connections
            .iter()
            .map(|edge| {
                let endpoint = |e: spec::Endpoint| -> SimResult<Endpoint> {
                    Ok(Endpoint {
                        node: e.node.map(|i| id(i, 1)).transpose()?,
                        kind: PortType::Audio,
                        port: u32::from(e.port),
                    })
                };
                Ok(Connection {
                    source: endpoint(edge.source)?,
                    destination: endpoint(edge.destination)?,
                })
            })
            .collect::<SimResult<_>>()?,
    };
    Ok(graph::compile(&Catalog::new(config(), &specs)?, &graph)?)
}
fn submit(control: &mut Controller, request: Request) -> SimResult<()> {
    control
        .submit(Lane::Normal, request)
        .map(|_| ())
        .map_err(|e| e.error.into())
}
fn install(control: &mut Controller, case: &Scenario) -> SimResult<()> {
    // Stable slots are declared at preparation in the native runtime. Exercise
    // their ordinary retirement/reinstallation protocol before the initial graph.
    for node in nodes(case)? {
        let next = node.spec().id.checked_next_generation()?;
        submit(
            control,
            Request::Remove {
                node: node.spec().id,
            },
        )?;
        submit(
            control,
            Request::Install {
                prepared: Some(Node::new(next, node.into_processor())),
            },
        )?;
    }
    submit(
        control,
        Request::commit(Epoch(case.nodes.len() as u64 * 2), graph(case)?),
    )
}
fn actions(control: &mut Controller, step: &spec::Step) -> SimResult<()> {
    for action in &step.actions {
        let spec::Action::Gain {
            node,
            at,
            target,
            duration,
            ..
        } = *action
        else {
            return Err("queue profile supports gain actions only".into());
        };
        submit(
            control,
            Request::Schedule {
                at: Frame(at),
                action: Action::Processor {
                    node: id(node, 1)?,
                    operation: Operation {
                        target: Q824(target),
                        duration,
                    },
                },
                payload: Box::new([]),
            },
        )?;
    }
    Ok(())
}
fn compare_results(a: &mut Controller, b: &mut Controller) -> SimResult<()> {
    loop {
        match (a.try_completion(), b.try_completion()) {
            (None, None) => break,
            (Some(a), Some(b)) => {
                if (a.sequence, a.lane, a.outcome, a.frame, a.epoch)
                    != (b.sequence, b.lane, b.outcome, b.frame, b.epoch)
                {
                    return Err(format!(
                        "completion mismatch: {:?} != {:?}",
                        (a.sequence, a.outcome, a.frame, a.epoch),
                        (b.sequence, b.outcome, b.frame, b.epoch)
                    )
                    .into());
                }
                if matches!(a.outcome, Outcome::Rejected(_)) {
                    return Err(format!("supported replay rejected: {:?}", a.outcome).into());
                }
                if a.retired.is_some() != b.retired.is_some() {
                    return Err("retirement mismatch".into());
                }
            }
            _ => return Err("completion count mismatch".into()),
        }
    }
    loop {
        match (a.try_execution(), b.try_execution()) {
            (None, None) => break,
            (Some(a), Some(b)) if a == b => {}
            _ => return Err("execution receipt mismatch".into()),
        }
    }
    Ok(())
}
pub fn run<D: Device + Probe>(case: &Scenario, device: D) -> SimResult<Report> {
    case.validate()?;
    if case.quantum != 32
        || case.chunk_frames != 32
        || case.input_ports != 1
        || case.output_ports != 1
        || !case.loops.is_empty()
        || !case.media.is_empty()
        || case.nodes.len() > 2
    {
        return Err("unsupported queue replay profile".into());
    }
    let (mut cpu_control, mut cpu) =
        GraphRuntime::<Processor, NoPlanner, Operation, Q824>::prepare_with_planner(
            config(),
            Quantum::new(32)?,
            nodes(case)?,
            NoPlanner,
        )?;
    let (mut hw_control, mut hw) = Adapter::prepare(config(), nodes(case)?, device)?;
    install(&mut cpu_control, case)?;
    install(&mut hw_control, case)?;
    let mut samples = 0;
    let start = Instant::now();
    for step in &case.steps {
        actions(&mut cpu_control, step)?;
        actions(&mut hw_control, step)?;
        let input = step.inputs[0].iter().copied().map(Q824).collect::<Vec<_>>();
        let mut output = vec![Q824(0); 32];
        cpu.process(
            AudioBlock {
                frame: cpu.next_frame(),
                frames: 32,
                inputs: &[&input],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )?;
        let actual = hw
            .process(&step.inputs)
            .map_err(|e| format!("hardware {e}: {:?}", hw.device().last_error()))?;
        if actual[0].iter().copied().ne(output.iter().map(|s| s.0)) {
            return Err(format!("audio mismatch at frame {}", hw.frame().0 - 32).into());
        }
        if cpu.next_frame() != hw.frame()
            || cpu.epoch() != hw.epoch()
            || hw.device_mut().frame() != hw.frame().0
        {
            return Err("frame/epoch mismatch".into());
        }
        compare_results(&mut cpu_control, &mut hw_control)?;
        samples += 32;
    }
    if hw.metrics.compilations != 1 {
        return Err("graph was recompiled".into());
    }
    println!(
        "queue differential elapsed {:?}; one runtime graph compilation",
        start.elapsed()
    );
    let m = &hw.metrics;
    Ok(Report {
        quanta: case.steps.len(),
        samples,
        cycles: hw.device().cycles(),
        compile: m.compile_time,
        configure: m.configuration_time,
        instructions: m.instructions,
        state: m.state_words,
        image_bytes: m.image_bytes,
        gain_writes: m.gain_writes,
    })
}
