use crate::{
    config::AppTransport,
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

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Endpoint {
    Data,
    Management,
}
pub(crate) fn admission_header(endpoint: Endpoint) -> [u8; 3] {
    let version = shoop_app_protocol::VERSION.to_le_bytes();
    [version[0], version[1], endpoint as u8]
}
pub(crate) fn open(
    entry: &HostTarget,
    address: &str,
    deadline: Instant,
    endpoint: Endpoint,
) -> Result<shoop_transport_socket::Port> {
    if entry.credential.is_some() {
        return open_authenticated(entry, address, deadline);
    }
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
            shoop_transport_socket::setup::connect_lan(*address, config, budget)
        } else {
            shoop_transport_socket::setup::connect(*address, config, budget)
        };
        match result {
            Ok(port) => {
                use std::io::Write;
                let header = admission_header(endpoint);
                let mut written = 0;
                let mut stream = port.stream();
                while written < header.len() {
                    match stream.write(&header[written..]) {
                        Ok(0) => return Err(anyhow!("Host closed during endpoint negotiation")),
                        Ok(count) => written += count,
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                            ) => {}
                        Err(error) => return Err(error.into()),
                    }
                    remaining(deadline)?;
                    if written < header.len() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                return Ok(port);
            }
            Err(error) => last = Some(error),
        }
    }
    Err(last
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow!("no usable endpoint")))
}
fn open_authenticated(
    entry: &HostTarget,
    address: &str,
    deadline: Instant,
) -> Result<shoop_transport_socket::Port> {
    let token = entry.deployment_token()?;
    let addresses = crate::remote::resolve(address, deadline)?;
    let mut last = None;
    for (index, address) in addresses.iter().enumerate() {
        let budget = remaining(deadline)? / (addresses.len() - index) as u32;
        let result = shoop_transport_socket::setup::connect_authenticated(
            *address,
            &token,
            shoop_transport_socket::Config {
                reliable: 8,
                telemetry: 1,
                bulk: 8,
            },
            budget,
        )
        .map_err(anyhow::Error::from);
        match result {
            Ok(port) => return Ok(port),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("no usable embedded endpoint")))
}
fn greeting(port: &mut impl RecordPort, deadline: Instant) -> Result<()> {
    let mut bytes = [0; MAX_RECORD_BYTES];
    loop {
        if let Some(length) = port.take(&mut bytes) {
            return match codec::decode(&bytes[..length])
                .map_err(|_| anyhow!("invalid session greeting"))?
            {
                WireMessage::CarrierReady => Ok(()),
                WireMessage::Rejected(reason) => {
                    Err(anyhow!("session endpoint refused: {reason:?}"))
                }
                _ => Err(anyhow!("unexpected session endpoint greeting")),
            };
        }
        if port.failure().is_some() {
            return Err(anyhow!("session endpoint closed during greeting"));
        }
        remaining(deadline)?;
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn prepare(entry: &impl ConnectionTarget, deadline: Instant) -> Result<HostTarget> {
    let entry = entry.target();
    entry.profile().validate()?;
    if entry.credential.is_some() {
        entry.validate()?;
        return Ok(entry);
    }
    let target = if entry.instance == 0 {
        let mut target = describe(&entry.management, remaining(deadline)?)?;
        target.endpoint = entry.endpoint;
        target.management = entry.management;
        target.network = entry.network;
        target
    } else {
        entry
    };
    target.validate()?;
    Ok(target)
}
pub fn resolve_target(entry: &impl ConnectionTarget, timeout: Duration) -> Result<HostTarget> {
    prepare(entry, Instant::now() + timeout)
}
pub fn connect(entry: &impl ConnectionTarget, timeout: Duration) -> Result<Connection> {
    let deadline = Instant::now() + timeout;
    let entry = prepare(entry, deadline)?;
    let mut port = open(&entry, &entry.endpoint, deadline, Endpoint::Data)
        .context("connect to session data connection")?;
    greeting(&mut port, deadline)?;
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
        let seed = (crate::remote::identity()? as u64).max(1);
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
        if let Err(error) = self.upload.pump(&mut self.port) {
            if error == shoop_app_connection::Error::Disconnected {
                client.disconnect();
            }
            return Err(client_error(error));
        }
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
    let deadline = Instant::now() + timeout;
    let entry = prepare(entry, deadline)?;
    let mut port = open(&entry, &entry.management, deadline, Endpoint::Management)
        .context("connect to session management connection")?;
    greeting(&mut port, deadline)?;
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

pub fn backend_restart_status(
    entry: &impl ConnectionTarget,
    timeout: Duration,
) -> Result<Option<shoop_app_protocol::BackendRestartStatus>> {
    use shoop_app_protocol::VERSION;
    backend_restart_reply(management(
        entry,
        ManagementRequest::BackendRestartStatus { version: VERSION },
        timeout,
    )?)
}

fn backend_restart_reply(
    reply: ManagementReply,
) -> Result<Option<shoop_app_protocol::BackendRestartStatus>> {
    use shoop_app_protocol::Refusal;
    match reply {
        ManagementReply::BackendRestart(status) => Ok(Some(status)),
        ManagementReply::Refused(Refusal::Unsupported) => Ok(None),
        reply => Err(anyhow!("backend restart status refused: {reply:?}")),
    }
}

/// Submit once, then query the accepted operation. Never replay a destructive request after a lost reply.
pub fn restart_backend(
    entry: &impl ConnectionTarget,
    timeout: Duration,
    progress: impl FnMut(shoop_app_protocol::BackendRestartStatus),
) -> Result<()> {
    restart_backend_on(
        timeout,
        |request, budget| management(entry, request, budget),
        progress,
    )
}

fn restart_backend_on(
    timeout: Duration,
    mut request: impl FnMut(ManagementRequest, Duration) -> Result<ManagementReply>,
    mut progress: impl FnMut(shoop_app_protocol::BackendRestartStatus),
) -> Result<()> {
    use shoop_app_protocol::{BackendRestartPhase as Phase, ClientId, OperationId, VERSION};
    let deadline = Instant::now() + timeout;
    let previous = backend_restart_reply(request(
        ManagementRequest::BackendRestartStatus { version: VERSION },
        remaining(deadline)?.min(Duration::from_secs(3)),
    )?)?
    .context("host does not support backend restart")?;
    if previous.phase == Phase::Failed {
        return Err(anyhow!(
            "previous backend restart failed; full device reboot required"
        ));
    }
    let operation = OperationId {
        client: ClientId(crate::remote::identity()?),
        sequence: 1,
    };
    let reply = request(ManagementRequest::RestartBackend {
        version: VERSION, operation, expected_generation: previous.generation,
    }, remaining(deadline)?.min(Duration::from_secs(3)))
        .context("backend restart acknowledgment lost or refused; query status before requesting another restart")?;
    let ManagementReply::BackendRestart(mut status) = reply else {
        return Err(anyhow!("backend restart refused: {reply:?}"));
    };
    loop {
        if status.operation != Some(operation)
            || Some(status.generation) != previous.generation.checked_add(1)
        {
            return Err(anyhow!("backend restart operation was superseded"));
        }
        progress(status);
        match status.phase {
            Phase::Ready => return Ok(()),
            Phase::Failed => {
                return Err(anyhow!(
                    "backend restart failed: {:?}; full device reboot required",
                    status.failure
                ))
            }
            _ => {}
        }
        remaining(deadline)?;
        std::thread::sleep(Duration::from_millis(100));
        status = backend_restart_reply(request(
            ManagementRequest::BackendRestartStatus { version: VERSION },
            remaining(deadline)?.min(Duration::from_secs(3)),
        )?)?
        .context("backend restart capability disappeared")?;
    }
}

/// Request a destructive device reboot through the existing management connection.
/// Success acknowledges the request, not completion; do not automatically retry a lost reply.
pub fn reboot(entry: &impl ConnectionTarget, timeout: Duration) -> Result<()> {
    use shoop_app_protocol::VERSION;
    match management(
        entry,
        ManagementRequest::Reboot { version: VERSION },
        timeout,
    )? {
        ManagementReply::Rebooting { version } if version == VERSION => Ok(()),
        reply => Err(anyhow!("device reboot refused: {reply:?}")),
    }
}

/// Explicitly acquire shutdown authority for an ownerless host. Never evicts a live UI.
pub fn shutdown_ownerless(entry: &impl ConnectionTarget, timeout: Duration) -> Result<()> {
    let entry = entry.target();
    use shoop_app_protocol::{ClientId, OperationId, Outcome, VERSION};
    let operation = OperationId {
        client: ClientId(crate::remote::identity()?),
        sequence: 1,
    };
    let mut request = ManagementRequest::ClaimShutdown {
        version: VERSION,

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

pub fn host_report(
    entry: &impl ConnectionTarget,
    timeout: Duration,
) -> Result<shoop_app_protocol::HostReport> {
    let deadline = Instant::now() + timeout;
    let entry = prepare(entry, deadline)?;
    let mut port = open(&entry, &entry.management, deadline, Endpoint::Management)?;
    greeting(&mut port, deadline)?;
    let mut record = [0; MAX_RECORD_BYTES];
    let n = codec::encode(
        &WireMessage::ManagementRequest(ManagementRequest::Settings {
            version: shoop_app_protocol::VERSION,
        }),
        &mut record,
    )
    .map_err(|_| anyhow!("invalid settings query"))?;
    port.post(&record[..n])
        .map_err(|_| anyhow!("host settings query disconnected"))?;
    let mut bytes = Vec::new();
    let mut expected = None;
    loop {
        if let Some(n) = port.take(&mut record) {
            match codec::decode(&record[..n]).map_err(|_| anyhow!("invalid host settings reply"))? {
                WireMessage::ManagementReply(ManagementReply::SettingsChunk {
                    offset,
                    total,
                    bytes: chunk,
                }) => {
                    anyhow::ensure!(
                        offset as usize == bytes.len() && expected.is_none_or(|size| size == total),
                        "incoherent host settings reply"
                    );
                    if expected.is_none() {
                        bytes.reserve_exact(total as usize);
                        expected = Some(total);
                    }
                    bytes.extend_from_slice(&chunk);
                    if bytes.len() == total as usize {
                        return codec::decode_host_report(&bytes)
                            .map_err(|_| anyhow!("invalid host settings document"));
                    }
                }
                WireMessage::ManagementReply(ManagementReply::Refused(reason)) => {
                    return Err(anyhow!("host settings query refused: {reason:?}"))
                }
                _ => return Err(anyhow!("unexpected host settings reply")),
            }
        }
        if port.failure().is_some() {
            return Err(anyhow!("host settings reply interrupted"));
        }
        remaining(deadline)?;
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Inspect a host-owned socket without admitting a data or management session.
pub fn describe(address: &str, timeout: Duration) -> Result<HostTarget> {
    let deadline = Instant::now() + timeout;
    let addresses = crate::remote::resolve(address, deadline)?;
    let mut last = None;
    for (index, address) in addresses.iter().enumerate() {
        let budget = remaining(deadline)? / (addresses.len() - index) as u32;
        let result = (|| -> Result<HostTarget> {
            let stream = shoop_transport_socket::setup::inspect(*address, budget)?;
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(remaining(deadline)?))?;
            let mut target: HostTarget = crate::launch::read_message(stream)?;
            if !address.ip().is_loopback() {
                for endpoint in [&mut target.endpoint, &mut target.management] {
                    if let Ok(mut socket) = endpoint.parse::<std::net::SocketAddr>() {
                        if socket.ip().is_loopback() {
                            socket.set_ip(address.ip());
                            *endpoint = socket.to_string();
                        }
                    }
                }
            }
            target.validate()?;
            anyhow::ensure!(
                target.process.is_some() && target.instance != 0,
                "Host description lacks process identity"
            );
            Ok(target)
        })();
        match result {
            Ok(target) => return Ok(target),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("No usable host endpoint")))
}
