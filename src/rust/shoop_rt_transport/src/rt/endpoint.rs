//! A queue-backed carrier endpoint, independent of processing and pacing.
use super::{Control, Error};
use crate::carrier::{CarrierError, Endpoint as CarrierEndpoint, Outbox};
use alloc::{boxed::Box, vec};
use shoop_rt_protocol::{
    immediate::{
        Batch, Command, CommandRecord, Header as CommandHeader, Response, SwitchProgram, COMMANDS,
    },
    snapshot::{Chunk, Header, SnapshotDelivery, CHUNK_BYTES},
    Capabilities, ClientMessage, DisconnectReason, DriverStatus, EngineMessage, Lane, Limits,
    PROTOCOL_VERSION,
};

#[derive(Clone, Copy)]
/// Fixed configuration and descriptive driver metadata; no pacing controls.
pub struct Settings {
    pub graph: u64,
    pub topology: u64,
    pub limits: Limits,
    pub driver: DriverStatus,
}

/// Supports command groups and snapshots; legacy requests, resync and bulk are ignored.
pub struct Endpoint {
    ports: Control,
    settings: Settings,
    capabilities: Capabilities,
    connected: bool,
    pending: usize,
    discard: usize,
    snapshots: Snapshots,
}
impl Endpoint {
    /// Legacy request/bulk credits are always zero.
    pub fn new(ports: Control, mut settings: Settings) -> Result<Self, Error> {
        if settings.driver.sample_rate == 0 {
            return Err(Error::InvalidConfig);
        }
        settings.limits.reliable_credits = 0;
        settings.limits.bulk_credits = 0;
        let capacity = ports.snapshots.capacity();
        Ok(Self {
            ports,
            settings,
            capabilities: Capabilities::default(),
            connected: false,
            pending: 0,
            discard: 0,
            snapshots: Snapshots {
                scratch: vec![0; capacity].into_boxed_slice(),
                accepted: vec![0; capacity].into_boxed_slice(),
                len: 0,
                offset: 0,
                sequence: 0,
                enabled: false,
                delivery: Default::default(),
                requested: None,
                reply: false,
                completion: None,
                configuration_sent: false,
                sent_frame: None,
            },
        })
    }
    pub fn set_driver(&mut self, driver: DriverStatus) {
        self.settings.driver = driver;
    }
    pub fn snapshot_capacity(&self) -> usize {
        self.ports.snapshots.capacity()
    }
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }
    pub fn is_connected(&self) -> bool {
        self.connected
    }
    pub fn set_configuration(&mut self, graph: u64, topology: u64) {
        self.settings.graph = graph;
        self.settings.topology = topology;
        self.snapshots.configuration_sent = false;
        self.snapshots.len = 0;
        self.snapshots.offset = 0;
        self.snapshots.sent_frame = None;
    }
    pub fn read_snapshot(&self, bytes: &mut [u8]) -> Option<(u32, usize)> {
        let (sequence, len) = self.ports.snapshots.read(bytes)?;
        self.matches(Header::read(&bytes[..len])?)
            .then_some((sequence, len))
    }
    pub fn snapshot_error(&self) -> Option<Error> {
        self.ports.snapshots.error()
    }
    fn matches(&self, header: Header) -> bool {
        (header.graph, header.topology) == (self.settings.graph, self.settings.topology)
    }
    fn reset(&mut self) {
        self.connected = false;
        self.discard = self.pending;
        self.snapshots.enabled = false;
        self.snapshots.delivery = Default::default();
        self.snapshots.requested = None;
        self.snapshots.reply = false;
        self.snapshots.completion = None;
        self.snapshots.configuration_sent = false;
        self.snapshots.sent_frame = None;
        self.snapshots.len = 0;
        self.snapshots.offset = 0;
        self.snapshots.sequence = 0;
    }
    fn submit(&mut self, batch: Batch) -> Result<(), CarrierError> {
        if !self.connected {
            return Err(CarrierError::Disconnected);
        }
        if let [Command::SwitchProgram(program)] = batch.commands() {
            if self.ports.switches.is_some() {
                return self.submit_switch(batch.header(), *program);
            }
        }
        let mut records = [CommandRecord::new(batch.commands()[0]); COMMANDS];
        for (record, &command) in records.iter_mut().zip(batch.commands()) {
            *record = CommandRecord::new(command);
        }
        self.ports
            .commands
            .push(batch.header(), &records[..batch.commands().len()])
            .map_err(|_| CarrierError::Full)?;
        self.pending += 1;
        Ok(())
    }

    /// Handoffs belong to the program loader and outlive its application connection.
    pub fn submit_switch(
        &mut self,
        header: CommandHeader,
        program: SwitchProgram,
    ) -> Result<(), CarrierError> {
        self.ports
            .switches
            .as_mut()
            .expect("prepared switch queue")
            .push(
                header,
                &[CommandRecord::new(Command::SwitchProgram(program))],
            )
            .map_err(|_| CarrierError::Full)
    }

    /// Returns false when the next response remains queued because the outbox is full.
    pub fn drain_responses(&mut self, out: &mut Outbox) -> bool {
        for (lane, queue) in core::iter::once(&mut self.ports.commands)
            .chain(self.ports.switches.iter_mut())
            .enumerate()
        {
            let switch_lane = lane != 0;
            while let Some(group) = queue.response() {
                if switch_lane || (self.connected && self.discard == 0) {
                    if out.available(Lane::Reliable) == 0 {
                        group.leave_unread();
                        return false;
                    }
                    let mut records = [CommandRecord::new(
                        shoop_rt_protocol::immediate::Command::BusMute(0, false),
                    ); COMMANDS];
                    let mut count = 0;
                    for &record in group.commands() {
                        if let shoop_rt_protocol::immediate::Command::SwitchProgram(d) =
                            record.command()
                        {
                            if record.code() == shoop_rt_protocol::immediate::Code::Applied
                                && self.settings.graph != d.revision
                            {
                                self.settings.graph = d.revision;
                                self.settings.topology = d.revision;
                                self.snapshots.configuration_sent = false;
                                self.snapshots.len = 0;
                                self.snapshots.offset = 0;
                                self.snapshots.sent_frame = None;
                            }
                        }
                        records[count] = record;
                        count += 1;
                    }
                    out.push(EngineMessage::ImmediateResponse(
                        Response::new(group.header, &records[..count])
                            .expect("completed command group"),
                    ))
                    .expect("available response slot");
                }
                if !switch_lane {
                    self.discard = self.discard.saturating_sub(1);
                    self.pending -= 1;
                }
            }
        }
        true
    }
    fn publications(&mut self, out: &mut Outbox) {
        if !self.connected {
            return;
        }
        let state = &mut self.snapshots;
        if !state.flush_completion(out) {
            return;
        }
        if !state.enabled {
            return;
        }
        if state.offset < state.len {
            state.send(out);
            return;
        }
        if state.requested.is_none() && state.delivery == SnapshotDelivery::Pull {
            return;
        }
        if !state.configuration_sent {
            if out
                .push(EngineMessage::SnapshotConfiguration {
                    graph: self.settings.graph,
                    topology: self.settings.topology,
                })
                .is_err()
            {
                return;
            }
            state.configuration_sent = true;
        }
        let requested = state.requested.take();
        let candidate =
            self.ports
                .snapshots
                .read(&mut state.scratch)
                .and_then(|(sequence, len)| {
                    let header = Header::read(&state.scratch[..len])?;
                    ((header.graph, header.topology)
                        == (self.settings.graph, self.settings.topology))
                        .then_some((sequence, len, header))
                });
        let Some((sequence, len, header)) = candidate else {
            if requested.is_some() {
                state.completion = Some(0);
                state.flush_completion(out);
            }
            return;
        };
        if let Some(after) = requested {
            if sequence <= after {
                state.completion = Some(sequence);
                state.flush_completion(out);
                return;
            }
        } else {
            let SnapshotDelivery::Push { max_hz } = state.delivery else {
                return;
            };
            let interval = u64::from(self.settings.driver.sample_rate)
                .div_ceil(u64::from(max_hz.get()))
                .max(1);
            if sequence <= state.sequence
                || state
                    .sent_frame
                    .is_some_and(|last| header.frame < last.saturating_add(interval))
            {
                return;
            }
        }
        state.accepted[..len].copy_from_slice(&state.scratch[..len]);
        if requested.is_none() {
            out.discard_push_snapshots();
        }
        state.len = len;
        state.offset = 0;
        state.sequence = sequence;
        state.reply = requested.is_some();
        state.sent_frame = Some(header.frame);
        state.send(out);
    }
}
impl CarrierEndpoint for Endpoint {
    fn try_deliver(
        &mut self,
        message: ClientMessage,
        out: &mut Outbox,
    ) -> Result<(), CarrierError> {
        match message {
            ClientMessage::Immediate(batch) => return self.submit(batch),
            ClientMessage::Hello { version } => {
                if out.available(Lane::Reliable) == 0 {
                    return Err(CarrierError::Full);
                }
                self.reset();
                out.discard_push_snapshots();
                let reply = if version == PROTOCOL_VERSION {
                    self.connected = true;
                    EngineMessage::Welcome {
                        version,
                        capabilities: self.capabilities,
                        limits: self.settings.limits,
                        driver: self.settings.driver,
                    }
                } else {
                    EngineMessage::Rejected {
                        version: PROTOCOL_VERSION,
                    }
                };
                out.push(reply)?;
            }
            ClientMessage::Goodbye => {
                out.push(EngineMessage::Disconnected(DisconnectReason::Shutdown))?;
                self.reset();
                out.discard_push_snapshots();
            }
            ClientMessage::ObserveSnapshots(enabled) if self.connected => {
                let state = &mut self.snapshots;
                state.enabled = enabled;
                state.configuration_sent = false;
                if !enabled {
                    if state.reply || state.requested.is_some() {
                        state.completion = Some(0);
                    }
                    state.offset = state.len;
                    state.reply = false;
                    state.requested = None;
                    out.discard_push_snapshots();
                }
            }
            ClientMessage::ConfigureSnapshots(delivery) if self.connected => {
                self.snapshots.delivery = delivery;
            }
            ClientMessage::RequestSnapshot { after } if self.connected => {
                if self.snapshots.requested.is_some()
                    || self.snapshots.reply
                    || self.snapshots.completion.is_some()
                {
                    return Err(CarrierError::Full);
                }
                if self.snapshots.enabled {
                    self.snapshots.requested = Some(after);
                } else {
                    self.snapshots.completion = Some(0);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn deliver(&mut self, message: ClientMessage, out: &mut Outbox) {
        let _ = self.try_deliver(message, out);
    }
    fn poll(&mut self, out: &mut Outbox) {
        let faulted = self.ports.snapshots.error().is_some();
        self.drain_responses(out);
        if faulted {
            if self.connected
                && out
                    .push(EngineMessage::Disconnected(DisconnectReason::Fault))
                    .is_ok()
            {
                self.reset();
                out.discard_push_snapshots();
            }
            return;
        }
        self.publications(out);
    }
}
struct Snapshots {
    scratch: Box<[u8]>,
    accepted: Box<[u8]>,
    len: usize,
    offset: usize,
    sequence: u32,
    enabled: bool,
    delivery: SnapshotDelivery,
    requested: Option<u32>,
    reply: bool,
    completion: Option<u32>,
    configuration_sent: bool,
    sent_frame: Option<u64>,
}
impl Snapshots {
    fn flush_completion(&mut self, out: &mut Outbox) -> bool {
        if let Some(sequence) = self.completion {
            if out.push(EngineMessage::SnapshotReady { sequence }).is_err() {
                return false;
            }
            self.completion = None;
        }
        true
    }
    fn send(&mut self, out: &mut Outbox) {
        let lane = if self.reply {
            Lane::Reliable
        } else {
            Lane::Telemetry
        };
        while self.offset < self.len && out.available(lane) > 0 {
            let len = (self.len - self.offset).min(CHUNK_BYTES);
            let mut chunk = Chunk {
                sequence: self.sequence,
                total: self.len as u32,
                offset: self.offset as u32,
                len: len as u16,
                bytes: [0; CHUNK_BYTES],
            };
            chunk.bytes[..len].copy_from_slice(&self.accepted[self.offset..self.offset + len]);
            out.push(if self.reply {
                EngineMessage::SnapshotReply(chunk)
            } else {
                EngineMessage::Snapshot(chunk)
            })
            .expect("available snapshot slot");
            self.offset += len;
        }
        if self.offset == self.len && self.reply {
            self.reply = false;
            self.completion = Some(self.sequence);
            self.flush_completion(out);
        }
    }
}
