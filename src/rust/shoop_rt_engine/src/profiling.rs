use core::fmt::{self, Write};
use rt_profiling::wire::Record as AudioRecord;
use rt_runtime::profiling::{Status, CHUNK_SAMPLES, PHASE};
use shoop_rt_protocol::{EngineMessage, Lane, Name};
use shoop_rt_transport::carrier::Outbox;

use crate::{endpoint::ControlEndpoint, pool::Pool};

struct Label {
    bytes: [u8; 64],
    len: usize,
}
impl Write for Label {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl Pool {
    pub(crate) fn profiling_names(&self) -> alloc::boxed::Box<[Name]> {
        let mut names = alloc::vec![Name::new("Node").unwrap(); self.nodes()];
        let mut set = |node: rt_runtime::NodeId, args: fmt::Arguments<'_>| {
            let mut label = Label {
                bytes: [0; 64],
                len: 0,
            };
            let _ = label.write_fmt(args);
            names[node.slot() as usize] =
                Name::new(core::str::from_utf8(&label.bytes[..label.len]).unwrap_or("Node"))
                    .expect("bounded node label");
        };
        set(self.reader(), format_args!("Media export reader"));
        for slot in 0..self.strips() {
            set(
                self.strip_input(slot),
                format_args!("Strip {} input", slot + 1),
            );
            set(
                self.strip_output(slot),
                format_args!("Strip {} output", slot + 1),
            );
        }
        for slot in 0..self.loops() {
            set(
                self.playback(slot),
                format_args!("Loop {} playback", slot + 1),
            );
            set(
                self.capture(slot),
                format_args!("Loop {} capture", slot + 1),
            );
            set(self.fader(slot), format_args!("Loop {} fader", slot + 1));
        }
        for slot in 0..self.buses() {
            set(self.bus(slot), format_args!("Bus {} output", slot + 1));
        }
        for slot in 0..self.processors() {
            set(
                self.processor(slot),
                format_args!("Host processor {}", slot + 1),
            );
        }
        names.into_boxed_slice()
    }
}

impl ControlEndpoint {
    pub(crate) fn forward_profiling(&mut self, out: &mut Outbox) {
        if !self.connected {
            return;
        }
        if !self.profile_catalog {
            let catalog = profiling_protocol::Descriptor {
                stream: rt_profiling::wire::stream(1, rt_profiling::wire::RAW_TYPE),
                name: Name::new("audio.graph.raw").expect("static name"),
                available: self.profiling.is_some(),
                min_interval_ms: 200,
                max_interval_ms: 200,
                snapshot: false,
                inspect: false,
            };
            if out
                .push(EngineMessage::Profiling(
                    profiling_protocol::Record::Descriptor(catalog),
                ))
                .is_err()
            {
                return;
            }
            self.profile_catalog = true;
        }
        if self.profile_wire.exhausted() {
            if let Some(reader) = &self.profiling {
                reader.control().set_enabled(false);
            }
        }
        let status = self
            .profiling
            .as_ref()
            .map_or(Status::default(), |reader| reader.control().status());
        if self.profile_status != Some(status) {
            if out.available(Lane::Profiling) < 2 {
                return;
            }
            let generic = profiling_protocol::Status {
                stream: rt_profiling::wire::stream(1, rt_profiling::wire::RAW_TYPE),
                requested: status.requested,
                active: status.active,
                interval_ms: 200,
                outcome: if status.requested && !status.available {
                    profiling_protocol::Outcome::Unavailable
                } else {
                    profiling_protocol::Outcome::Applied
                },
                dropped: status.dropped,
                coalesced: 0,
            };
            if out
                .push(EngineMessage::Profiling(
                    profiling_protocol::Record::Status(generic),
                ))
                .is_err()
            {
                return;
            }
            if out
                .push(EngineMessage::Profiling(
                    self.profile_wire.encode(AudioRecord::Status(status)),
                ))
                .is_err()
            {
                return;
            }
            self.profile_status = Some(status);
        }
        if self.profile_generation != status.generation {
            self.profile_named.fill(false);
            self.profile_generation = status.generation;
            self.profile_layout.rewind();
        }
        if status.requested || status.active {
            self.profile_layout.forward(
                status.generation,
                self.graph_epoch.0,
                &self.profile_names,
                &mut self.profile_named,
                out,
                &mut self.profile_wire,
            );
        }
        let Some(reader) = &mut self.profiling else {
            return;
        };
        for _ in 0..128 {
            if out.available(Lane::Profiling) < CHUNK_SAMPLES + 1 {
                break;
            }
            let Some(packet) = reader.next_packet() else {
                break;
            };
            for sample in &packet.samples[..usize::from(packet.len)] {
                if sample.key & PHASE == 0 {
                    let key = sample.key & !rt_runtime::profiling::LAYOUT;
                    if let Some(named) = self.profile_named.get_mut(key as usize) {
                        if !*named {
                            let _ = out.push(EngineMessage::Profiling(self.profile_wire.encode(
                                AudioRecord::Node {
                                    generation: packet.generation,
                                    key,
                                    name: self.profile_names[key as usize],
                                },
                            )));
                            *named = true;
                        }
                    }
                }
            }
            let _ = out.push(EngineMessage::Profiling(
                self.profile_wire.encode(AudioRecord::Packet(packet)),
            ));
        }
    }
}

