//! Graph knowledge ends here. Output consists of instructions, initial state and bindings.

use alloc::{vec, vec::Vec};

use crate::{
    fixed::{Fixed, Ramp},
    jobs::BufferId,
    state::{Address, Instruction, Value},
    storage::ViewId,
    transitions::LoopId,
    Error,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeId(
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub usize,
);

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Node {
    LogicalPlayback {
        loop_id: LoopId,
        view: ViewId,
    },
    LogicalCapture {
        loop_id: LoopId,
        input: NodeId,
        view: ViewId,
    },
    Input {
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
    },
    Playback {
        view: ViewId,
        #[cfg_attr(feature = "serde", serde(rename = "length_chunks"))]
        length: u64,
        #[cfg_attr(feature = "serde", serde(rename = "position_chunks"))]
        position: u64,
        playing: bool,
    },
    Gain {
        input: NodeId,
        gain: Fixed,
    },
    Mix {
        left: NodeId,
        right: NodeId,
    },
    Capture {
        input: NodeId,
        view: ViewId,
        recording: bool,
    },
    Output {
        input: NodeId,
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Graph {
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub quantum: usize,
    pub chunk_frames: u32,
    pub nodes: Vec<Node>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Binding {
    LoopView {
        loop_id: LoopId,
        view: Address,
    },
    None,
    Playback {
        position: Address,
        length: Address,
        enabled: Address,
    },
    Gain {
        ramp: Address,
    },
    Capture {
        enabled: Address,
    },
}

pub use crate::state::Program;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Compiled {
    pub program: Program,
    pub initial_state: Vec<Value>,
    pub bindings: Vec<Binding>,
}

fn inputs(node: &Node) -> Vec<NodeId> {
    match *node {
        Node::Gain { input, .. }
        | Node::Capture { input, .. }
        | Node::LogicalCapture { input, .. }
        | Node::Output { input, .. } => {
            vec![input]
        }
        Node::Mix { left, right } => vec![left, right],
        _ => Vec::new(),
    }
}

pub fn compile(graph: &Graph) -> Result<Compiled, Error> {
    let quantum = u32::try_from(graph.quantum).map_err(|_| Error::InvalidShape)?;
    if quantum == 0 || graph.chunk_frames == 0 || quantum % graph.chunk_frames != 0 {
        return Err(Error::InvalidShape);
    }
    let mut result = Compiled {
        program: Program {
            instructions: Vec::new(),
            quantum,
            chunk_frames: graph.chunk_frames,
            jobs_per_quantum: 0,
        },
        initial_state: Vec::new(),
        bindings: vec![Binding::None; graph.nodes.len()],
    };
    let mut done = vec![false; graph.nodes.len()];
    let mut outputs = Vec::new();
    for _ in 0..graph.nodes.len() {
        let next = graph.nodes.iter().enumerate().find(|(index, node)| {
            !done[*index]
                && inputs(node)
                    .iter()
                    .all(|source| done.get(source.0) == Some(&true))
        });
        let Some((index, node)) = next else {
            return Err(Error::InvalidGraph);
        };
        for source in inputs(node) {
            if matches!(
                graph.nodes[source.0],
                Node::Capture { .. } | Node::LogicalCapture { .. } | Node::Output { .. }
            ) {
                return Err(Error::InvalidGraph);
            }
        }
        if let Node::Output { port, .. } = *node {
            if outputs.contains(&port) {
                return Err(Error::InvalidGraph);
            }
            outputs.push(port);
        }
        lower(index, node, &mut result)?;
        done[index] = true;
    }
    Ok(result)
}

fn lower(index: usize, node: &Node, compiled: &mut Compiled) -> Result<(), Error> {
    let destination = BufferId(index);
    let mut state = |value| {
        let address = Address(compiled.initial_state.len());
        compiled.initial_state.push(value);
        address
    };
    let instruction = match *node {
        Node::LogicalPlayback { loop_id, view } => {
            if loop_id.generation == 0 {
                return Err(Error::InvalidGraph);
            }
            let view = state(Value::Word(view.0 as u64));
            compiled.bindings[index] = Binding::LoopView { loop_id, view };
            Instruction::ProcessLogicalPlayback {
                loop_id,
                view,
                destination,
            }
        }
        Node::LogicalCapture {
            loop_id,
            input,
            view,
        } => {
            if loop_id.generation == 0 {
                return Err(Error::InvalidGraph);
            }
            let view = state(Value::Word(view.0 as u64));
            compiled.bindings[index] = Binding::LoopView { loop_id, view };
            Instruction::ProcessLogicalCapture {
                loop_id,
                view,
                source: BufferId(input.0),
            }
        }
        Node::Input { port } => Instruction::ProcessInput { port, destination },
        Node::Playback {
            view,
            length,
            position,
            playing,
        } => {
            if length == 0 || position >= length {
                return Err(Error::InvalidGraph);
            }
            let view = state(Value::Word(view.0 as u64));
            let position = state(Value::Word(position));
            let length = state(Value::Word(length));
            let enabled = state(Value::Word(u64::from(playing)));
            compiled.bindings[index] = Binding::Playback {
                position,
                length,
                enabled,
            };
            Instruction::ProcessLoopPlayback {
                view,
                position,
                length,
                enabled,
                destination,
            }
        }
        Node::Gain { input, gain } => {
            let ramp = state(Value::Ramp(Ramp::constant(gain)));
            compiled.bindings[index] = Binding::Gain { ramp };
            Instruction::ProcessGain {
                ramp,
                source: BufferId(input.0),
                destination,
            }
        }
        Node::Mix { left, right } => Instruction::ProcessMix {
            left: BufferId(left.0),
            right: BufferId(right.0),
            destination,
        },
        Node::Capture {
            input,
            view,
            recording,
        } => {
            let view = state(Value::Word(view.0 as u64));
            let enabled = state(Value::Word(u64::from(recording)));
            compiled.bindings[index] = Binding::Capture { enabled };
            Instruction::ProcessLoopCapture {
                view,
                enabled,
                source: BufferId(input.0),
            }
        }
        Node::Output { input, port } => Instruction::ProcessOutput {
            source: BufferId(input.0),
            port,
        },
    };
    if matches!(instruction, Instruction::ProcessLogicalCapture { .. }) {
        compiled.program.jobs_per_quantum += 1;
    }
    compiled.program.instructions.push(instruction);
    compiled.program.jobs_per_quantum += 1;
    Ok(())
}
