//! Serializable hardware image and static lowering; there is no RTL executor here.
use crate::{production, spec::*, validation::Checked};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub format_version: u32,
    pub name: String,
    pub transition: TransitionImage,
    pub program: Program,
    pub initial_state: Memory,
    pub buffers: u32,
    pub input_ports: u16,
    pub output_ports: u16,
    pub media: MediaImage,
    pub loop_views: Vec<LoopViews>,
    pub gains: Vec<GainBinding>,
    pub node_order: Vec<Identity>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionImage {
    pub description: Description,
    pub limits: Limits,
    pub components: Vec<Vec<Identity>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub quantum: u32,
    pub loops: Vec<LoopSpec>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopSpec {
    pub id: Identity,
    pub kind: String,
    pub length: u64,
    pub sync_source: Option<Identity>,
    pub repeat_sync: bool,
    pub timeline: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub loop_slots: u32,
    pub pending_requests: u32,
    pub events: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub instructions: Vec<Instruction>,
    pub quantum: u32,
    pub chunk_frames: u32,
    pub jobs_per_quantum: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub memory: Vec<Value>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Value {
    Word(u64),
    Ramp(Ramp),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    pub start: i32,
    pub target: i32,
    pub duration: u32,
    pub elapsed: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Instruction {
    ProcessInput {
        port: u16,
        destination: u16,
    },
    ProcessLogicalPlayback {
        loop_id: Identity,
        view: u16,
        destination: u16,
    },
    ProcessLogicalCapture {
        loop_id: Identity,
        view: u16,
        source: u16,
    },
    ProcessGain {
        ramp: u16,
        source: u16,
        destination: u16,
    },
    ProcessMix {
        left: u16,
        right: u16,
        destination: u16,
    },
    ProcessOutput {
        source: u16,
        port: u16,
    },
    ProcessSilence {
        destination: u16,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaImage {
    pub chunk_frames: u32,
    pub samples: Vec<i32>,
    pub free_chunks: Vec<u32>,
    pub views: Vec<View>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub chunks: Vec<u32>,
    pub frames: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopViews {
    pub id: Identity,
    pub views: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GainBinding {
    pub id: Identity,
    pub ramps: Vec<u16>,
}
struct Builder {
    instructions: Vec<Instruction>,
    memory: Vec<Value>,
    buffers: u32,
    zero: Option<u16>,
}
impl Builder {
    fn buffer(&mut self) -> Checked<u16> {
        let b = u16::try_from(self.buffers).map_err(|_| "too many hardware buffers")?;
        self.buffers += 1;
        Ok(b)
    }
    fn state(&mut self, v: Value) -> Checked<u16> {
        let a = u16::try_from(self.memory.len()).map_err(|_| "too many hardware state words")?;
        self.memory.push(v);
        Ok(a)
    }
    fn silence(&mut self) -> Checked<u16> {
        if let Some(b) = self.zero {
            return Ok(b);
        }
        let b = self.buffer()?;
        self.instructions
            .push(Instruction::ProcessSilence { destination: b });
        self.zero = Some(b);
        Ok(b)
    }
    fn add(&mut self, sum: Option<u16>, source: u16) -> Checked<u16> {
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

pub fn compile(scenario: &Scenario) -> Checked<Image> {
    let production::Prepared { ordered, .. } = production::prepare::<rt_runtime::Q824>(scenario)?;
    let slots = scenario
        .loops
        .iter()
        .map(|l| l.id.slot + 1)
        .max()
        .unwrap_or(1);
    let requests = u32::from(scenario.request_capacity);
    let topology = production::topology(scenario, slots as usize, requests as usize)?;
    let components = topology
        .components()
        .iter()
        .map(|c| {
            c.iter()
                .map(|id| Identity {
                    slot: id.slot(),
                    generation: id.generation(),
                })
                .collect()
        })
        .collect();
    let transition = TransitionImage {
        description: Description {
            quantum: scenario.quantum,
            loops: topology
                .nodes()
                .iter()
                .map(|n| {
                    let source = scenario
                        .loops
                        .iter()
                        .find(|l| production::loop_id(l.id).ok() == Some(n.id))
                        .unwrap();
                    LoopSpec {
                        id: source.id,
                        kind: "primitive".into(),
                        length: n.length,
                        sync_source: source.sync_source,
                        repeat_sync: source.repeat_sync,
                        timeline: None,
                    }
                })
                .collect(),
        },
        limits: Limits {
            loop_slots: slots,
            pending_requests: requests,
            events: slots * 2 + requests,
        },
        components,
    };
    let mut media = MediaImage {
        chunk_frames: scenario.chunk_frames,
        samples: Vec::new(),
        free_chunks: Vec::new(),
        views: Vec::new(),
    };
    let mut banks = BTreeMap::new();
    for m in &scenario.media {
        for (channel, samples) in m.channels.iter().enumerate() {
            let first = media.samples.len() / scenario.chunk_frames as usize;
            media.samples.extend(samples);
            banks.insert((m.name.as_str(), channel), first as u32);
        }
    }
    let mut loop_views = Vec::new();
    for l in &scenario.loops {
        let mut views = Vec::new();
        for channel in 0..l.channels as usize {
            views.push(media.views.len() as u32);
            let (chunks, frames) = if let Some(c) = &l.content {
                let first = banks[&(c.media.as_str(), channel)] + c.start_chunks as u32;
                (
                    (0..c.length_chunks).map(|i| first + i as u32).collect(),
                    c.length_chunks * u64::from(scenario.chunk_frames),
                )
            } else {
                (Vec::new(), 0)
            };
            media.views.push(View { chunks, frames });
        }
        loop_views.push(LoopViews { id: l.id, views });
    }
    let referenced: std::collections::BTreeSet<_> = media
        .views
        .iter()
        .flat_map(|v| v.chunks.iter().copied())
        .collect();
    media.free_chunks = (0..media.samples.len() as u32 / scenario.chunk_frames)
        .filter(|id| !referenced.contains(id))
        .collect();
    let mut b = Builder {
        instructions: Vec::new(),
        memory: Vec::new(),
        buffers: 0,
        zero: None,
    };
    let mut inputs = Vec::new();
    for port in 0..scenario.input_ports {
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
        let node = scenario.nodes.iter().find(|n| n.id == id).unwrap();
        let channels = match node.operation {
            NodeKind::Gain { channels, .. } => channels,
            NodeKind::Playback { target } | NodeKind::Capture { target } => {
                scenario
                    .loops
                    .iter()
                    .find(|l| l.id == target)
                    .unwrap()
                    .channels
            }
        };
        let mut sources = vec![None; channels as usize];
        for edge in &ordered.connections[at..step.input_end] {
            let source = match edge.source.node {
                Some(n) => outputs[&(identity(n), edge.source.port as u16)],
                None => inputs[edge.source.port as usize],
            };
            let sum = &mut sources[edge.destination.port as usize];
            *sum = Some(b.add(*sum, source)?);
        }
        at = step.input_end;
        let mut ramps = Vec::new();
        for channel in 0..channels {
            match node.operation {
                NodeKind::Playback { target } => {
                    let views = &loop_views.iter().find(|v| v.id == target).unwrap().views;
                    let view = b.state(Value::Word(views[channel as usize] as u64))?;
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
                        loop_views.iter().find(|v| v.id == target).unwrap().views[channel as usize]
                            as u64,
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
    let mut sums = vec![None; scenario.output_ports as usize];
    for edge in &ordered.connections[at..] {
        let source = match edge.source.node {
            Some(n) => outputs[&(identity(n), edge.source.port as u16)],
            None => inputs[edge.source.port as usize],
        };
        let sum = &mut sums[edge.destination.port as usize];
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
    Ok(Image {
        format_version: FORMAT_VERSION,
        name: scenario.name.clone(),
        transition,
        program: Program {
            instructions: b.instructions,
            quantum: scenario.quantum,
            chunk_frames: scenario.chunk_frames,
            jobs_per_quantum,
        },
        initial_state: Memory { memory: b.memory },
        buffers: b.buffers,
        input_ports: scenario.input_ports,
        output_ports: scenario.output_ports,
        media,
        loop_views,
        gains,
        node_order,
    })
}
