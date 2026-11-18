//! Compile logical graph connections into target-independent processor operations.
use crate::*;
use alloc::borrow::Cow;
pub use shoop_graph;
use shoop_graph::{
    BusView, ChannelLayout, Connection, Graph, GraphView, LoopView, Owner, PortId, PortRole,
    PortView, SignalType, StripControls, StripView,
};
mod plan;
mod validate;

#[derive(Clone, Copy, Debug)]
pub struct LoopDescription {
    pub id: Identity,
    pub source: Option<u32>,
    pub input: bool,
    pub output: bool,
    pub gain: f32,
    pub repeat_sync: bool,
}
pub struct MonoGraphFixture {
    pub quantum: u32,
    pub loops: Vec<LoopDescription>,
}
impl MonoGraphFixture {
    pub fn compile(&self) -> Result<LogicalStateProgram, CompileError> {
        self.compile_with_limits(ProgramLimits::default())
    }
    pub fn compile_with_limits(
        &self,
        limits: ProgramLimits,
    ) -> Result<LogicalStateProgram, CompileError> {
        if self.loops.is_empty() {
            return Err(CompileError::InvalidProgram);
        }
        compile(
            &self.to_graph(),
            CompileContext {
                quantum: self.quantum,
                limits,
            },
        )
    }
}
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
    plan::compile(&graph, context)
}
impl MonoGraphFixture {
    /// Full graph fixtures for component benches, using one independent strip per loop.
    pub fn to_graph(&self) -> Graph {
        let mut graph = Graph::default();
        let master = Identity {
            slot: 0,
            generation: 1,
        };
        graph.buses.push(BusView {
            id: master,
            name: Cow::Borrowed("Master"),
            channels: 2,
            master: true,
            gain_db: 0.,
            balance: 0.,
            muted: false,
            processor: None,
        });
        let mut add_port = |owner, role, index| {
            let id = PortId {
                owner,
                signal: SignalType::Audio,
                role,
                index,
            };
            graph.ports.push(PortView {
                id,
                channel: index,
                name: Cow::Borrowed(""),
            });
            id
        };
        let input = add_port(Owner::Host, PortRole::Input, 0);
        let mut bus_inputs = Vec::new();
        for i in 0..2 {
            let source = add_port(Owner::Bus(master), PortRole::Output, i);
            let destination = add_port(Owner::Host, PortRole::Output, i);
            graph.connections.push(Connection {
                source,
                destination,
            });
            bus_inputs.push(add_port(Owner::Bus(master), PortRole::Input, i));
        }
        for l in &self.loops {
            let layout = ChannelLayout::Direct {
                audio: 1,
                midi: false,
            };
            graph.strips.push(StripView {
                id: l.id,
                name: Cow::Borrowed("fixture"),
                layout,
                controls: StripControls::default(),
                default_dry_playback: false,
                processor: None,
            });
            graph.loops.push(LoopView {
                id: l.id,
                strip: Some(l.id),
                layout,
                gain: l.gain,
                balance: 0.,
                boundary_smoothing_frames: 0,
                sync_source: l.source.map(|slot| {
                    self.loops.iter().find(|s| s.id.slot == slot).map_or(
                        Identity {
                            slot,
                            generation: 1,
                        },
                        |s| s.id,
                    )
                }),
                repeat_sync: l.repeat_sync,
                composite: None,
            });
            let capture = add_port(Owner::Strip(l.id), PortRole::Input, 0);
            let playback = add_port(Owner::Strip(l.id), PortRole::Output, 0);
            if l.input {
                graph.connections.push(Connection {
                    source: input,
                    destination: capture,
                });
            }
            if l.output {
                for &destination in &bus_inputs {
                    graph.connections.push(Connection {
                        source: playback,
                        destination,
                    });
                }
            }
        }
        graph
    }
}
