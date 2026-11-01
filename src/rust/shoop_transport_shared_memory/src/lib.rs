//! Both ends of a process-shared record carrier; no engine owners cross the mapping.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]
use iceoryx2::prelude::{
    ipc_threadsafe, BackpressureStrategy, FileName, Node, NodeBuilder, ServiceName,
};
use iceoryx2::prelude::{PortFactory, SemanticString};
use iceoryx2::{
    config::Config as IoxConfig,
    port::{publisher::Publisher, subscriber::Subscriber},
};
use shoop_transport::{CarrierError, CodecError, Lane, RecordPort, MAX_RECORD_BYTES};
use std::{io, net::TcpStream};
pub const DESCRIPTOR_BYTES: usize = 40;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    Client,
    Server,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Descriptor {
    session: [u8; 16],
    slots: [u32; 3],
}
impl Descriptor {
    pub fn new(slots: [u32; 3]) -> io::Result<Self> {
        if slots.iter().any(|&n| n == 0 || n > 65536) {
            return Err(io::Error::other("invalid shared queue capacity"));
        }
        let mut session = [0; 16];
        getrandom::fill(&mut session).map_err(io::Error::other)?;
        Ok(Self { session, slots })
    }
    pub fn encode(&self) -> [u8; DESCRIPTOR_BYTES] {
        let mut out = [0; DESCRIPTOR_BYTES];
        out[..4].copy_from_slice(b"SDM1");
        out[4..6].copy_from_slice(&1u16.to_le_bytes());
        out[6..8].copy_from_slice(&(MAX_RECORD_BYTES as u16).to_le_bytes());
        for (i, n) in self.slots.iter().enumerate() {
            out[8 + 4 * i..12 + 4 * i].copy_from_slice(&n.to_le_bytes());
        }
        out[20..36].copy_from_slice(&self.session);
        out
    }
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != DESCRIPTOR_BYTES
            || &bytes[..4] != b"SDM1"
            || bytes[4..6] != 1u16.to_le_bytes()
            || bytes[6..8] != (MAX_RECORD_BYTES as u16).to_le_bytes()
            || bytes[36..40] != [0; 4]
        {
            return Err(io::Error::other("incompatible shared-memory descriptor"));
        }
        let slots = std::array::from_fn(|i| {
            u32::from_le_bytes(bytes[8 + 4 * i..12 + 4 * i].try_into().unwrap())
        });
        if slots.iter().any(|&n| n == 0 || n > 65536) {
            return Err(io::Error::other("invalid shared queue capacity"));
        }
        Ok(Self {
            session: bytes[20..36].try_into().unwrap(),
            slots,
        })
    }
    fn prefix(&self) -> String {
        self.session.iter().map(|b| format!("{b:02x}")).collect()
    }
}
type Service = ipc_threadsafe::Service;
type Payload = [u8; MAX_RECORD_BYTES];
pub struct Port {
    publishers: Vec<Publisher<Service, Payload, ()>>,
    subscribers: Vec<Subscriber<Service, Payload, ()>>,
    node: Node<Service>,
    liveness: Option<TcpStream>,
    failure: Option<CarrierError>,
}
fn io_error(error: impl std::fmt::Debug) -> io::Error {
    io::Error::other(format!("shared-memory setup: {error:?}"))
}
fn index(lane: Lane) -> usize {
    match lane {
        Lane::Reliable => 0,
        Lane::Telemetry => 1,
        Lane::Bulk => 2,
    }
}
impl Port {
    pub fn create(descriptor: Descriptor, side: Side) -> io::Result<Self> {
        Self::prepare(descriptor, side, true)
    }
    pub fn open(descriptor: Descriptor, side: Side) -> io::Result<Self> {
        Self::prepare(descriptor, side, false)
    }
    fn prepare(descriptor: Descriptor, side: Side, create: bool) -> io::Result<Self> {
        let descriptor = Descriptor::decode(&descriptor.encode())?;
        let prefix = descriptor.prefix();
        let mut config = IoxConfig::default();
        config.global.prefix =
            FileName::new(format!("sd_{prefix}_").as_bytes()).map_err(io_error)?;
        let node = NodeBuilder::new()
            .config(&config)
            .create::<Service>()
            .map_err(io_error)?;
        let mut publishers = Vec::with_capacity(3);
        let mut subscribers = Vec::with_capacity(3);
        for direction in 0..2 {
            for lane in 0..3 {
                let name: ServiceName = format!("shoop/{prefix}/{direction}/{lane}")
                    .as_str()
                    .try_into()
                    .map_err(io_error)?;
                let builder = node
                    .service_builder(&name)
                    .publish_subscribe::<Payload>()
                    .enable_safe_overflow(lane == 1)
                    .subscriber_max_buffer_size(descriptor.slots[lane] as usize)
                    .max_publishers(1)
                    .max_subscribers(1)
                    .max_nodes(2);
                let service = if create {
                    builder.create().map_err(io_error)?
                } else {
                    builder.open().map_err(io_error)?
                };
                if service.static_config().subscriber_max_buffer_size()
                    != descriptor.slots[lane] as usize
                    || service.static_config().has_safe_overflow() != (lane == 1)
                {
                    return Err(io::Error::other("shared-memory layout mismatch"));
                }
                let sending = match side {
                    Side::Client => direction == 0,
                    Side::Server => direction == 1,
                };
                if sending {
                    publishers.push(
                        service
                            .publisher_builder()
                            .max_loaned_samples(1)
                            .backpressure_strategy(BackpressureStrategy::DiscardData)
                            .create()
                            .map_err(io_error)?,
                    );
                } else {
                    subscribers.push(service.subscriber_builder().create().map_err(io_error)?);
                }
            }
        }
        Ok(Self {
            publishers,
            subscribers,
            node,
            liveness: None,
            failure: None,
        })
    }
    pub fn with_liveness(mut self, stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        self.liveness = Some(stream);
        Ok(self)
    }
    pub fn cleanup_dead_peer(&self) -> io::Result<u64> {
        let report = self.node.try_cleanup_dead_nodes();
        if report.failed_cleanups != 0 {
            return Err(io::Error::other(
                "dead peer resources could not be reclaimed",
            ));
        }
        Ok(report.cleanups)
    }
    fn check_liveness(&mut self) {
        if self.failure.is_some() {
            return;
        }
        if let Some(stream) = &self.liveness {
            match stream.peek(&mut [0; 1]) {
                Ok(0) => self.failure = Some(CarrierError::Disconnected),
                Ok(_) => self.failure = Some(CarrierError::Codec(CodecError::Invalid)),
                Err(error)
                    if error.kind() != io::ErrorKind::WouldBlock
                        && error.kind() != io::ErrorKind::Interrupted =>
                {
                    self.failure = Some(CarrierError::Disconnected)
                }
                _ => {}
            }
        }
    }
}
impl Drop for Port {
    fn drop(&mut self) {
        let _ = self.node.try_cleanup_dead_nodes();
    }
}
impl RecordPort for Port {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.post_on_lane(record, Lane::Reliable)
    }
    fn post_on_lane(&mut self, record: &[u8], lane: Lane) -> Result<(), CarrierError> {
        self.check_liveness();
        if let Some(error) = self.failure {
            return Err(error);
        }
        if record.len() < 2
            || record.len() > MAX_RECORD_BYTES
            || usize::from(u16::from_le_bytes([record[0], record[1]])) != record.len()
        {
            return Err(CarrierError::Codec(CodecError::Invalid));
        }
        let mut payload = [0; MAX_RECORD_BYTES];
        payload[..record.len()].copy_from_slice(record);
        match self.publishers[index(lane)].send_copy(payload) {
            Ok(1) => Ok(()),
            Ok(0) => Err(CarrierError::Full),
            _ => {
                self.failure = Some(CarrierError::Disconnected);
                Err(CarrierError::Disconnected)
            }
        }
    }
    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        self.check_liveness();
        if self.failure.is_some() {
            return None;
        }
        for lane in [0, 2, 1] {
            match self.subscribers[lane].receive() {
                Ok(Some(sample)) => {
                    let bytes = sample.payload();
                    let len = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
                    if !(2..=MAX_RECORD_BYTES).contains(&len) || len > buf.len() {
                        self.failure = Some(CarrierError::Codec(CodecError::Invalid));
                        return None;
                    }
                    buf[..len].copy_from_slice(&bytes[..len]);
                    return Some(len);
                }
                Ok(None) => {}
                Err(_) => {
                    self.failure = Some(CarrierError::Disconnected);
                    return None;
                }
            }
        }
        None
    }
    fn failure(&mut self) -> Option<CarrierError> {
        self.check_liveness();
        self.failure
    }
}
