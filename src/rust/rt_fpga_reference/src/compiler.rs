//! Graph knowledge ends here. Output consists of instructions, initial state and bindings.

use alloc::{vec, vec::Vec};

use crate::{
    fixed::{Fixed, Ramp},
    jobs::BufferId,
    state::{Address, Instruction, Value},
    storage::ViewId,
    Error,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub usize);

#[derive(Clone, Debug)]
pub enum Node {
    Input {
        port: usize,
    },
    Playback {
        view: ViewId,
        length: u64,
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
        port: usize,
    },
}

pub struct Graph {
    pub quantum: usize,
    pub nodes: Vec<Node>,
}

#[derive(Clone, Copy, Debug)]
pub enum Binding {
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

pub struct Compiled {
    pub program: Program,
    pub initial_state: Vec<Value>,
    pub bindings: Vec<Binding>,
}

fn inputs(node: &Node) -> Vec<NodeId> {
    match *node {
        Node::Gain { input, .. } | Node::Capture { input, .. } | Node::Output { input, .. } => {
            vec![input]
        }
        Node::Mix { left, right } => vec![left, right],
        _ => Vec::new(),
    }
}

pub fn compile(graph: &Graph) -> Result<Compiled, Error> {
    let quantum = u32::try_from(graph.quantum).map_err(|_| Error::InvalidShape)?;
    if quantum == 0 {
        return Err(Error::InvalidShape);
    }
    let mut result = Compiled {
        program: Program {
            instructions: Vec::new(),
            quantum,
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
                Node::Capture { .. } | Node::Output { .. }
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
    compiled.program.instructions.push(instruction);
    compiled.program.jobs_per_quantum += 1;
    Ok(())
}