struct Layout {
    nodes: alloc::boxed::Box<[rt_runtime::profiling::GraphNode]>,
    edges: alloc::boxed::Box<[rt_runtime::profiling::GraphEdge]>,
    node_count: usize,
    edge_count: usize,
    valid: bool,
}
impl Layout {
    fn new(nodes: usize, edges: usize) -> Self {
        use rt_runtime::profiling::{GraphEdge, GraphNode, MAX_GRAPH_EDGES, MAX_GRAPH_NODES};
        Self {
            nodes: alloc::vec![GraphNode::default(); nodes.min(MAX_GRAPH_NODES)].into_boxed_slice(),
            edges: alloc::vec![GraphEdge::default(); edges.min(MAX_GRAPH_EDGES)].into_boxed_slice(),
            node_count: 0,
            edge_count: 0,
            valid: true,
        }
    }
}

pub(crate) struct Layouts {
    active: Layout,
    staged: Layout,
    groups: alloc::boxed::Box<[rt_runtime::profiling::GraphNode]>,
    pending: bool,
    cursor: usize,
    sample_rate: u32,
    quantum: u32,
}
impl Layouts {
    pub fn new(pool: &Pool, edges: usize, sample_rate: u32, quantum: u32) -> Self {
        use rt_runtime::profiling::GraphNode;
        let mut groups = alloc::vec![GraphNode::default(); pool.nodes()];
        for (key, node) in groups.iter_mut().enumerate() {
            node.key = key as u64;
        }
        for slot in 0..pool.strips() {
            for id in [pool.strip_input(slot), pool.strip_output(slot)] {
                groups[id.slot() as usize].group = 1;
                groups[id.slot() as usize].slot = slot as u32;
            }
        }
        for slot in 0..pool.loops() {
            for id in [pool.capture(slot), pool.playback(slot), pool.fader(slot)] {
                groups[id.slot() as usize].group = 2;
                groups[id.slot() as usize].slot = slot as u32;
            }
        }
        for slot in 0..pool.buses() {
            groups[pool.bus(slot).slot() as usize].group = 3;
            groups[pool.bus(slot).slot() as usize].slot = slot as u32;
        }
        Self {
            active: Layout::new(pool.nodes(), edges),
            staged: Layout::new(pool.nodes(), edges),
            groups: groups.into_boxed_slice(),
            pending: false,
            cursor: 0,
            sample_rate,
            quantum,
        }
    }
    pub fn stage(&mut self, plan: &rt_runtime::graph::PlanDescriptor, model: &crate::model::Model) {
        use rt_runtime::profiling::{GraphEdge, Port};
        let t = &mut self.staged;
        t.valid = plan.steps.len() <= t.nodes.len() && plan.connections.len() <= t.edges.len();
        self.pending = true;
        if !t.valid {
            return;
        }
        t.node_count = plan.steps.len();
        t.edge_count = plan.connections.len();
        for (slot, step) in plan.steps.iter().enumerate() {
            t.nodes[slot] = self.groups[step.node.slot() as usize];
            let node = &mut t.nodes[slot];
            let slot = node.slot as usize;
            node.owner_generation = match node.group {
                1 => model.strips[slot].map_or(0, |s| s.id.0.generation),
                2 => model.loops[slot].map_or(0, |s| s.id.0.generation),
                3 => model.buses[slot].map_or(0, |s| s.id.0.generation),
                _ => 0,
            };
            t.valid &= node.group == 0 || node.owner_generation != 0;
        }
        let port = |p: rt_runtime::graph::Endpoint| Port {
            node: p.node.map(|n| u64::from(n.slot())),
            kind: match p.kind {
                rt_runtime::graph::PortType::Audio => 0,
                rt_runtime::graph::PortType::Midi => 1,
            },
            channel: p.port,
        };
        for (slot, edge) in plan.connections.iter().enumerate() {
            t.edges[slot] = GraphEdge {
                source: port(edge.source),
                destination: port(edge.destination),
            };
        }
    }
    pub fn activate(&mut self) {
        if self.pending {
            core::mem::swap(&mut self.active, &mut self.staged);
            self.pending = false;
            self.rewind();
        }
    }
    pub fn rewind(&mut self) {
        self.cursor = 0;
    }
    fn forward(
        &mut self,
        generation: u64,
        epoch: u64,
        names: &[shoop_rt_protocol::Name],
        named: &mut [bool],
        out: &mut Outbox,
        encoder: &mut rt_profiling::wire::Encoder,
    ) {
        use rt_runtime::profiling::GraphRecord;
        let t = &self.active;
        if !t.valid {
            if self.cursor == 0
                && out
                    .push(EngineMessage::Profiling(encoder.encode(
                        AudioRecord::Graph {
                            generation,
                            epoch,
                            record: GraphRecord::Unavailable,
                        },
                    )))
                    .is_ok()
            {
                self.cursor = 1;
            }
            return;
        }
        for _ in 0..16 {
            let record = match self.cursor {
                0 => GraphRecord::Begin {
                    nodes: t.node_count as u32,
                    edges: t.edge_count as u32,
                    sample_rate: self.sample_rate,
                    quantum: self.quantum,
                },
                n if n <= t.node_count => GraphRecord::Node {
                    index: (n - 1) as u32,
                    node: t.nodes[n - 1],
                },
                n if n <= t.node_count + t.edge_count => GraphRecord::Edge {
                    index: (n - t.node_count - 1) as u32,
                    edge: t.edges[n - t.node_count - 1],
                },
                n if n == t.node_count + t.edge_count + 1 => GraphRecord::End,
                _ => break,
            };
            if let GraphRecord::Node { node, .. } = record {
                let key = node.key as usize;
                if !named[key] {
                    if out.available(Lane::Profiling) < 2 {
                        break;
                    }
                    if out
                        .push(EngineMessage::Profiling(encoder.encode(
                            AudioRecord::Node {
                                generation,
                                key: node.key,
                                name: names[key],
                            },
                        )))
                        .is_err()
                    {
                        break;
                    }
                    named[key] = true;
                }
            }
            if out
                .push(EngineMessage::Profiling(encoder.encode(
                    AudioRecord::Graph {
                        generation,
                        epoch,
                        record,
                    },
                )))
                .is_err()
            {
                break;
            }
            self.cursor += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_runtime::graph::{Connection, Endpoint, NodeStep, PlanDescriptor, PortType};
    use rt_runtime::profiling::{GraphRecord, Port};
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn exported_topology_matches_the_activated_descriptor_and_waits_for_capacity() {
        let pool = Pool::new(
            &[crate::description::Segment {
                strips: 1,
                loops: 1,
            }],
            1,
        );
        let node = pool.strip_input(0);
        let descriptor = PlanDescriptor {
            steps: alloc::vec![NodeStep { node, input_end: 1 }],
            connections: alloc::vec![Connection {
                source: Endpoint::host(PortType::Midi, 2),
                destination: Endpoint::node(node, PortType::Midi, 0)
            }],
        };
        let mut layouts = Layouts::new(&pool, 4, 48000, 32);
        let mut model = crate::model::Model::new(1, 1, 1, 2, Default::default(), 0);
        model.strips[0] = Some(crate::model::Strip {
            default_dry: false,
            processor: None,
            sends: Default::default(),
            returns: Default::default(),
            midi_send: Default::default(),
            id: shoop_rt_protocol::StripId(shoop_rt_protocol::SlotId::new(0, 7)),
            shape: shoop_rt_protocol::StripShape {
                audio_inputs: 0,
                audio_outputs: 0,
                midi_input: true,
                midi_output: true,
            },
            controls: Default::default(),
            inputs: Default::default(),
            outputs: Default::default(),
            midi_inputs: Default::default(),
            midi_outputs: Default::default(),
        });
        layouts.stage(&descriptor, &model);
        assert_eq!(layouts.active.node_count, 0);
        layouts.activate();
        assert_eq!(layouts.active.node_count, descriptor.steps.len());
        assert_eq!(layouts.active.nodes[0].owner_generation, 7);
        assert_eq!(layouts.active.nodes[0].key, u64::from(node.slot()));
        assert_eq!(
            layouts.active.edges[0].source,
            Port {
                node: None,
                kind: 1,
                channel: 2
            }
        );
        assert_eq!(
            layouts.active.edges[0].destination,
            Port {
                node: Some(u64::from(node.slot())),
                kind: 1,
                channel: 0
            }
        );
        let names = pool.profiling_names();
        let mut named = alloc::vec![false; pool.nodes()];
        let mut out = Outbox::new(2, 1, 1);
        let mut encoder = rt_profiling::wire::Encoder::default();
        for _ in 0..256 {
            out.push(EngineMessage::Profiling(encoder.encode(
                AudioRecord::Graph {
                    generation: 1,
                    epoch: 2,
                    record: GraphRecord::End,
                },
            )))
            .unwrap();
        }
        layouts.forward(1, 2, &names, &mut named, &mut out, &mut encoder);
        assert_eq!(layouts.cursor, 0);
        while out.pop().is_some() {}
        layouts.forward(1, 2, &names, &mut named, &mut out, &mut encoder);
        let mut records = alloc::vec::Vec::new();
        while let Some(message) = out.pop() {
            if let EngineMessage::Profiling(record) = message {
                if let Some(record) = AudioRecord::decode(record) {
                    records.push(record);
                }
            }
        }
        assert!(records
            .iter()
            .any(|m| matches!(m, AudioRecord::Node { key, .. } if *key == u64::from(node.slot()))));
        assert!(matches!(
            records.last(),
            Some(AudioRecord::Graph {
                record: GraphRecord::End,
                ..
            })
        ));
    }
}
