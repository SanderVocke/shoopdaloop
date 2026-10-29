use rt_runtime::graph::{Connection, Endpoint, NodeStep, PortType};
use rt_runtime::{Error, NodeId};

pub const BYTES: usize = 512;
pub const HEADER: usize = 48;
pub const NORMAL_SLOTS: usize = 4;
pub const SLOTS: usize = 5;
pub const STREAM: u64 = 1;
pub const PLAN_BEGIN: u8 = 1;
pub const PLAN_STEPS: u8 = 2;
pub const PLAN_EDGES: u8 = 3;
pub const PLAN_COMMIT: u8 = 4;
pub const PLAN_CANCEL: u8 = 5;
pub const PARAMETER: u8 = 6;
pub const MIDI: u8 = 7;
pub const EXECUTION: u8 = 8;
pub const STATUS: u8 = 9;
pub const CREATE: u8 = 10;
pub const UPLOAD: u8 = 11;
pub const WRITE: u8 = 12;
pub const FINISH: u8 = 13;
pub const COMMIT: u8 = 14;
pub const CANCEL_UPLOAD: u8 = 15;
pub const SNAPSHOT: u8 = 16;
pub const CURSOR: u8 = 17;
pub const READ: u8 = 18;
pub const CLOSE_CURSOR: u8 = 19;
pub const RELEASE: u8 = 20;
pub const CURRENTNESS: u8 = 21;
pub const INFO: u8 = 22;
pub const CAPTURE: u8 = 23;
pub const STOP_CAPTURE: u8 = 24;
pub const REMOVE_RESOURCE: u8 = 25;
pub const REMOVE_NODE: u8 = 26;
pub const RESET_MIDI: u8 = 27;

pub const APPLIED: u32 = 0;
pub const STAGED: u32 = 1;
pub const SCHEDULED: u32 = 2;
pub const CANCELLED: u32 = 3;
pub const REJECTED: u32 = 4;
pub const EXECUTED: u32 = 5;
pub const PENDING: u32 = 6;

