use crate::{
    compiler::{self, Builder},
    image::*,
};
use alloc::collections::BTreeMap;
use alloc::{format, string::String, vec, vec::Vec};
use rt_runtime::{
    Error, NodeId,
    graph::{PlanDescriptor, PortType},
};
use shoop_rt_engine::{EngineNode, NodeConfiguration, external::SignalKind};

pub const ONE: i32 = 1 << 24;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Parameter {
    Gain,
    Balance,
    Mute,
    Presence,
    Bank,
    Monitor,
}
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub node: NodeId,
    pub parameter: Parameter,
    pub address: u16,
}
pub struct Node {
    pub id: NodeId,
    pub processor: EngineNode,
    pub kind: SignalKind,
    pub configuration: Option<NodeConfiguration>,
    pub signal: Signal,
}
#[derive(Clone)]
pub struct Signal {
    pub inputs: usize,
    pub outputs: usize,
    pub manual_monitor: bool,
    pub generation: u32,
    pub ramps: BTreeMap<Parameter, Ramp>,
}
impl core::ops::Deref for Node {
    type Target = Signal;
    fn deref(&self) -> &Signal {
        &self.signal
    }
}
impl core::ops::DerefMut for Node {
    fn deref_mut(&mut self) -> &mut Signal {
        &mut self.signal
    }
}

impl Node {
    pub fn new(id: NodeId, processor: EngineNode) -> Result<Self, Error> {
        let kind = processor.signal_kind()?;
        let ramps = [
            Parameter::Gain,
            Parameter::Balance,
            Parameter::Mute,
            Parameter::Presence,
            Parameter::Bank,
            Parameter::Monitor,
        ]
        .into_iter()
        .map(|p| {
            let value = match p {
                Parameter::Balance | Parameter::Bank | Parameter::Monitor => 0,
                _ => ONE,
            };
            (
                p,
                Ramp {
                    start: value,
                    target: value,
                    duration: 0,
                    elapsed: 0,
                },
            )
        })
        .collect();
        let outputs = if let SignalKind::Bus { channels, .. } = kind {
            channels
        } else {
            0
        };
        Ok(Self {
            id,
            processor,
            kind,
            configuration: None,
            signal: Signal {
                inputs: 0,
                outputs,
                manual_monitor: false,
                generation: 0,
                ramps,
            },
        })
    }
    pub fn duration(&self) -> u32 {
        match self.kind {
            SignalKind::StripInput { ramp, .. }
            | SignalKind::StripOutput { ramp, .. }
            | SignalKind::Fader { ramp, .. }
            | SignalKind::Bus { ramp, .. } => ramp,
            _ => 0,
        }
    }
    pub fn configure(&mut self, configuration: NodeConfiguration) {
        self.inputs = configuration.inputs as usize;
        self.outputs = configuration.outputs as usize;
        self.manual_monitor = configuration.monitoring;
        self.generation = configuration.generation;
        for (parameter, value) in [
            (Parameter::Gain, (configuration.gain * ONE as f32) as i32),
            (
                Parameter::Balance,
                (configuration.balance * ONE as f32) as i32,
            ),
            (Parameter::Mute, if configuration.muted { 0 } else { ONE }),
            (Parameter::Bank, i32::from(configuration.bank) * ONE),
            (Parameter::Presence, ONE),
        ] {
            self.ramps.insert(
                parameter,
                Ramp {
                    start: value,
                    target: value,
                    duration: 0,
                    elapsed: 0,
                },
            );
        }
    }
}
pub struct Compiled {
    pub image: Image,
    pub bindings: Vec<Binding>,
    pub meters: Vec<(SignalKind, Vec<u16>)>,
    pub smoothing: Vec<(u16, u64)>,
}
fn gain(
    b: &mut Builder,
    bindings: &mut Vec<Binding>,
    node: &Node,
    parameter: Parameter,
    source: u16,
) -> Result<u16, String> {
    let ramp = node.ramps[&parameter];
    let address = b.state(Value::Ramp(ramp))?;
    bindings.push(Binding {
        node: node.id,
        parameter,
        address,
    });
    let destination = b.buffer()?;
    b.instructions.push(Instruction::ProcessGain {
        ramp: address,
        source,
        destination,
    });
    Ok(destination)
}
fn arithmetic(b: &mut Builder, left: u16, right: u16, mode: ArithmeticMode) -> Result<u16, String> {
    let destination = b.buffer()?;
    b.instructions.push(Instruction::ProcessArithmetic {
        left,
        right,
        destination,
        mode,
    });
    Ok(destination)
}
fn parameter(
    b: &mut Builder,
    bindings: &mut Vec<Binding>,
    node: &Node,
    p: Parameter,
    one: u16,
) -> Result<u16, String> {
    gain(b, bindings, node, p, one)
}
fn balanced(
    b: &mut Builder,
    plain: u16,
    balance: Option<u16>,
    channel: usize,
) -> Result<u16, String> {
    if let Some(balance) = balance {
        if channel < 2 {
            return arithmetic(
                b,
                plain,
                balance,
                if channel == 0 {
                    ArithmeticMode::LeftBalance
                } else {
                    ArithmeticMode::RightBalance
                },
            );
        }
    }
    Ok(plain)
}
fn constant(b: &mut Builder, source: Option<u16>) -> Result<u16, String> {
    match source {
        Some(source) => Ok(source),
        None => b.silence(),
    }
}

