use alloc::{collections::VecDeque, vec::Vec};
use core::time::Duration;
use shoop_app::{ConnectionId, Service};
use shoop_app_protocol::{codec, stream::StreamEncoder, *};
use shoop_transport::{CarrierError, RecordPort, MAX_RECORD_BYTES};

pub struct Session<P> {
    network: Option<Network>,
    heartbeat: Option<crate::Heartbeat>,
    pub id: ConnectionId,
    pub port: P,
    age: Duration,
    attached: bool,
    closing: bool,
    closing_since: Option<Duration>,
    semantic: Option<StreamEncoder>,
    snapshot: bool,
    media: Option<StreamEncoder>,
    pending: Option<Vec<u8>>,
    transfer: u64,
    replies: VecDeque<ServiceMessage>,
}
impl<P: RecordPort> Session<P> {
    pub fn new(id: ConnectionId, port: P) -> Self {
        Self {
            network: None,
            heartbeat: None,
            id,
            port,
            age: Duration::ZERO,
            attached: false,
            closing: false,
            closing_since: None,
            semantic: None,
            snapshot: false,
            media: None,
            pending: None,
            transfer: 1,
            replies: VecDeque::new(),
        }
    }
    pub fn with_heartbeat(
        mut self,
        config: crate::HeartbeatConfig,
        seed: u64,
    ) -> Result<Self, CarrierError> {
        self.heartbeat = Some(crate::Heartbeat::new(config, seed)?);
        Ok(self)
    }
    pub fn with_network(
        mut self,
        config: crate::HeartbeatConfig,
        seed: u64,
        snapshot_hz: u32,
    ) -> Result<Self, CarrierError> {
        if !(1..=60).contains(&snapshot_hz) {
            return Err(CarrierError::Codec(shoop_transport::CodecError::Invalid));
        }
        self = self.with_heartbeat(config, seed)?;
        self.network = Some(Network::new(snapshot_hz));
        Ok(self)
    }
    pub fn retire(&mut self) {
        if let Some(network) = self.network.as_mut() {
            network.clear();
        }
        self.closing = true;
        self.closing_since.get_or_insert(self.age);
        self.media = None;
        self.semantic = None;
        self.pending = None;
    }
    pub fn advance(&mut self, elapsed: Duration) {
        self.age = self.age.saturating_add(elapsed);
        if let Some(network) = self.network.as_mut() {
            network.since = network.since.saturating_add(elapsed);
        }
        if let Some(heartbeat) = self.heartbeat.as_mut() {
            heartbeat.advance(elapsed);
        }
    }
    pub fn receive(&mut self, service: &mut Service) {
        if self
            .heartbeat
            .as_ref()
            .is_some_and(|heartbeat| heartbeat.expired())
        {
            service.detach(self.id);
            self.retire();
            return;
        }
        if self.closing {
            return;
        }
        let mut bytes = [0; MAX_RECORD_BYTES];
        let mut commands = 0;
        for _ in 0..MAX_PERSISTENCE_RECORDS_PER_TURN {
            if commands == MAX_COMMANDS_PER_TURN {
                break;
            }
            let Some(length) = self.port.take(&mut bytes) else {
                break;
            };
            let decoded = codec::decode(&bytes[..length]);
            if let Ok(message) = &decoded {
                if self
                    .heartbeat
                    .as_mut()
                    .is_some_and(|heartbeat| heartbeat.receive(message))
                {
                    continue;
                }
            }
            let result = match decoded {
                Ok(WireMessage::Client(message)) => {
                    if !matches!(message, ClientMessage::Persistence { .. }) {
                        commands += 1;
                    }
                    if matches!(
                        message,
                        ClientMessage::CancelMedia { .. } | ClientMessage::ReadMedia { .. }
                    ) {
                        self.media = None;
                    }
                    let detached = matches!(message, ClientMessage::Detach { .. });
                    let result = service.handle(self.id, message);
                    if result.is_ok() && detached {
                        self.retire();
                    }
                    result
                }
                _ => Err(Refusal::Invalid),
            };
            if let Err(reason) = result {
                self.replies.push_back(ServiceMessage::Rejected(reason));
                self.retire();
                break;
            }
            self.attached |= service.has_connection(self.id);
        }
    }
    pub fn write(&mut self, service: &mut Service) {
        if let Some(heartbeat) = self.heartbeat.as_mut() {
            if heartbeat.pump(&mut self.port).is_err() {
                self.retire();
                return;
            }
        }
        if let Some(mut network) = self.network.take() {
            network.write(self, service);
            self.network = Some(network);
            return;
        }
        let mut records = 0;
        for _ in 0..2 * MAX_MESSAGES_PER_TURN {
            if records == MAX_MESSAGES_PER_TURN {
                break;
            }
            if self.pending.is_none() {
                if self.semantic.is_none() {
                    let update = self
                        .replies
                        .pop_front()
                        .or_else(|| service.take_update(self.id));
                    if let Some(update) = update {
                        let media = matches!(update, ServiceMessage::Media { .. });
                        self.snapshot = matches!(update, ServiceMessage::Snapshot { .. });
                        let stream = StreamEncoder::new(update, TransferId(self.transfer));
                        self.transfer = self.transfer.saturating_add(1);
                        match stream {
                            Ok(stream) if media => self.media = Some(stream),
                            Ok(stream) => self.semantic = Some(stream),
                            Err(_) => {
                                self.retire();
                                return;
                            }
                        }
                    }
                }
                let from_media = self.media.is_some()
                    && (self.semantic.is_none() || (self.snapshot && records % 4 == 3));
                let stream = if from_media {
                    &mut self.media
                } else {
                    &mut self.semantic
                };
                let Some(encoder) = stream.as_mut() else {
                    break;
                };
                let Some(message) = encoder.next_message() else {
                    *stream = None;
                    continue;
                };
                let mut bytes = [0; MAX_RECORD_BYTES];
                match codec::encode(&message, &mut bytes) {
                    Ok(length) => self.pending = Some(bytes[..length].to_vec()),
                    Err(_) => {
                        self.retire();
                        return;
                    }
                }
            }
            match self.port.post(self.pending.as_ref().unwrap()) {
                Ok(()) => {
                    self.pending = None;
                    records += 1;
                }
                Err(CarrierError::Full) => break,
                Err(_) => {
                    self.retire();
                    break;
                }
            }
        }
    }
    pub fn alive(&mut self, service: &Service) -> bool {
        if self
            .heartbeat
            .as_ref()
            .is_some_and(|heartbeat| heartbeat.expired())
        {
            return false;
        }

        if self
            .closing_since
            .is_some_and(|started| self.age.saturating_sub(started) > Duration::from_secs(2))
        {
            return false;
        }
        if self.port.failure().is_some() {
            return false;
        }
        if self.attached && !service.has_connection(self.id) && !self.closing {
            return false;
        }
        if !self.attached && self.age > Duration::from_secs(3) {
            return false;
        }
        let _ = self.port.flush();
        let drained = self.port.pending_records() == 0;
        !(self.closing
            && self.replies.is_empty()
            && self.semantic.is_none()
            && self.pending.is_none()
            && drained)
    }
}

