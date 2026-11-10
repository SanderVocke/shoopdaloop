//! Graph knowledge ends here. Output consists of instructions, initial state and bindings.

use alloc::{vec, vec::Vec};

use crate::{
    fixed::{Fixed, Ramp},
    jobs::{BufferId, Job},
    state::{Address, Instruction, JobTemplate, Register, Value},
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
    let r = [Register(0), Register(1), Register(2), Register(3)];
    let code = &mut compiled.program.instructions;
    match *node {
        Node::Input { port } => code.push(Instruction::Emit(JobTemplate::Literal(Job::Input {
            port,
            destination,
        }))),
        Node::Playback {
            view,
            length,
            position,
            playing,
        } => {
            if length == 0 || position >= length {
                return Err(Error::InvalidGraph);
            }
            let addresses = [
                state(Value::Word(view.0 as u64)),
                state(Value::Word(position)),
                state(Value::Word(length)),
                state(Value::Word(u64::from(playing))),
            ];
            for (&address, &destination) in addresses.iter().zip(&r) {
                code.push(Instruction::Load {
                    address,
                    destination,
                });
            }
            code.push(Instruction::Emit(JobTemplate::ReadView {
                view: r[0],
                position: r[1],
                length: r[2],
                enabled: r[3],
                destination,
            }));
            code.push(Instruction::AdvancePosition {
                position: r[1],
                length: r[2],
                enabled: r[3],
            });
            code.push(Instruction::Store {
                source: r[1],
                address: addresses[1],
            });
            compiled.bindings[index] = Binding::Playback {
                position: addresses[1],
                length: addresses[2],
                enabled: addresses[3],
            };
        }
        Node::Gain { input, gain } => {
            let address = state(Value::Ramp(Ramp::constant(gain)));
            code.push(Instruction::Load {
                address,
                destination: r[0],
            });
            code.push(Instruction::Emit(JobTemplate::Gain {
                ramp: r[0],
                source: BufferId(input.0),
                destination,
            }));
            code.push(Instruction::AdvanceRamp { ramp: r[0] });
            code.push(Instruction::Store {
                source: r[0],
                address,
            });
            compiled.bindings[index] = Binding::Gain { ramp: address };
        }
        Node::Mix { left, right } => code.push(Instruction::Emit(JobTemplate::Literal(Job::Mix {
            left: BufferId(left.0),
            right: BufferId(right.0),
            destination,
        }))),
        Node::Capture {
            input,
            view,
            recording,
        } => {
            let view_address = state(Value::Word(view.0 as u64));
            let enabled = state(Value::Word(u64::from(recording)));
            code.push(Instruction::Load {
                address: view_address,
                destination: r[0],
            });
            code.push(Instruction::Load {
                address: enabled,
                destination: r[1],
            });
            code.push(Instruction::Emit(JobTemplate::Capture {
                view: r[0],
                enabled: r[1],
                source: BufferId(input.0),
            }));
            compiled.bindings[index] = Binding::Capture { enabled };
        }
        Node::Output { input, port } => {
            code.push(Instruction::Emit(JobTemplate::Literal(Job::Output {
                source: BufferId(input.0),
                port,
            })))
        }
    }
    compiled.program.jobs_per_quantum += 1;
    Ok(())
}
