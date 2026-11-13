//! Static lowering in the exact production connection and node order.
use crate::image::*;
use rt_runtime::graph::PlanDescriptor;
use std::collections::BTreeMap;
type Checked<T> = Result<T, String>;
#[derive(Clone, Copy)]
pub struct NodeDescriptor {
    pub id: Identity,
    pub channels: u16,
    pub operation: NodeKind,
}
pub(crate) struct Builder {
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) memory: Vec<Value>,
    pub(crate) buffers: u32,
    pub(crate) zero: Option<u16>,
}
impl Builder {
    pub(crate) fn buffer(&mut self) -> Checked<u16> {
        let b = u16::try_from(self.buffers).map_err(|_| "too many hardware buffers")?;
        self.buffers += 1;
        Ok(b)
    }
    pub(crate) fn state(&mut self, v: Value) -> Checked<u16> {
        let a = u16::try_from(self.memory.len()).map_err(|_| "too many hardware state words")?;
        self.memory.push(v);
        Ok(a)
    }
    pub(crate) fn silence(&mut self) -> Checked<u16> {
        if let Some(b) = self.zero {
            return Ok(b);
        }
        let b = self.buffer()?;
        self.instructions
            .push(Instruction::ProcessSilence { destination: b });
        self.zero = Some(b);
        Ok(b)
    }
    pub(crate) fn add(&mut self, sum: Option<u16>, source: u16) -> Checked<u16> {
        match sum {
            None => Ok(source),
            Some(left) => {
                let destination = self.buffer()?;
                self.instructions.push(Instruction::ProcessMix {
                    left,
                    right: source,
                    destination,
                });
                Ok(destination)
            }
        }
    }
}
fn identity(id: rt_runtime::NodeId) -> Identity {
    Identity {
        slot: id.slot(),
        generation: id.generation(),
    }
}

