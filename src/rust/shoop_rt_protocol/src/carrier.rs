//! Carriers move messages between a client and an engine [`Endpoint`].
//!
//! Carriers contain no engine or application semantics. [`InProcess`] passes typed
//! messages by direct call. [`RecordClient`] and [`RecordServer`] exchange encoded
//! records over a [`RecordPort`], such as a `MessagePort` bridge.

use std::collections::VecDeque;

use crate::codec::{self, CodecError};
use crate::message::{ClientMessage, DisconnectReason, EngineMessage, Lane};
use crate::MAX_RECORD_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarrierError {
    /// The carrier cannot take more now; retry later.
    Full,
    Disconnected,
    Codec(CodecError),
}

/// Engine-side output, with one bounded queue per lane. Capacities are prepared at
/// construction and never grow.
pub struct Outbox {
    reliable: VecDeque<EngineMessage>,
    telemetry: VecDeque<EngineMessage>,
    bulk: VecDeque<EngineMessage>,
    capacity: [usize; 3],
    dropped_telemetry: u64,
}

impl Outbox {
    pub fn new(reliable: usize, telemetry: usize, bulk: usize) -> Self {
        Self {
            reliable: VecDeque::with_capacity(reliable),
            telemetry: VecDeque::with_capacity(telemetry),
            bulk: VecDeque::with_capacity(bulk),
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
        }
    }

    /// Next message: reliable first, then bulk, then telemetry.
    pub fn pop(&mut self) -> Option<EngineMessage> {
        self.reliable
            .pop_front()
            .or_else(|| self.bulk.pop_front())
            .or_else(|| self.telemetry.pop_front())
    }

    fn unpop(&mut self, message: EngineMessage) {
        match message.lane() {
            Lane::Reliable => self.reliable.push_front(message),
            Lane::Telemetry => self.telemetry.push_front(message),
            Lane::Bulk => self.bulk.push_front(message),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.reliable.is_empty() && self.telemetry.is_empty() && self.bulk.is_empty()
    }

    pub fn dropped_telemetry(&self) -> u64 {
        self.dropped_telemetry
    }
}

/// The engine side of a connection.
pub trait Endpoint {
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
    pub fn endpoint(&self) -> &E {
        &self.endpoint
    }
    pub fn endpoint_mut(&mut self) -> &mut E {
        &mut self.endpoint
    }
}

impl<E: Endpoint> Carrier for InProcess<E> {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        self.endpoint.deliver(message, &mut self.outbox);
        Ok(())
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        if self.outbox.is_empty() {
            self.endpoint.poll(&mut self.outbox);
        }
        self.outbox.pop()
    }
}

/// A bidirectional transport of encoded records.
pub trait RecordPort {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError>;
    /// Copy the next received record into `buf` and return its length.
    fn take(&mut self, buf: &mut [u8]) -> Option<usize>;
}

/// The client side of a record carrier.
pub struct RecordClient<P> {
    port: P,
    buf: Box<[u8]>,
}

impl<P: RecordPort> RecordClient<P> {
    pub fn new(port: P) -> Self {
        Self {
            port,
            buf: vec![0; MAX_RECORD_BYTES].into_boxed_slice(),
        }
    }
    pub fn port_mut(&mut self) -> &mut P {
        &mut self.port
    }
}

impl<P: RecordPort> Carrier for RecordClient<P> {
    fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
        let len = codec::encode_client(&message, &mut self.buf).map_err(CarrierError::Codec)?;
        self.port.post(&self.buf[..len])
    }
    fn receive(&mut self) -> Option<EngineMessage> {
        let len = self.port.take(&mut self.buf)?;
        Some(
            codec::decode_engine(&self.buf[..len])
                .unwrap_or(EngineMessage::Disconnected(DisconnectReason::ProtocolError)),
        )
    }
}

/// The engine side of a record carrier: decodes client records into an endpoint and
/// encodes its output. Pumping does no allocation.
pub struct RecordServer<E, P> {
    endpoint: E,
    port: P,
    outbox: Outbox,
    buf: Box<[u8]>,
}

impl<E: Endpoint, P: RecordPort> RecordServer<E, P> {
    pub fn new(endpoint: E, port: P, outbox: Outbox) -> Self {
        Self {
            endpoint,
            port,
            outbox,
            buf: vec![0; MAX_RECORD_BYTES].into_boxed_slice(),
        }
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
        for _ in 0..max_records {
            let Some(len) = self.port.take(&mut self.buf) else {
                break;
            };
            match codec::decode_client(&self.buf[..len]) {
                Ok(message) => self.endpoint.deliver(message, &mut self.outbox),
                Err(_) => {
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
            if self.port.post(&self.buf[..len]).is_err() {
                self.outbox.unpop(message);
                break;
            }
        }
    }
}
