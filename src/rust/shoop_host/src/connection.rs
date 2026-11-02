use crate::{
    registry::AppTransport,
    remote::{remaining, ConnectionTarget, Target as HostTarget},
};
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
    fn flush(&mut self) -> Result<(), CarrierError> {
        match self {
            Self::Socket(p) => p.flush(),
            Self::Shared(_) => Ok(()),
        }
    }
    fn pending_records(&self) -> usize {
        match self {
            Self::Socket(p) => p.pending(),
            Self::Shared(_) => 0,
        }
    }

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

fn open(
    entry: &HostTarget,
    address: &str,
    deadline: Instant,
) -> Result<shoop_transport_socket::Port> {
    let token = entry.token()?;
    let addresses = crate::remote::resolve(address, deadline)?;
    let mut last = None;
    for (index, address) in addresses.iter().enumerate() {
        let budget = remaining(deadline)? / ((addresses.len() - index) as u32);
        let config = if entry.network.is_some() {
            shoop_transport_socket::Config {
                reliable: 8,
                telemetry: 1,
                bulk: 8,
            }
        } else {
            Default::default()
        };
        let result = if entry.network.is_some() {
            shoop_transport_socket::setup::connect_lan(*address, &token, config, budget)
        } else {
            shoop_transport_socket::setup::connect(*address, &token, config, budget)
        };
        match result {
            Ok(port) => return Ok(port),
            Err(error) => last = Some(error),
        }
    }
    Err(last
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow!("no usable endpoint")))
}
fn greeting(port: &mut impl RecordPort, entry: &HostTarget, deadline: Instant) -> Result<()> {
    let mut bytes = [0; MAX_RECORD_BYTES];
    loop {
        if let Some(length) = port.take(&mut bytes) {
            return match codec::decode(&bytes[..length])
                .map_err(|_| anyhow!("invalid session greeting"))?
            {
                WireMessage::CarrierReady { host, session }
                    if host == entry.host && session == entry.session =>
                {
                    Ok(())
                }
                WireMessage::Rejected(reason) => {
                    Err(anyhow!("session endpoint refused: {reason:?}"))
                }
                _ => Err(anyhow!(
                    "session endpoint identity differs from its profile"
                )),
            };
        }
        if port.failure().is_some() {
            return Err(anyhow!("session endpoint closed during greeting"));
        }
        remaining(deadline)?;
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub fn connect(entry: &impl ConnectionTarget, timeout: Duration) -> Result<Connection> {
    let entry = entry.target();
    let deadline = Instant::now() + timeout;
    let mut port =
        open(&entry, &entry.endpoint, deadline).context("authenticate session data connection")?;
    greeting(&mut port, &entry, deadline)?;
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
    let connection = Connection {
        port,
        upload: shoop_app_connection::Upload::new(),
        heartbeat: None,
        last: Instant::now(),
    };
    match entry.network {
        Some(network) => connection.with_heartbeat(network.heartbeat()),
        None => Ok(connection),
    }
}

pub struct Connection {
    pub port: Port,
    upload: shoop_app_connection::Upload,
    heartbeat: Option<shoop_app_connection::Heartbeat>,
    last: Instant,
}
fn client_error(error: shoop_app_connection::Error) -> ClientError {
    match error {
        shoop_app_connection::Error::Full => ClientError::Full,
        shoop_app_connection::Error::Disconnected => ClientError::Disconnected,
        shoop_app_connection::Error::Protocol => ClientError::Protocol,
    }
}
impl Connection {
    pub fn with_heartbeat(mut self, config: shoop_app_connection::HeartbeatConfig) -> Result<Self> {
        let seed = (crate::registry::identity()? as u64).max(1);
        self.heartbeat = Some(
            shoop_app_connection::Heartbeat::new(config, seed)
                .map_err(|_| anyhow!("invalid heartbeat configuration"))?,
        );
        Ok(self)
    }
    pub fn send(&mut self, message: ClientMessage) -> Result<(), ClientError> {
        self.upload
            .send(&mut self.port, message)
            .map_err(client_error)
    }
    pub fn poll(&mut self, client: &mut Client) -> Result<usize, ClientError> {
        let now = Instant::now();
        if let Some(heartbeat) = self.heartbeat.as_mut() {
            heartbeat.advance(now.saturating_duration_since(self.last));
            if heartbeat.expired() {
                client.disconnect();
                return Err(ClientError::Disconnected);
            }
            if heartbeat.pump(&mut self.port).is_err() {
                client.disconnect();
                return Err(ClientError::Disconnected);
            }
        }
        self.last = now;
        self.upload.pump(&mut self.port).map_err(client_error)?;
        let mut count = 0;
        let mut bytes = [0; MAX_RECORD_BYTES];
        for _ in 0..MAX_MESSAGES_PER_TURN {
            let Some(n) = self.port.take(&mut bytes) else {
                break;
            };
            let message = codec::decode(&bytes[..n]).map_err(|_| ClientError::Protocol)?;
            if self
                .heartbeat
                .as_mut()
                .is_some_and(|heartbeat| heartbeat.receive(&message))
            {
                continue;
            }
            self.upload.received(&message);
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

#[cfg(test)]
mod tests;

pub fn management(
    entry: &impl ConnectionTarget,
    request: ManagementRequest,
    timeout: Duration,
) -> Result<ManagementReply> {
    let entry = entry.target();
    let deadline = Instant::now() + timeout;
    let mut port = open(&entry, &entry.management, deadline)
        .context("authenticate session management connection")?;
    greeting(&mut port, &entry, deadline)?;
    let mut bytes = [0; MAX_RECORD_BYTES];
    let length = codec::encode(&WireMessage::ManagementRequest(request), &mut bytes)
        .map_err(|_| anyhow!("invalid management request"))?;
    port.post(&bytes[..length])
        .map_err(|_| anyhow!("management connection closed before submission"))?;
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

/// Explicitly acquire shutdown authority for an ownerless host. Never evicts a live UI.
pub fn shutdown_ownerless(entry: &impl ConnectionTarget, timeout: Duration) -> Result<()> {
    let entry = entry.target();
    use shoop_app_protocol::{ClientId, OperationId, Outcome, VERSION};
    let operation = OperationId {
        client: ClientId(crate::registry::identity()?),
        sequence: 1,
    };
    let mut request = ManagementRequest::ClaimShutdown {
        version: VERSION,
        host: entry.host,
        session: entry.session,
        operation,
    };
    let deadline = Instant::now() + timeout;
    loop {
        match management(
            &entry,
            request,
            remaining(deadline)?.min(Duration::from_secs(2)),
        )? {
            ManagementReply::Accepted {
                authority,
                operation: pending,
            } if pending.id == operation => {
                request = ManagementRequest::Result {
                    authority,
                    operation,
                }
            }
            ManagementReply::Finished { result, .. } if result.id == operation => {
                return match result.outcome {
                    Outcome::Confirmed { .. } => Ok(()),
                    outcome => Err(anyhow!("host cleanup failed: {outcome:?}")),
                };
            }
            reply => return Err(anyhow!("ownerless shutdown refused: {reply:?}")),
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("host cleanup has not completed"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