pub fn lower(
    plan: &PlanDescriptor,
    nodes: &[Node],
    topology: &shoop_loops::PreparedTopology,
    smoothing_frames: u32,
) -> Result<Compiled, String> {
    if plan
        .connections
        .iter()
        .any(|edge| edge.source.kind == PortType::Midi)
    {
        return Err("active MIDI routing is unsupported".into());
    }
    let mut b = Builder {
        instructions: vec![],
        memory: vec![],
        buffers: 0,
        zero: None,
    };
    let mut inputs = vec![];
    for port in 0..2 {
        let destination = b.buffer()?;
        b.instructions
            .push(Instruction::ProcessInput { port, destination });
        inputs.push(destination);
    }
    let one = b.buffer()?;
    b.instructions.push(Instruction::ProcessConstant {
        value: ONE,
        destination: one,
    });
    let mut outputs = BTreeMap::new();
    let mut bindings = vec![];
    let mut meters = vec![];
    let mut smoothing = vec![];
    let mut at = 0;
    for step in &plan.steps {
        let node = nodes
            .iter()
            .find(|node| node.id == step.node)
            .ok_or("unknown prepared engine node")?;
        let counts = rt_runtime::processor::Processor::<
            shoop_loops::runtime::LoopPlanner,
            shoop_rt_engine::EngineOp,
        >::ports(&node.processor);
        let mut source = vec![None; counts.audio_inputs];
        for edge in &plan.connections[at..step.input_end] {
            if edge.source.kind == PortType::Midi {
                continue;
            }
            let buffer = match edge.source.node {
                Some(id) => *outputs
                    .get(&(id, edge.source.port))
                    .ok_or("unavailable signal output")?,
                None => *inputs
                    .get(edge.source.port as usize)
                    .ok_or("invalid virtual input")?,
            };
            let sum = source
                .get_mut(edge.destination.port as usize)
                .ok_or("invalid engine input port")?;
            *sum = Some(b.add(*sum, buffer)?);
        }
        at = step.input_end;
        let mut result = vec![];
        let mut meter = vec![];
        match node.kind {
            SignalKind::Playback {
                slot,
                channels,
                auxiliary,
            } => {
                let target = topology
                    .nodes()
                    .iter()
                    .find(|spec| spec.id.slot() == slot)
                    .ok_or("missing logical playback owner")?
                    .id;
                let level = parameter(&mut b, &mut bindings, node, Parameter::Gain, one)?;
                let balance = if node.outputs == 2 {
                    Some(parameter(
                        &mut b,
                        &mut bindings,
                        node,
                        Parameter::Balance,
                        one,
                    )?)
                } else {
                    None
                };
                for channel in 0..channels {
                    let view = b.state(Value::Word(channel as u64))?;
                    let destination = b.buffer()?;
                    b.instructions.push(Instruction::ProcessLogicalPlayback {
                        loop_id: Identity {
                            slot: target.slot(),
                            generation: target.generation(),
                        },
                        view,
                        destination,
                    });
                    let mut source =
                        arithmetic(&mut b, destination, level, ArithmeticMode::Multiply)?;
                    if let Some(balance) = balance {
                        if channel < 2 {
                            source = arithmetic(
                                &mut b,
                                source,
                                balance,
                                if channel == 0 {
                                    ArithmeticMode::LeftBalance
                                } else {
                                    ArithmeticMode::RightBalance
                                },
                            )?;
                        }
                    }
                    let destination = b.buffer()?;
                    let base = u64::from(destination) | ((slot as u64 * 2 + channel as u64) << 16);
                    let control =
                        b.state(Value::Word(base | (u64::from(smoothing_frames) << 32)))?;
                    smoothing.push((control, base));
                    b.instructions.push(Instruction::ProcessSmoothing {
                        loop_id: Identity {
                            slot: target.slot(),
                            generation: target.generation(),
                        },
                        control,
                        source,
                    });
                    result.push(destination);
                }
                for _ in 0..auxiliary {
                    result.push(b.silence()?);
                }
            }
            SignalKind::Capture { slot, channels, .. } => {
                let target = topology
                    .nodes()
                    .iter()
                    .find(|spec| spec.id.slot() == slot)
                    .ok_or("missing logical capture owner")?
                    .id;
                for channel in 0..channels {
                    let view = b.state(Value::Word(channel as u64))?;
                    let source = constant(&mut b, source[channel])?;
                    b.instructions.push(Instruction::ProcessLogicalCapture {
                        loop_id: Identity {
                            slot: target.slot(),
                            generation: target.generation(),
                        },
                        view,
                        source,
                    });
                }
            }
            SignalKind::Reader => {}
            SignalKind::Fader { channels, .. } => {
                for channel in 0..2 * channels {
                    let source = constant(&mut b, source[channel])?;
                    let destination =
                        gain(&mut b, &mut bindings, node, Parameter::Presence, source)?;
                    if channel < channels {
                        meter.push(destination);
                    }
                    result.push(destination);
                }
            }
            SignalKind::StripInput { channels, .. } => {
                let bank = parameter(&mut b, &mut bindings, node, Parameter::Bank, one)?;
                let inverse = arithmetic(&mut b, bank, bank, ArithmeticMode::OneMinus)?;
                let level = parameter(&mut b, &mut bindings, node, Parameter::Gain, one)?;
                let balance = if node.inputs == 2 {
                    Some(parameter(
                        &mut b,
                        &mut bindings,
                        node,
                        Parameter::Balance,
                        one,
                    )?)
                } else {
                    None
                };
                let monitor = parameter(&mut b, &mut bindings, node, Parameter::Monitor, one)?;
                let mut captured = vec![];
                let mut monitored = vec![];
                for channel in 0..channels {
                    let first = constant(&mut b, source[channel])?;
                    let second = constant(&mut b, source[channels + channel])?;
                    let first = arithmetic(&mut b, first, inverse, ArithmeticMode::Multiply)?;
                    let second = arithmetic(&mut b, second, bank, ArithmeticMode::Multiply)?;
                    let mixed = b.add(Some(first), second)?;
                    let coefficient = balanced(&mut b, level, balance, channel)?;
                    let adjusted =
                        arithmetic(&mut b, mixed, coefficient, ArithmeticMode::Multiply)?;
                    meter.push(adjusted);
                    captured.push(adjusted);
                    monitored.push(arithmetic(
                        &mut b,
                        adjusted,
                        monitor,
                        ArithmeticMode::Multiply,
                    )?);
                }
                result.extend(captured);
                result.extend(monitored);
                while result.len() < counts.audio_outputs {
                    result.push(b.silence()?);
                }
            }
            SignalKind::StripOutput { channels, .. } | SignalKind::Bus { channels, .. } => {
                let is_bus = matches!(node.kind, SignalKind::Bus { .. });
                let bank = parameter(&mut b, &mut bindings, node, Parameter::Bank, one)?;
                let inverse = arithmetic(&mut b, bank, bank, ArithmeticMode::OneMinus)?;
                let level = parameter(&mut b, &mut bindings, node, Parameter::Gain, one)?;
                let scale = parameter(
                    &mut b,
                    &mut bindings,
                    node,
                    if is_bus {
                        Parameter::Mute
                    } else {
                        Parameter::Presence
                    },
                    one,
                )?;
                let level = arithmetic(&mut b, level, scale, ArithmeticMode::Multiply)?;
                let balance = if node.outputs == 2 {
                    Some(parameter(
                        &mut b,
                        &mut bindings,
                        node,
                        Parameter::Balance,
                        one,
                    )?)
                } else {
                    None
                };
                let after = parameter(
                    &mut b,
                    &mut bindings,
                    node,
                    if is_bus {
                        Parameter::Presence
                    } else {
                        Parameter::Mute
                    },
                    one,
                )?;
                let mut first = vec![];
                let mut second = vec![];
                for channel in 0..channels {
                    let source = constant(&mut b, source[channel])?;
                    let coefficient = balanced(&mut b, level, balance, channel)?;
                    let adjusted =
                        arithmetic(&mut b, source, coefficient, ArithmeticMode::Multiply)?;
                    let adjusted = arithmetic(&mut b, adjusted, after, ArithmeticMode::Multiply)?;
                    meter.push(adjusted);
                    first.push(arithmetic(
                        &mut b,
                        adjusted,
                        inverse,
                        ArithmeticMode::Multiply,
                    )?);
                    second.push(arithmetic(
                        &mut b,
                        adjusted,
                        bank,
                        ArithmeticMode::Multiply,
                    )?);
                }
                result.extend(first);
                result.extend(second);
            }
            SignalKind::BusInput { channels, .. } => {
                for channel in 0..channels {
                    result.push(constant(&mut b, source[channel])?);
                }
            }
        }
        if !meter.is_empty() {
            meters.push((node.kind, meter));
        }
        for (port, buffer) in result.into_iter().enumerate() {
            outputs.insert((node.id, port as u32), buffer);
        }
    }
    let mut sums = [None; 2];
    for edge in &plan.connections[at..] {
        if edge.source.kind == PortType::Midi {
            continue;
        }
        let source = match edge.source.node {
            Some(id) => *outputs
                .get(&(id, edge.source.port))
                .ok_or("missing engine output")?,
            None => inputs[edge.source.port as usize],
        };
        let sum = sums
            .get_mut(edge.destination.port as usize)
            .ok_or("invalid virtual output")?;
        *sum = Some(b.add(*sum, source)?);
    }
    for (port, sum) in sums.into_iter().enumerate() {
        let source = constant(&mut b, sum)?;
        b.instructions.push(Instruction::ProcessOutput {
            source,
            port: port as u16,
        });
    }
    let jobs = b
        .instructions
        .iter()
        .map(|ins| {
            if matches!(ins, Instruction::ProcessLogicalCapture { .. }) {
                2
            } else {
                1
            }
        })
        .sum();
    let mut transition =
        compiler::transition_image(topology, 32, |_| false).map_err(|e| format!("{e:?}"))?;
    transition.limits.events = transition
        .limits
        .events
        .max(2 * transition.limits.loop_slots + transition.limits.pending_requests);
    let image = Image {
        format_version: 1,
        name: "application-sync-loop".into(),
        transition,
        program: Program {
            instructions: b.instructions,
            quantum: 32,
            chunk_frames: 32,
            jobs_per_quantum: jobs,
        },
        initial_state: Memory { memory: b.memory },
        buffers: b.buffers,
        input_ports: 2,
        output_ports: 2,
        media: MediaImage {
            chunk_frames: 32,
            samples: vec![],
            free_chunks: vec![],
            views: (0..2)
                .map(|_| View {
                    chunks: vec![],
                    frames: 0,
                })
                .collect(),
        },
        loop_views: topology
            .nodes()
            .iter()
            .map(|spec| LoopViews {
                id: Identity {
                    slot: spec.id.slot(),
                    generation: spec.id.generation(),
                },
                views: vec![0, 1],
            })
            .collect(),
        gains: vec![],
        node_order: plan
            .steps
            .iter()
            .map(|step| Identity {
                slot: step.node.slot(),
                generation: step.node.generation(),
            })
            .collect(),
    };
    if image.buffers > 128
        || image.program.instructions.len() > 256
        || image.initial_state.memory.len() > 256
    {
        return Err("engine image exceeds device profile".into());
    }
    Ok(Compiled {
        image,
        bindings,
        meters,
        smoothing,
    })
}