pub struct Management<P> {
    greeting: Option<WireMessage>,
    pub port: P,
    age: Duration,
    pending: Option<WireMessage>,
    sent: bool,
    settings: Option<(Vec<u8>, usize)>,
    deployment: Option<Vec<u8>>,
}
impl<P: RecordPort> Management<P> {
    pub fn new(port: P) -> Self {
        Self {
            greeting: Some(WireMessage::CarrierReady),
            port,
            age: Duration::ZERO,
            pending: None,
            sent: false,
            settings: None,
            deployment: None,
        }
    }
    pub fn pump(&mut self, service: &mut Service, elapsed: Duration) -> bool {
        self.age = self.age.saturating_add(elapsed);
        if self.age >= Duration::from_secs(5) || self.port.failure().is_some() {
            return false;
        }
        let mut bytes = [0; MAX_RECORD_BYTES];
        if let Some(message) = self.greeting.as_ref() {
            let Ok(n) = codec::encode(message, &mut bytes) else {
                return false;
            };
            match self.port.post(&bytes[..n]) {
                Ok(()) => self.greeting = None,
                Err(CarrierError::Full) => return true,
                Err(_) => return false,
            }
        }
        if self.pending.is_none()
            && self.deployment.is_none()
            && !self.sent
            && self.settings.is_none()
        {
            if let Some(n) = self.port.take(&mut bytes) {
                if shoop_deployment_protocol::is_record(&bytes[..n]) {
                    use shoop_deployment_protocol as deploy;
                    let reply = match deploy::decode_request(&bytes[..n]) {
                        Ok(request) => service.deployment(request),
                        Err(deploy::Error::Version) => {
                            deploy::Reply::Refused(deploy::Refusal::Version)
                        }
                        Err(_) => deploy::Reply::Refused(deploy::Refusal::Invalid),
                    };
                    let Ok(n) = deploy::encode_reply(&reply, &mut bytes) else {
                        return false;
                    };
                    self.deployment = Some(bytes[..n].to_vec());
                } else {
                    let reply = match codec::decode(&bytes[..n]) {
                        Ok(WireMessage::ManagementRequest(ManagementRequest::Settings {
                            version,
                        })) if version == VERSION => {
                            match codec::encode_host_report(&service.host_report()) {
                                Ok(bytes) => {
                                    self.settings = Some((bytes, 0));
                                    ManagementReply::Status {
                                        version: VERSION,
                                        ready: service.is_ready(),
                                        busy: false,
                                        closed: false,
                                    }
                                }
                                Err(_) => ManagementReply::Refused(Refusal::Overloaded),
                            }
                        }
                        Ok(WireMessage::ManagementRequest(request)) => service.management(request),
                        _ => ManagementReply::Refused(Refusal::Invalid),
                    };
                    if self.settings.is_none() {
                        self.pending = Some(WireMessage::ManagementReply(reply));
                    }
                }
            }
        }
        if let Some(bytes) = self.deployment.as_ref() {
            match self.port.post(bytes) {
                Ok(()) => {
                    self.deployment = None;
                    self.sent = true;
                }
                Err(CarrierError::Full) => {}
                Err(_) => return false,
            }
        }
        if let Some((bytes, offset)) = self.settings.as_ref() {
            let end = (*offset + CHUNK_BYTES).min(bytes.len());
            self.pending = Some(WireMessage::ManagementReply(
                ManagementReply::SettingsChunk {
                    offset: *offset as u32,
                    total: bytes.len() as u32,
                    bytes: bytes[*offset..end].to_vec(),
                },
            ));
        }
        if let Some(message) = self.pending.as_ref() {
            let Ok(n) = codec::encode(message, &mut bytes) else {
                return false;
            };
            match self.port.post(&bytes[..n]) {
                Ok(()) => {
                    self.pending = None;
                    if let Some((bytes, offset)) = self.settings.as_mut() {
                        *offset = (*offset + CHUNK_BYTES).min(bytes.len());
                        if *offset == bytes.len() {
                            self.settings = None;
                            self.sent = true;
                        }
                    } else {
                        self.sent = true;
                    }
                }
                Err(CarrierError::Full) => {}
                Err(_) => return false,
            }
        }
        let _ = self.port.flush();
        !(self.sent && self.port.pending_records() == 0)
    }
}

