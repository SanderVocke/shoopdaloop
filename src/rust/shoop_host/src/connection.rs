use crate::registry::{AppTransport, HostEntry};
use anyhow::{anyhow, Context, Result};
use shoop_app_client::{Client, Error as ClientError};
use shoop_app_protocol::{
    codec, ClientMessage, ManagementReply, ManagementRequest, WireMessage, MAX_MESSAGES_PER_TURN,
};
use shoop_transport::{CarrierError, Lane, RecordPort, MAX_RECORD_BYTES};
use std::time::{Duration, Instant};

pub enum Port {
    Socket(shoop_transport_socket::Port),
    Shared(shoop_transport_shared_memory::Port),
}
impl RecordPort for Port {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.post_on_lane(record, Lane::Reliable)
    }
    fn post_on_lane(&mut self, record: &[u8], lane: Lane) -> Result<(), CarrierError> {
        match self {
            Self::Socket(port) => port.post_on_lane(record, lane),
            Self::Shared(port) => port.post_on_lane(record, lane),
        }
    }
    fn take(&mut self, output: &mut [u8]) -> Option<usize> {
        match self {
            Self::Socket(port) => port.take(output),
            Self::Shared(port) => port.take(output),
        }
    }
    fn failure(&mut self) -> Option<CarrierError> {
        match self {
            Self::Socket(port) => port.failure(),
            Self::Shared(port) => port.failure(),
        }
    }
}

pub fn connect(entry: &HostEntry, timeout: Duration) -> Result<Connection> {
    let token = entry.token()?;
    let mut port =
        shoop_transport_socket::setup::connect(entry.endpoint, &token, Default::default(), timeout)
            .context("authenticate session data connection")?;
    let deadline = Instant::now() + timeout;
    let mut bytes = [0; MAX_RECORD_BYTES];
    loop {
        if let Some(length) = port.take(&mut bytes) {
            match codec::decode(&bytes[..length])
                .map_err(|_| anyhow!("invalid session endpoint greeting"))?
            {
                WireMessage::CarrierReady { host, session }
                    if host == entry.host && session == entry.session =>
                {
                    break
                }
                WireMessage::Rejected(reason) => {
                    return Err(anyhow!("session endpoint refused: {reason:?}"))
                }
                _ => {
                    return Err(anyhow!(
                        "session endpoint identity differs from its registration"
                    ))
                }
            }
        }
        if port.failure().is_some() {
            return Err(anyhow!("session endpoint closed during greeting"));
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("session endpoint greeting timed out"));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let port = match entry.transport {
        AppTransport::Socket => Port::Socket(port),
        AppTransport::SharedMemory => {
            let descriptor = shoop_transport_shared_memory::Descriptor::decode(
                entry
                    .shared_memory
                    .as_deref()
                    .context("missing shared-memory descriptor")?,
            )?;
            let liveness = port.stream().try_clone()?;
            Port::Shared(
                shoop_transport_shared_memory::Port::open(
                    descriptor,
                    shoop_transport_shared_memory::Side::Client,
                )?
                .with_liveness(liveness)?,
            )
        }
    };
    Ok(Connection { port })
}

pub struct Connection {
    pub port: Port,
}
impl Connection {
    pub fn send(&mut self, message: ClientMessage) -> Result<(), ClientError> {
        let mut bytes = [0; MAX_RECORD_BYTES];
        let length = codec::encode(&WireMessage::Client(message), &mut bytes)
            .map_err(|_| ClientError::Protocol)?;
        self.port
            .post(&bytes[..length])
            .map_err(|error| match error {
                CarrierError::Full => ClientError::Full,
                _ => ClientError::Disconnected,
            })
    }
    pub fn poll(&mut self, client: &mut Client) -> Result<usize, ClientError> {
        let mut count = 0;
        let mut bytes = [0; MAX_RECORD_BYTES];
        for _ in 0..MAX_MESSAGES_PER_TURN {
            let Some(length) = self.port.take(&mut bytes) else {
                break;
            };
            let message = codec::decode(&bytes[..length]).map_err(|_| ClientError::Protocol)?;
            client.receive_wire(message)?;
            count += 1;
        }
        if self.port.failure().is_some() {
            client.disconnect();
            return Err(ClientError::Disconnected);
        }
        Ok(count)
    }
}

pub fn management(
    entry: &HostEntry,
    request: ManagementRequest,
    timeout: Duration,
) -> Result<ManagementReply> {
    let token = entry.token()?;
    let mut port = shoop_transport_socket::setup::connect(
        entry.management,
        &token,
        Default::default(),
        timeout,
    )
    .context("authenticate session management connection")?;
    let mut bytes = [0; MAX_RECORD_BYTES];
    let length = codec::encode(&WireMessage::ManagementRequest(request), &mut bytes)
        .map_err(|_| anyhow!("invalid management request"))?;
    port.post(&bytes[..length])
        .map_err(|_| anyhow!("management connection closed before submission"))?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(length) = port.take(&mut bytes) {
            let reply = codec::decode(&bytes[..length])
                .map_err(|_| anyhow!("invalid management response"))?;
            let WireMessage::ManagementReply(reply) = reply else {
                return Err(anyhow!("unexpected management response"));
            };
            return Ok(reply);
        }
        if port.failure().is_some() {
            return Err(anyhow!("session management connection was lost"));
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("session management response timed out"));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
