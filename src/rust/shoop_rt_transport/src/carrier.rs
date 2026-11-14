//! Carriers move messages between a client and an engine [`Endpoint`].
//!
//! Carriers contain no engine or application semantics. [`InProcess`] passes typed
//! messages by direct call. [`RecordClient`] and [`RecordServer`] exchange encoded
//! records over a [`RecordPort`], such as a `MessagePort` bridge.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec;

use shoop_rt_protocol::codec;
use shoop_rt_protocol::message::{ClientMessage, DisconnectReason, EngineMessage, Lane};
use shoop_rt_protocol::MAX_RECORD_BYTES;

pub use shoop_transport::{CarrierError, RecordPort};

/// Engine-side output, with one bounded queue per lane. Capacities are prepared at
/// construction and never grow.
pub struct Outbox {
    reliable: VecDeque<EngineMessage>,
    telemetry: VecDeque<EngineMessage>,
    bulk: VecDeque<EngineMessage>,
    profiling: VecDeque<EngineMessage>,
    low_lane: usize,
    capacity: [usize; 3],
    dropped_telemetry: u64,
}

impl Outbox {
    pub fn new(reliable: usize, telemetry: usize, bulk: usize) -> Self {
        Self {
            reliable: VecDeque::with_capacity(reliable),
            telemetry: VecDeque::with_capacity(telemetry),
            bulk: VecDeque::with_capacity(bulk),
            profiling: VecDeque::with_capacity(256),
            low_lane: 0,
            capacity: [reliable, telemetry, bulk],
            dropped_telemetry: 0,
        }
    }

    /// Queue a message. A full telemetry queue drops its oldest record; full
    /// reliable and bulk queues refuse.
    pub fn push(&mut self, message: EngineMessage) -> Result<(), CarrierError> {
        let (queue, capacity) = match message.lane() {
            Lane::Reliable => (&mut self.reliable, self.capacity[0]),
            Lane::Telemetry => (&mut self.telemetry, self.capacity[1]),
            Lane::Bulk => (&mut self.bulk, self.capacity[2]),
            Lane::Profiling => (&mut self.profiling, 256),
        };
        if queue.len() == capacity {
            if message.lane() != Lane::Telemetry || capacity == 0 {
                return Err(CarrierError::Full);
            }
            queue.pop_front();
            self.dropped_telemetry += 1;
        }
        queue.push_back(message);
        Ok(())
    }

    /// Free space in a lane.
    pub fn available(&self, lane: Lane) -> usize {
        match lane {
            Lane::Reliable => self.capacity[0] - self.reliable.len(),
            Lane::Telemetry => self.capacity[1] - self.telemetry.len(),
            Lane::Bulk => self.capacity[2] - self.bulk.len(),
            Lane::Profiling => 256 - self.profiling.len(),
        }
    }

    /// Reliable first, then fair rotation over bulk, telemetry and profiling.
    pub fn pop(&mut self) -> Option<EngineMessage> {
        if let Some(message) = self.reliable.pop_front() {
            return Some(message);
        }
        for _ in 0..3 {
            let lane = self.low_lane;
            self.low_lane = (self.low_lane + 1) % 3;
            let message = match lane {
                0 => self.bulk.pop_front(),
                1 => self.telemetry.pop_front(),
                _ => self.profiling.pop_front(),
            };
            if message.is_some() {
                return message;
            }
        }
        None
    }

    fn unpop(&mut self, message: EngineMessage) {
        match message.lane() {
            Lane::Reliable => self.reliable.push_front(message),
            Lane::Telemetry => self.telemetry.push_front(message),
            Lane::Bulk => self.bulk.push_front(message),
            Lane::Profiling => self.profiling.push_front(message),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.reliable.is_empty()
            && self.telemetry.is_empty()
            && self.bulk.is_empty()
            && self.profiling.is_empty()
    }

    pub fn dropped_telemetry(&self) -> u64 {
        self.dropped_telemetry
    }

    /// Supersede only undelivered push snapshots, preserving other traffic.
    pub fn discard_push_snapshots(&mut self) {
        let before = self.telemetry.len();
        self.telemetry
            .retain(|message| !matches!(message, EngineMessage::Snapshot(_)));
        self.dropped_telemetry = self
            .dropped_telemetry
            .saturating_add((before - self.telemetry.len()) as u64);
    }
}

/// The engine side of a connection.
pub trait Endpoint {
    /// Refuse locally without consuming a complete message when staging is full.
    fn try_deliver(
        &mut self,
        message: ClientMessage,
        out: &mut Outbox,
    ) -> Result<(), CarrierError> {
        self.deliver(message, out);
        Ok(())
    }
    /// Handle one client message, queueing any answers.
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox);
    /// Queue messages the engine produced on its own: results that completed,
    /// telemetry, bulk chunks, limit changes.
    fn poll(&mut self, out: &mut Outbox);
}

/// The client side of a connection.
pub trait Carrier {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError>;
    fn receive(&mut self) -> Option<EngineMessage>;
    /// Asynchronous carriers default to a bounded push stream.
    fn default_snapshot_delivery(&self) -> shoop_rt_protocol::snapshot::SnapshotDelivery {
        Default::default()
    }
}

/// Direct calls into an endpoint owned by the carrier, without serialization.
pub struct InProcess<E> {
    endpoint: E,
    outbox: Outbox,
}