#[derive(Clone, Copy, Debug)]
pub struct Packet {
    pub bytes: [u8; BYTES],
    pub len: usize,
}
impl Default for Packet {
    fn default() -> Self {
        Self {
            bytes: [0; BYTES],
            len: HEADER,
        }
    }
}
impl Packet {
    pub fn new(op: u8, slot: usize, sequence: u64, epoch: u64, frame: u64) -> Self {
        let mut packet = Self::default();
        packet.bytes[..4].copy_from_slice(b"RTR1");
        packet.bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        packet.bytes[6] = op;
        put64(&mut packet.bytes, 8, sequence);
        put64(&mut packet.bytes, 16, STREAM);
        put64(&mut packet.bytes, 24, epoch);
        put64(&mut packet.bytes, 32, frame);
        packet.bytes[44..46].copy_from_slice(&(slot as u16).to_le_bytes());
        packet
    }
    pub fn body(&mut self, bytes: &[u8]) {
        assert!(bytes.len() <= BYTES - HEADER);
        self.bytes[HEADER..HEADER + bytes.len()].copy_from_slice(bytes);
        self.len = HEADER + bytes.len();
        put32(&mut self.bytes, 40, bytes.len() as u32);
    }
    pub fn header(&self, slot: usize, stream: u64) -> Result<Header, Error> {
        if self.len < HEADER || self.len > BYTES {
            return Err(Error::InvalidShape);
        }
        let bytes = &self.bytes;
        if &bytes[..4] != b"RTR1"
            || u16::from_le_bytes([bytes[4], bytes[5]]) != 1
            || bytes[7] & !1 != 0
            || get64(bytes, 16)? != stream
            || get32(bytes, 40)? as usize != self.len - HEADER
            || u16::from_le_bytes([bytes[44], bytes[45]]) as usize != slot
            || bytes[46..48] != [0, 0]
        {
            return Err(Error::InvalidEvent);
        }
        let sequence = get64(bytes, 8)?;
        if sequence == 0 {
            return Err(Error::InvalidState);
        }
        Ok(Header {
            op: bytes[6],
            cancelled: bytes[7] != 0,
            sequence,
            epoch: get64(bytes, 24)?,
            frame: get64(bytes, 32)?,
        })
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Header {
    pub op: u8,
    pub cancelled: bool,
    pub sequence: u64,
    pub epoch: u64,
    pub frame: u64,
}
pub fn get32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(4).ok_or(Error::Capacity)?)
            .ok_or(Error::InvalidShape)?
            .try_into()
            .map_err(|_| Error::InvalidShape)?,
    ))
}
pub fn get64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(8).ok_or(Error::Capacity)?)
            .ok_or(Error::InvalidShape)?
            .try_into()
            .map_err(|_| Error::InvalidShape)?,
    ))
}
pub fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
pub fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
pub fn error_code(error: Error) -> u32 {
    match error {
        Error::InvalidConfig => 1,
        Error::Capacity => 2,
        Error::Exhausted => 3,
        Error::InvalidShape => 4,
        Error::InvalidFrame => 5,
        Error::InvalidId => 6,
        Error::InvalidEvent => 7,
        Error::Full => 8,
        Error::Disconnected => 9,
        Error::InvalidState => 10,
        Error::InvalidGraph => 11,
        Error::Cycle => 12,
        Error::PortType => 13,
        Error::StaleEpoch => 14,
        Error::Busy => 15,
        Error::InvalidParameter => 16,
        Error::InvalidOffset => 17,
        Error::Incomplete => 18,
        Error::Budget => 19,
        Error::StaleRevision => 20,
    }
}
pub fn decode_step(bytes: &[u8]) -> Result<NodeStep, Error> {
    if bytes.len() != 12 {
        return Err(Error::InvalidShape);
    }
    Ok(NodeStep {
        node: NodeId::from_parts(get32(bytes, 0)?, get32(bytes, 4)?)?,
        input_end: get32(bytes, 8)? as usize,
    })
}
pub fn encode_step(step: NodeStep, bytes: &mut [u8]) {
    put32(bytes, 0, step.node.slot());
    put32(bytes, 4, step.node.generation());
    put32(bytes, 8, step.input_end as u32);
}
pub fn decode_endpoint(bytes: &[u8]) -> Result<Endpoint, Error> {
    let slot = get32(bytes, 0)?;
    let generation = get32(bytes, 4)?;
    let node = if slot == u32::MAX {
        if generation != 0 {
            return Err(Error::InvalidId);
        }
        None
    } else {
        Some(NodeId::from_parts(slot, generation)?)
    };
    let kind = match bytes.get(8) {
        Some(0) => PortType::Audio,
        Some(1) => PortType::Midi,
        _ => return Err(Error::PortType),
    };
    if bytes.get(9) != Some(&0) {
        return Err(Error::InvalidEvent);
    }
    Ok(Endpoint {
        node,
        kind,
        port: u16::from_le_bytes([
            *bytes.get(10).ok_or(Error::InvalidShape)?,
            *bytes.get(11).ok_or(Error::InvalidShape)?,
        ]) as u32,
    })
}
pub fn encode_endpoint(endpoint: Endpoint, bytes: &mut [u8]) {
    put32(bytes, 0, endpoint.node.map_or(u32::MAX, NodeId::slot));
    put32(bytes, 4, endpoint.node.map_or(0, NodeId::generation));
    bytes[8] = if endpoint.kind == PortType::Audio {
        0
    } else {
        1
    };
    bytes[9] = 0;
    bytes[10..12].copy_from_slice(&(endpoint.port as u16).to_le_bytes());
}
pub fn decode_edge(bytes: &[u8]) -> Result<Connection, Error> {
    if bytes.len() != 24 {
        return Err(Error::InvalidShape);
    }
    Ok(Connection {
        source: decode_endpoint(&bytes[..12])?,
        destination: decode_endpoint(&bytes[12..])?,
    })
}
pub fn encode_edge(edge: Connection, bytes: &mut [u8]) {
    encode_endpoint(edge.source, &mut bytes[..12]);
    encode_endpoint(edge.destination, &mut bytes[12..]);
}
