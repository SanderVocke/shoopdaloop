//! Audio payloads carried by an opaque diagnostic stream.
use profiling_protocol::{Action, Bytes, Chunk, Name, Request, StreamId};
use serde::{Deserialize, Serialize};

pub const SOURCE: u32 = 1;
pub const RAW_TYPE: u32 = 1;
pub const VIEW_TYPE: u32 = 2;
pub const SCHEMA: u16 = 1;
pub fn stream(epoch: u64, report_type: u32) -> StreamId {
    StreamId {
        source: SOURCE,
        epoch,
        report_type,
        schema: SCHEMA,
    }
}
pub fn subscribe(enabled: bool) -> Request {
    Request {
        source: SOURCE,
        epoch: 1,
        action: Action::Subscribe {
            enabled,
            interval_ms: 200,
        },
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Record {
    Status(crate::Status),
    Graph {
        generation: u64,
        epoch: u64,
        record: crate::GraphRecord,
    },
    Packet(crate::Packet),
    Node {
        generation: u64,
        key: u64,
        name: Name,
    },
}
impl Record {
    pub fn valid(&self) -> bool {
        match self {
            Self::Packet(p) => p.valid(),
            Self::Node { key, name, .. } => *key < crate::MAX_SAMPLES as u64 && name.valid(),
            Self::Graph { record, .. } => match record {
                crate::GraphRecord::Begin { nodes, edges, .. } => {
                    *nodes as usize <= crate::MAX_GRAPH_NODES
                        && *edges as usize <= crate::MAX_GRAPH_EDGES
                }
                crate::GraphRecord::Node { index, node } => {
                    (*index as usize) < crate::MAX_GRAPH_NODES
                        && node.key < crate::MAX_SAMPLES as u64
                        && node.group <= 3
                        && (node.group == 0 || node.owner_generation != 0)
                }
                crate::GraphRecord::Edge { index, edge } => {
                    (*index as usize) < crate::MAX_GRAPH_EDGES
                        && [edge.source, edge.destination].iter().all(|p| {
                            p.kind <= 1
                                && p.channel <= u16::MAX as u32
                                && p.node.is_none_or(|key| key < crate::MAX_SAMPLES as u64)
                        })
                }
                _ => true,
            },
            Self::Status(_) => true,
        }
    }
    pub fn encode(self, sequence: u64) -> profiling_protocol::Record {
        let mut bytes = [0; profiling_protocol::CHUNK_BYTES];
        let len = profiling_protocol::encode(&self, &mut bytes).expect("bounded audio record");
        profiling_protocol::Record::Data(Chunk {
            stream: stream(1, RAW_TYPE),
            sequence,
            offset: 0,
            total: len as u32,
            payload: Bytes::new(&bytes[..len]).expect("bounded audio payload"),
        })
    }
    pub fn decode(record: profiling_protocol::Record) -> Option<Self> {
        let profiling_protocol::Record::Data(chunk) = record else {
            return None;
        };
        if chunk.stream != stream(1, RAW_TYPE)
            || chunk.offset != 0
            || chunk.total as usize != chunk.payload.as_slice().len()
            || !chunk.valid()
        {
            return None;
        }
        let decoded: Self = profiling_protocol::decode(chunk.payload.as_slice()).ok()?;
        decoded.valid().then_some(decoded)
    }
    pub fn collect(self, collector: &mut crate::Collector) {
        match self {
            Self::Status(status) => collector.status(status),
            Self::Packet(packet) => collector.receive(packet),
            Self::Graph {
                generation,
                epoch,
                record,
            } => collector.graph(generation, epoch, record),
            Self::Node {
                generation,
                key,
                name,
            } => collector.name(generation, key, name.as_str()),
        }
    }
}
#[derive(Default)]
pub struct Encoder {
    sequence: u64,
}
impl Encoder {
    pub fn exhausted(&self) -> bool {
        self.sequence == u64::MAX
    }
    pub fn encode(&mut self, record: Record) -> profiling_protocol::Record {
        let Some(sequence) = self.sequence.checked_add(1) else {
            return profiling_protocol::Record::Status(profiling_protocol::Status {
                stream: stream(1, RAW_TYPE),
                requested: false,
                active: false,
                interval_ms: 200,
                outcome: profiling_protocol::Outcome::Capacity,
                dropped: 1,
                coalesced: 0,
            });
        };
        self.sequence = sequence;
        record.encode(sequence)
    }
}
pub fn inspect(request: crate::CaptureRequest) -> Request {
    let mut bytes = [0; profiling_protocol::MAX_CONTROL_BYTES];
    let len = profiling_protocol::encode(&request, &mut bytes).expect("bounded capture request");
    Request {
        source: SOURCE,
        epoch: 1,
        action: Action::Inspect(Bytes::new(&bytes[..len]).expect("bounded request")),
    }
}
