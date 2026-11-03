use core::time::Duration;
use shoop_app_protocol::{codec, WireMessage};
use shoop_transport::{CarrierError, CodecError, RecordPort, MAX_RECORD_BYTES};

#[derive(Clone, Copy, Debug)]
pub struct HeartbeatConfig {
    pub interval: Duration,
    pub timeout: Duration,
}
impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(5),
        }
    }
}
impl HeartbeatConfig {
    pub fn valid(self) -> bool {
        self.interval >= Duration::from_millis(10)
            && self.interval <= Duration::from_secs(60)
            && self.timeout >= self.interval.saturating_mul(2)
            && self.timeout <= Duration::from_secs(300)
    }
}
pub struct Heartbeat {
    config: HeartbeatConfig,
    age: Duration,
    since_reply: Duration,
    outstanding: Option<u64>,
    next: u64,
    reply: Option<u64>,
    expired: bool,
}
impl Heartbeat {
    pub fn new(config: HeartbeatConfig, seed: u64) -> Result<Self, CarrierError> {
        if !config.valid() || seed == 0 {
            return Err(CarrierError::Codec(CodecError::Invalid));
        }
        Ok(Self {
            config,
            age: Duration::ZERO,
            since_reply: Duration::ZERO,
            outstanding: None,
            next: seed,
            reply: None,
            expired: false,
        })
    }
    pub fn advance(&mut self, elapsed: Duration) {
        self.age = self.age.saturating_add(elapsed);
        self.since_reply = self.since_reply.saturating_add(elapsed);
        if self.outstanding.is_some() && self.age >= self.config.timeout {
            self.expired = true;
        }
        // A permanently full carrier must not hold a lease forever before a probe is sent.
        if self.since_reply >= self.config.timeout.saturating_add(self.config.interval) {
            self.expired = true;
        }
    }
    pub fn expired(&self) -> bool {
        self.expired
    }
    pub fn receive(&mut self, message: &WireMessage) -> bool {
        match message {
            WireMessage::Ping(sequence) => {
                self.reply = Some(*sequence);
                true
            }
            WireMessage::Pong(sequence) => {
                if !self.expired && self.outstanding == Some(*sequence) {
                    self.outstanding = None;
                    self.since_reply = Duration::ZERO;
                    self.age = Duration::ZERO;
                }
                true
            }
            _ => false,
        }
    }
    pub fn pump(&mut self, port: &mut impl RecordPort) -> Result<(), CarrierError> {
        if self.expired {
            return Err(CarrierError::Disconnected);
        }
        let mut bytes = [0; MAX_RECORD_BYTES];
        if let Some(sequence) = self.reply {
            let n = codec::encode(&WireMessage::Pong(sequence), &mut bytes)
                .map_err(CarrierError::Codec)?;
            match port.post(&bytes[..n]) {
                Ok(()) => self.reply = None,
                Err(CarrierError::Full) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        if self.outstanding.is_none() && self.since_reply >= self.config.interval {
            let sequence = self.next;
            let n = codec::encode(&WireMessage::Ping(sequence), &mut bytes)
                .map_err(CarrierError::Codec)?;
            match port.post(&bytes[..n]) {
                Ok(()) => {
                    self.outstanding = Some(sequence);
                    self.age = Duration::ZERO;
                    self.next = self.next.checked_add(1).ok_or(CarrierError::Disconnected)?;
                }
                Err(CarrierError::Full) => {}
                Err(error) => return Err(error),
            }
        }
        port.flush()
    }
}