impl<E: Endpoint> InProcess<E> {
    pub fn new(endpoint: E, outbox: Outbox) -> Self {
        Self { endpoint, outbox }
    }
    pub fn into_parts(self) -> (E, Outbox) {
        (self.endpoint, self.outbox)
    }
    pub fn endpoint(&self) -> &E {
        &self.endpoint
    }
    pub fn endpoint_mut(&mut self) -> &mut E {
        &mut self.endpoint
    }
}

impl<E: Endpoint> Carrier for InProcess<E> {
    fn default_snapshot_delivery(&self) -> shoop_rt_protocol::snapshot::SnapshotDelivery {
        if cfg!(target_arch = "wasm32") {
            Default::default()
        } else {
            shoop_rt_protocol::snapshot::SnapshotDelivery::Pull
        }
    }
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        self.endpoint.try_deliver(message, &mut self.outbox)
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        if self.outbox.is_empty() {
            self.endpoint.poll(&mut self.outbox);
        }
        self.outbox.pop()
    }
}

/// The client side of a record carrier.
pub struct RecordClient<P> {
    port: P,
    buf: Box<[u8]>,
    failed: bool,
}

impl<P: RecordPort> RecordClient<P> {
    pub fn new(port: P) -> Self {
        Self {
            port,
            failed: false,
            buf: vec![0; MAX_RECORD_BYTES].into_boxed_slice(),
        }
    }
    pub fn into_port(self) -> P {
        self.port
    }
    pub fn port_mut(&mut self) -> &mut P {
        &mut self.port
    }
}

impl<P: RecordPort> Carrier for RecordClient<P> {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        let len = codec::encode_client(&message, &mut self.buf).map_err(CarrierError::Codec)?;
        if self.failed {
            return Err(CarrierError::Disconnected);
        }
        self.port.post_on_lane(&self.buf[..len], message.lane())
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        if self.failed {
            return None;
        }
        let len = match self.port.take(&mut self.buf) {
            Some(len) => len,
            None => {
                let failure = self.port.failure()?;
                self.failed = true;
                let reason = match failure {
                    CarrierError::Codec(_) => DisconnectReason::ProtocolError,
                    _ => DisconnectReason::Shutdown,
                };
                return Some(EngineMessage::Disconnected(reason));
            }
        };
        let message = codec::decode_engine(&self.buf[..len])
            .unwrap_or(EngineMessage::Disconnected(DisconnectReason::ProtocolError));
        self.port.consumed(message.lane());
        Some(message)
    }
}

/// The engine side of a record carrier: decodes client records into an endpoint and
/// encodes its output. Pumping does no allocation.
pub struct RecordServer<E, P> {
    pending: Option<ClientMessage>,
    endpoint: E,
    failure: Option<CarrierError>,
    port: P,
    outbox: Outbox,
    buf: Box<[u8]>,
}

impl<E: Endpoint, P: RecordPort> RecordServer<E, P> {
    pub fn new(endpoint: E, port: P, outbox: Outbox) -> Self {
        Self {
            pending: None,
            endpoint,
            failure: None,
            port,
            outbox,
            buf: vec![0; MAX_RECORD_BYTES].into_boxed_slice(),
        }
    }
    pub fn into_parts(self) -> (E, P, Outbox) {
        (self.endpoint, self.port, self.outbox)
    }
    pub fn failure(&self) -> Option<CarrierError> {
        self.failure
    }
    pub fn endpoint(&self) -> &E {
        &self.endpoint
    }
    pub fn endpoint_mut(&mut self) -> &mut E {
        &mut self.endpoint
    }

    /// Deliver up to `max_records` received records, poll the endpoint and post its
    /// output until the port is full.
    pub fn pump(&mut self, max_records: usize) {
        if let Some(error) = self.port.failure() {
            self.pending = None;
            self.failure = Some(error);
            return;
        }
        for _ in 0..max_records {
            if let Some(message) = self.pending.take() {
                match self.endpoint.try_deliver(message, &mut self.outbox) {
                    Ok(()) => {}
                    Err(CarrierError::Full) => {
                        self.pending = Some(message);
                        break;
                    }
                    Err(error) => {
                        self.failure = Some(error);
                        break;
                    }
                }
            }
            let Some(len) = self.port.take(&mut self.buf) else {
                self.failure = self.failure.or_else(|| self.port.failure());
                break;
            };
            match codec::decode_client(&self.buf[..len]) {
                Ok(message) => match self.endpoint.try_deliver(message, &mut self.outbox) {
                    Ok(()) => {}
                    Err(CarrierError::Full) => {
                        self.pending = Some(message);
                        break;
                    }
                    Err(error) => {
                        self.failure = Some(error);
                        break;
                    }
                },
                Err(error) => {
                    self.failure = Some(CarrierError::Codec(error));
                    let _ = self
                        .outbox
                        .push(EngineMessage::Disconnected(DisconnectReason::ProtocolError));
                }
            }
        }
        self.endpoint.poll(&mut self.outbox);
        while let Some(message) = self.outbox.pop() {
            let len = match codec::encode_engine(&message, &mut self.buf) {
                Ok(len) => len,
                Err(_) => continue,
            };
            if let Err(error) = self.port.post_on_lane(&self.buf[..len], message.lane()) {
                if error != CarrierError::Full {
                    self.failure = Some(error);
                }
                self.outbox.unpop(message);
                break;
            }
        }
    }
}
