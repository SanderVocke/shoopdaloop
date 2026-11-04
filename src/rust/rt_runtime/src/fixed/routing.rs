use super::graph::{Address, Route};
use crate::graph::{self, Catalog, GraphDescription, NodeSpec};
use crate::{Error, NodeId, PreparedConfig};
use alloc::{boxed::Box, sync::Arc, vec::Vec};

struct Layout {
    catalog: Catalog,
    nodes: Box<[NodeId]>,
}

/// Control-owned compiler for one runtime's immutable processor catalog.
#[derive(Clone)]
pub struct GraphCompiler {
    layout: Arc<Layout>,
}

#[derive(Clone, Copy)]
pub(super) struct Step {
    pub slot: usize,
    pub input_end: usize,
}

/// Prepared routing storage; rejected and retired graphs return to control.
pub struct PreparedGraph {
    compiler: GraphCompiler,
    pub(super) description: GraphDescription,
    pub(super) steps: Vec<Step>,
    pub(super) routes: Vec<Route>,
}

impl core::fmt::Debug for PreparedGraph {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PreparedGraph")
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

impl GraphCompiler {
    pub(super) fn node_slot(&self, id: NodeId) -> Result<usize, Error> {
        self.layout
            .nodes
            .iter()
            .position(|&n| n == id)
            .ok_or(Error::InvalidId)
    }
    pub(super) fn new(config: PreparedConfig, nodes: &[NodeSpec]) -> Result<Self, Error> {
        Ok(Self {
            layout: Arc::new(Layout {
                catalog: Catalog::new(config, nodes)?,
                nodes: nodes.iter().map(|n| n.id).collect(),
            }),
        })
    }
    pub(super) fn accepts(&self, graph: &PreparedGraph) -> bool {
        Arc::ptr_eq(&self.layout, &graph.compiler.layout)
    }
    /// Compile and bind all route addresses outside rendering.
    pub fn compile(&self, description: &GraphDescription) -> Result<PreparedGraph, Error> {
        let plan = graph::compile(&self.layout.catalog, description)?;
        let descriptor = plan.descriptor();
        let slot = |id| {
            self.layout
                .nodes
                .iter()
                .position(|&n| n == id)
                .ok_or(Error::InvalidId)
        };
        let steps = descriptor
            .steps
            .iter()
            .map(|s| {
                Ok(Step {
                    slot: slot(s.node)?,
                    input_end: s.input_end,
                })
            })
            .collect::<Result<_, Error>>()?;
        let address = |e: graph::Endpoint| -> Result<Address, Error> {
            Ok(Address {
                node: e.node.map(slot).transpose()?,
                port: e.port as usize,
            })
        };
        let routes = descriptor
            .connections
            .iter()
            .map(|e| {
                Ok(Route {
                    source: address(e.source)?,
                    destination: address(e.destination)?,
                    kind: e.source.kind,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(PreparedGraph {
            compiler: self.clone(),
            description: description.clone(),
            steps,
            routes,
        })
    }
}

impl PreparedGraph {
    pub(super) fn accepts(&self, graph: &Self) -> bool {
        self.compiler.accepts(graph)
    }
    pub fn description(&self) -> &GraphDescription {
        &self.description
    }
    pub fn compiler(&self) -> GraphCompiler {
        self.compiler.clone()
    }
}
