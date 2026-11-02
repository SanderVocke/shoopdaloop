use alloc::{collections::VecDeque, vec::Vec};
use core::time::Duration;
use shoop_app::{ConnectionId, Service};
use shoop_app_protocol::{codec, stream::StreamEncoder, *};
use shoop_transport::{CarrierError, RecordPort, MAX_RECORD_BYTES};

pub struct Session<P> {
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
    pub fn retire(&mut self) {
        self.closing = true;
        self.closing_since.get_or_insert(self.age);
        self.media = None;
        self.semantic = None;
        self.pending = None;
    }
    pub fn advance(&mut self, elapsed: Duration) {
        self.age = self.age.saturating_add(elapsed);
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
}
impl<P: RecordPort> Management<P> {
    pub fn new(port: P, host: HostId, session: SessionId) -> Self {
        Self {
            greeting: Some(WireMessage::CarrierReady { host, session }),
            port,
            age: Duration::ZERO,
            pending: None,
            sent: false,
        }
    }
    pub fn pump(&mut self, service: &mut Service, elapsed: Duration) -> bool {
        self.age = self.age.saturating_add(elapsed);
        if self.age >= Duration::from_secs(2) || self.port.failure().is_some() {
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
        if self.pending.is_none() && !self.sent {
            if let Some(n) = self.port.take(&mut bytes) {
                let reply = match codec::decode(&bytes[..n]) {
                    Ok(WireMessage::ManagementRequest(request)) => service.management(request),
                    _ => ManagementReply::Refused(Refusal::Invalid),
                };
                self.pending = Some(WireMessage::ManagementReply(reply));
            }
        }
        if let Some(message) = self.pending.as_ref() {
            let Ok(n) = codec::encode(message, &mut bytes) else {
                return false;
            };
            match self.port.post(&bytes[..n]) {
                Ok(()) => {
                    self.pending = None;
                    self.sent = true;
                }
                Err(CarrierError::Full) => {}
                Err(_) => return false,
            }
        }
        let _ = self.port.flush();
        !(self.sent && self.port.pending_records() == 0)
    }
}