pub fn lower(
    ordered: &PlanDescriptor,
    nodes: &[NodeDescriptor],
    mut image: Image,
) -> Checked<Image> {
    let mut b = Builder {
        instructions: Vec::new(),
        memory: Vec::new(),
        buffers: 0,
        zero: None,
    };
    let mut inputs = Vec::new();
    for port in 0..image.input_ports {
        let destination = b.buffer()?;
        b.instructions
            .push(Instruction::ProcessInput { port, destination });
        inputs.push(destination);
    }
    let mut outputs = BTreeMap::new();
    let mut gains = Vec::new();
    let mut node_order = Vec::new();
    let mut at = 0;
    for step in &ordered.steps {
        let id = identity(step.node);
        node_order.push(id);
        let node = nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or("missing node description")?;
        let channels = node.channels;
        if channels == 0 || matches!(node.operation,NodeKind::Gain{channels:c,..} if c!=channels) {
            return Err("invalid node channel count".into());
        }
        let mut sources = vec![None; channels as usize];
        for edge in ordered
            .connections
            .get(at..step.input_end)
            .ok_or("invalid input range")?
        {
            let source = match edge.source.node {
                Some(n) => *outputs
                    .get(&(
                        identity(n),
                        u16::try_from(edge.source.port).map_err(|_| "source port overflow")?,
                    ))
                    .ok_or("missing upstream output")?,
                None => *inputs
                    .get(edge.source.port as usize)
                    .ok_or("invalid host input")?,
            };
            let sum = sources
                .get_mut(edge.destination.port as usize)
                .ok_or("invalid node input")?;
            *sum = Some(b.add(*sum, source)?);
        }
        at = step.input_end;
        let mut ramps = Vec::new();
        for channel in 0..channels {
            match node.operation {
                NodeKind::Playback { target } => {
                    let views = &image
                        .loop_views
                        .iter()
                        .find(|v| v.id == target)
                        .ok_or("missing loop view")?
                        .views;
                    let view = b.state(Value::Word(
                        *views.get(channel as usize).ok_or("missing view channel")? as u64,
                    ))?;
                    let destination = b.buffer()?;
                    b.instructions.push(Instruction::ProcessLogicalPlayback {
                        loop_id: target,
                        view,
                        destination,
                    });
                    outputs.insert((id, channel), destination);
                }
                NodeKind::Capture { target } => {
                    let view = b.state(Value::Word(
                        image
                            .loop_views
                            .iter()
                            .find(|v| v.id == target)
                            .ok_or("missing loop view")?
                            .views
                            .get(channel as usize)
                            .copied()
                            .ok_or("missing view channel")? as u64,
                    ))?;
                    let source = match sources[channel as usize] {
                        Some(s) => s,
                        None => b.silence()?,
                    };
                    b.instructions.push(Instruction::ProcessLogicalCapture {
                        loop_id: target,
                        view,
                        source,
                    });
                }
                NodeKind::Gain { initial, .. } => {
                    let source = match sources[channel as usize] {
                        Some(s) => s,
                        None => b.silence()?,
                    };
                    let ramp = b.state(Value::Ramp(Ramp {
                        start: initial,
                        target: initial,
                        duration: 0,
                        elapsed: 0,
                    }))?;
                    ramps.push(ramp);
                    let destination = b.buffer()?;
                    b.instructions.push(Instruction::ProcessGain {
                        ramp,
                        source,
                        destination,
                    });
                    outputs.insert((id, channel), destination);
                }
            }
        }
        if !ramps.is_empty() {
            gains.push(GainBinding { id, ramps });
        }
    }
    let mut sums = vec![None; image.output_ports as usize];
    for edge in ordered
        .connections
        .get(at..)
        .ok_or("invalid output range")?
    {
        let source = match edge.source.node {
            Some(n) => *outputs
                .get(&(
                    identity(n),
                    u16::try_from(edge.source.port).map_err(|_| "source port overflow")?,
                ))
                .ok_or("missing upstream output")?,
            None => *inputs
                .get(edge.source.port as usize)
                .ok_or("invalid host input")?,
        };
        let sum = sums
            .get_mut(edge.destination.port as usize)
            .ok_or("invalid host output")?;
        *sum = Some(b.add(*sum, source)?);
    }
    for (port, sum) in sums.into_iter().enumerate() {
        let source = match sum {
            Some(s) => s,
            None => b.silence()?,
        };
        b.instructions.push(Instruction::ProcessOutput {
            source,
            port: port as u16,
        });
    }
    let jobs_per_quantum = b
        .instructions
        .iter()
        .map(|i| {
            if matches!(i, Instruction::ProcessLogicalCapture { .. }) {
                2
            } else {
                1
            }
        })
        .sum();
    image.program.instructions = b.instructions;
    image.program.jobs_per_quantum = jobs_per_quantum;
    image.initial_state.memory = b.memory;
    image.buffers = b.buffers;
    image.gains = gains;
    image.node_order = node_order;
    Ok(image)
}

pub fn transition_image(
    topology: &shoop_loops::PreparedTopology,
    quantum: u32,
    repeats: impl Fn(shoop_loops::LoopId) -> bool,
) -> Result<TransitionImage, rt_runtime::Error> {
    use rt_runtime::{Error, Quantum};
    topology
        .validate_quantum(Quantum::new(quantum as usize)?)
        .map_err(|_| Error::InvalidGraph)?;
    let limits = topology.limits();
    Ok(TransitionImage {
        description: Description {
            quantum,
            loops: topology
                .nodes()
                .iter()
                .map(|node| LoopSpec {
                    id: Identity {
                        slot: node.id.slot(),
                        generation: node.id.generation(),
                    },
                    kind: match node.kind {
                        shoop_loops::LoopKind::Primitive => "primitive",
                        shoop_loops::LoopKind::Composite => "composite",
                    }
                    .into(),
                    length: node.length,
                    sync_source: node.sync_source.map(|id| Identity {
                        slot: id.slot(),
                        generation: id.generation(),
                    }),
                    repeat_sync: repeats(node.id),
                    timeline: None,
                })
                .collect(),
        },
        limits: Limits {
            loop_slots: u32::try_from(limits.loop_slots).map_err(|_| Error::Capacity)?,
            pending_requests: u32::try_from(limits.pending_requests)
                .map_err(|_| Error::Capacity)?,
            events: u32::try_from(limits.journal_events).map_err(|_| Error::Capacity)?,
        },
        components: topology
            .dependency_order()
            .iter()
            .map(|id| {
                vec![Identity {
                    slot: id.slot(),
                    generation: id.generation(),
                }]
            })
            .collect(),
    })
}
