//! Complete graph uploads terminate at the ARM-side compiler, never at FPGA logic.
use crate::immediate::{Batch, Command, COMMANDS};
use alloc::{borrow::Cow, vec::Vec};
use shoop_graph::{
    BusView, ChannelLayout, Connection, Graph, GraphView, LoopView, Owner, PortId, PortRole,
    PortView, SignalType, StripControls, StripView, MAX_TRANSFER_BYTES,
};
#[derive(Debug)]
pub enum UploadError {
    Encode,
    Capacity,
}
impl core::fmt::Display for UploadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "graph upload: {self:?}")
    }
}
impl core::error::Error for UploadError {}
pub fn upload(
    view: &impl GraphView,
    quantum: u32,
    revision: u64,
) -> Result<Vec<Batch>, UploadError> {
    upload_image(view, quantum, revision, Command::ActivateGraph)
}

pub fn upload_composite(
    view: &impl GraphView,
    quantum: u32,
    revision: u64,
    edit: u64,
) -> Result<Vec<Batch>, UploadError> {
    upload_image(
        view,
        quantum,
        revision,
        Command::ActivateCompositeGraph(edit),
    )
}

fn upload_image(
    view: &impl GraphView,
    quantum: u32,
    revision: u64,
    activation: Command,
) -> Result<Vec<Batch>, UploadError> {
    let bytes = Graph::capture(view)
        .encode()
        .map_err(|_| UploadError::Encode)?;
    if bytes.is_empty() || bytes.len() > MAX_TRANSFER_BYTES {
        return Err(UploadError::Capacity);
    }
    let mut commands = alloc::vec![Command::BeginGraphImage {
        quantum,
        bytes: bytes.len() as u32
    }];
    for chunk in bytes.chunks(20) {
        let mut bytes = [0; 20];
        bytes[..chunk.len()].copy_from_slice(chunk);
        commands.push(Command::GraphImageData {
            len: chunk.len() as u8,
            bytes,
        });
    }
    commands.push(activation);
    Ok(commands
        .chunks(COMMANDS)
        .map(|c| Batch::new(revision, revision, c).unwrap())
        .collect())
}

/// Expand compact mono loop records into the complete graph vocabulary.
pub fn from_loops(loops: &[crate::immediate::GraphLoop]) -> Graph {
    let mut graph = Graph::default();
    let master = shoop_graph::Id {
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
    for l in loops {
        let id = shoop_graph::Id {
            slot: l.slot,
            generation: l.generation,
        };
        let layout = ChannelLayout::Direct {
            audio: 1,
            midi: false,
        };
        graph.strips.push(StripView {
            id: id,
            name: Cow::Borrowed("Loop"),
            layout,
            controls: StripControls::default(),
            default_dry_playback: false,
            processor: None,
        });
        graph.loops.push(LoopView {
            id: id,
            strip: Some(id),
            layout,
            gain: l.gain,
            balance: 0.,
            boundary_smoothing_frames: 0,
            sync_source: l.source.map(|slot| {
                loops.iter().find(|s| s.slot == slot).map_or(
                    shoop_graph::Id {
                        slot,
                        generation: 1,
                    },
                    |s| shoop_graph::Id {
                        slot: s.slot,
                        generation: s.generation,
                    },
                )
            }),
            repeat_sync: l.repeat_sync,
            composite: None,
        });
        let capture = add_port(Owner::Strip(id), PortRole::Input, 0);
        let playback = add_port(Owner::Strip(id), PortRole::Output, 0);
        if l.input.is_some() {
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
