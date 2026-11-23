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
    compile_with_chunk_frames(graph, context, 1)
}

pub fn compile_with_chunk_frames(
    graph: &impl GraphView,
    context: CompileContext,
    chunk_frames: u32,
) -> Result<LogicalStateProgram, CompileError> {
    lower(graph, context, chunk_frames, false)
}

/// Compile prepared regular and explicit composite programs with their audio graph.
pub fn compile_composites(
    graph: &impl GraphView,
    context: CompileContext,
    chunk_frames: u32,
) -> Result<LogicalStateProgram, CompileError> {
    lower(graph, context, chunk_frames, true)
}

fn lower(
    graph: &impl GraphView,
    context: CompileContext,
    chunk_frames: u32,
    composites: bool,
) -> Result<LogicalStateProgram, CompileError> {
    if !chunk_frames.is_power_of_two() || context.quantum % chunk_frames != 0 {
        return Err(CompileError::InvalidProgram);
    }
    let mut graph = Graph::capture(graph);
    validate::graph(&graph, context, composites)?;
    graph.strips.sort_by_key(|s| s.id);
    graph.loops.sort_by_key(|l| l.id);
    graph.buses.sort_by_key(|b| b.id);
    graph.ports.sort_by_key(|p| p.id);
    graph.connections.sort_by_key(|e| (e.destination, e.source));
    plan::lower(&graph, context, chunk_frames)
}