struct Network {
    interval: Duration,
    since: Duration,
    next_snapshot: Option<ServiceMessage>,
    urgent: VecDeque<WireMessage>,
    bulk: VecDeque<WireMessage>,
    force: bool,
}
impl Network {
    fn new(hz: u32) -> Self {
        Self {
            interval: Duration::from_nanos(1_000_000_000_u64.div_ceil(u64::from(hz))),
            since: Duration::ZERO,
            next_snapshot: None,
            urgent: VecDeque::new(),
            bulk: VecDeque::new(),
            force: true,
        }
    }
    fn clear(&mut self) {
        self.next_snapshot = None;
        self.urgent.clear();
        self.bulk.clear();
    }
    fn stream<P: RecordPort>(
        session: &mut Session<P>,
        update: ServiceMessage,
    ) -> Result<StreamEncoder, shoop_transport::CodecError> {
        let id = TransferId(session.transfer);
        session.transfer = session.transfer.saturating_add(1);
        StreamEncoder::new(update, id)
    }
    fn collect<P: RecordPort>(
        &mut self,
        session: &mut Session<P>,
        service: &mut Service,
    ) -> Result<(), shoop_transport::CodecError> {
        for _ in 0..MAX_MESSAGES_PER_TURN {
            if self.urgent.len() + self.bulk.len() >= QUEUE_CAPACITY {
                break;
            }
            let Some(update) = session
                .replies
                .pop_front()
                .or_else(|| service.take_update(session.id))
            else {
                break;
            };
            match update {
                update @ ServiceMessage::Snapshot { .. } => self.next_snapshot = Some(update),
                update @ ServiceMessage::Media { .. } => {
                    session.media = Some(Self::stream(session, update)?)
                }
                update => {
                    let bulk = matches!(update, ServiceMessage::Persistence { .. });
                    self.force |= matches!(
                        update,
                        ServiceMessage::Attached { .. }
                            | ServiceMessage::Finished { .. }
                            | ServiceMessage::ShutdownComplete { .. }
                    );
                    if let Some(message) = Self::stream(session, update)?.next_message() {
                        if bulk {
                            self.bulk.push_back(message);
                        } else {
                            self.urgent.push_back(message);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn snapshot<P: RecordPort>(&mut self, session: &mut Session<P>) -> Option<WireMessage> {
        let message = session.semantic.as_mut()?.next_message();
        if message.is_none() {
            session.semantic = None;
        }
        message
    }
    fn bulk<P: RecordPort>(&mut self, session: &mut Session<P>) -> Option<WireMessage> {
        if let Some(message) = self.bulk.pop_front() {
            return Some(message);
        }
        let message = session.media.as_mut()?.next_message();
        if message.is_none() {
            session.media = None;
        }
        message
    }
    fn write<P: RecordPort>(&mut self, session: &mut Session<P>, service: &mut Service) {
        if self.collect(session, service).is_err() {
            session.retire();
            self.clear();
            return;
        }
        for index in 0..16 {
            if session.port.pending_records() >= 8 {
                break;
            }
            if session.pending.is_none() {
                if session.semantic.is_none()
                    && self.next_snapshot.is_some()
                    && (self.force || self.since >= self.interval)
                {
                    match Self::stream(session, self.next_snapshot.take().unwrap()) {
                        Ok(stream) => session.semantic = Some(stream),
                        Err(_) => {
                            session.retire();
                            self.clear();
                            return;
                        }
                    }
                    self.since = Duration::ZERO;
                    self.force = false;
                }
                let message = self.urgent.pop_front().or_else(|| {
                    if index % 4 == 3 {
                        self.bulk(session).or_else(|| self.snapshot(session))
                    } else {
                        self.snapshot(session).or_else(|| self.bulk(session))
                    }
                });
                let Some(message) = message else {
                    break;
                };
                let mut bytes = [0; MAX_RECORD_BYTES];
                match codec::encode(&message, &mut bytes) {
                    Ok(n) => session.pending = Some(bytes[..n].to_vec()),
                    Err(_) => {
                        session.retire();
                        self.clear();
                        return;
                    }
                }
            }
            match session.port.post(session.pending.as_ref().unwrap()) {
                Ok(()) => session.pending = None,
                Err(CarrierError::Full) => break,
                Err(_) => {
                    session.retire();
                    self.clear();
                    return;
                }
            }
        }
    }
}
