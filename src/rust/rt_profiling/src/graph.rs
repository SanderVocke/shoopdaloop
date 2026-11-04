use alloc::{collections::BTreeSet, vec::Vec};
use serde::{Deserialize, Serialize};

pub const MAX_GRAPH_NODES: usize = 4096;
pub const MAX_GRAPH_EDGES: usize = 16384;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub key: u64,
    /// 0: ungrouped, 1: strip, 2: loop, 3: bus.
    pub group: u8,
    pub slot: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Port {
    pub node: Option<u64>,
    /// 0: audio, 1: MIDI.
    pub kind: u8,
    pub channel: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: Port,
    pub destination: Port,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GraphRecord {
    Begin {
        nodes: u32,
        edges: u32,
        sample_rate: u32,
        quantum: u32,
    },
    Node {
        index: u32,
        node: GraphNode,
    },
    Edge {
        index: u32,
        edge: GraphEdge,
    },
    End,
    Unavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Topology {
    pub generation: u64,
    pub epoch: u64,
    pub sample_rate: u32,
    pub quantum: u32,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl Topology {
    pub fn valid(&self) -> bool {
        if self.nodes.len() > MAX_GRAPH_NODES || self.edges.len() > MAX_GRAPH_EDGES {
            return false;
        }
        let keys: BTreeSet<_> = self.nodes.iter().map(|n| n.key).collect();
        keys.len() == self.nodes.len()
            && self
                .nodes
                .iter()
                .all(|n| n.key < crate::MAX_SAMPLES as u64 && n.group <= 3)
            && self.edges.iter().all(|e| {
                [e.source, e.destination].iter().all(|p| {
                    p.kind <= 1
                        && p.channel <= u16::MAX as u32
                        && p.node.is_none_or(|n| keys.contains(&n))
                })
            })
    }
    pub fn budget_ns(&self) -> Option<f64> {
        (self.sample_rate != 0 && self.quantum != 0)
            .then(|| f64::from(self.quantum) * 1e9 / f64::from(self.sample_rate))
    }
}

#[derive(Default)]
pub(crate) struct GraphAssembly {
    pub topology: Option<Topology>,
    nodes: usize,
    edges: usize,
}

impl GraphAssembly {
    pub fn accept(&mut self, generation: u64, epoch: u64, record: GraphRecord) -> Option<Topology> {
        if let GraphRecord::Begin {
            nodes,
            edges,
            sample_rate,
            quantum,
        } = record
        {
            *self = Self::default();
            if nodes as usize > MAX_GRAPH_NODES || edges as usize > MAX_GRAPH_EDGES {
                return None;
            }
            self.nodes = nodes as usize;
            self.edges = edges as usize;
            self.topology = Some(Topology {
                generation,
                epoch,
                sample_rate,
                quantum,
                ..Topology::default()
            });
            return None;
        }
        let t = self.topology.as_mut()?;
        if t.generation != generation || t.epoch != epoch {
            self.topology = None;
            return None;
        }
        match record {
            GraphRecord::Node { index, node }
                if index as usize == t.nodes.len()
                    && t.nodes.len() < self.nodes
                    && t.edges.is_empty() =>
            {
                t.nodes.push(node)
            }
            GraphRecord::Edge { index, edge }
                if index as usize == t.edges.len()
                    && t.edges.len() < self.edges
                    && t.nodes.len() == self.nodes =>
            {
                t.edges.push(edge)
            }
            GraphRecord::End if t.nodes.len() == self.nodes && t.edges.len() == self.edges => {
                return self.topology.take().filter(Topology::valid);
            }
            _ => self.topology = None,
        }
        None
    }
}
