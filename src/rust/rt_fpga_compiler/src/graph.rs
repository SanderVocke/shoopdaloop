//! Compile logical graph connections into target-independent processor operations.
use crate::*;
use shoop_graph::{Graph, GraphView, Owner};
mod plan;
mod validate;

#[derive(Clone, Copy, Debug)]
pub struct CompileContext {
    pub quantum: u32,
    pub limits: ProgramLimits,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Feature {
    Midi,
    Channels,
    Processor,
    Composite,
    StripControls,
    LoopControls,
    BusControls,
    Buses,
    Routing,
}
fn unsupported(object: Owner, feature: Feature) -> CompileError {
    CompileError::Unsupported { object, feature }
}
pub fn compile(
    graph: &impl GraphView,
    context: CompileContext,
) -> Result<LogicalStateProgram, CompileError> {
    let mut graph = Graph::capture(graph);
    validate::graph(&graph, context)?;
    graph.strips.sort_by_key(|s| s.id);
    graph.loops.sort_by_key(|l| l.id);
    graph.buses.sort_by_key(|b| b.id);
    graph.ports.sort_by_key(|p| p.id);
    graph.connections.sort_by_key(|e| (e.destination, e.source));
    plan::lower(&graph, context)
}
