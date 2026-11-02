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
    Ok(Connection {
        port,
        upload: None,
        upload_validation: None,
        upload_pending: None,
    })
}

pub struct Connection {
    pub port: Port,
    upload: Option<(
        shoop_app_protocol::Authority,
        shoop_app_protocol::persistence::Key,
        shoop_app_protocol::persistence_transfer::UploadStream,
    )>,
    upload_pending: Option<ClientMessage>,
    upload_validation: Option<(
        shoop_app_protocol::Authority,
        shoop_app_protocol::persistence::Key,
        shoop_app_protocol::persistence::Target,
        shoop_app_protocol::persistence_image::Validation,
    )>,
}
impl Connection {
    pub fn send(&mut self, message: ClientMessage) -> Result<(), ClientError> {
        match message {
            ClientMessage::StageImage {
                authority,
                key,
                target,
                image,
            } => {
                if self.upload.is_some() || self.upload_validation.is_some() {
                    return Err(ClientError::Full);
                }
                let validation =
                    shoop_app_protocol::persistence_image::Validation::for_transfer(image)
                        .map_err(|_| ClientError::Protocol)?;
                self.upload_validation = Some((authority, key, target, validation));
                return Ok(());
            }
            ClientMessage::CaptureImage {
                authority,
                key,
                target,
            } => {
                return self.send_wire(ClientMessage::Persistence {
                    authority,
                    request: shoop_app_protocol::persistence::Request::Capture { key, target },
                })
            }
            ClientMessage::Persistence { ref request, .. } if matches!(request, shoop_app_protocol::persistence::Request::Cancel { key } if self.upload.as_ref().is_some_and(|(_, staged, _)| staged == key) || self.upload_validation.as_ref().is_some_and(|(_, staged, _, _)| staged == key)) =>
            {
                self.upload = None;
                self.upload_validation = None;
                self.upload_pending = None;
            }
            ClientMessage::Detach { .. }
            | ClientMessage::Shutdown { .. }
            | ClientMessage::Attach { .. } => {
                self.upload = None;
                self.upload_validation = None;
                self.upload_pending = None;
            }
            _ => {}
        }
        self.send_wire(message)
    }
    fn send_wire(&mut self, message: ClientMessage) -> Result<(), ClientError> {
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
        if let Some((_, _, _, validation)) = self.upload_validation.as_mut() {
            if validation
                .step(256 * 1024)
                .map_err(|_| ClientError::Protocol)?
            {
                let (authority, key, target, validation) = self.upload_validation.take().unwrap();
                let stream =
                    shoop_app_protocol::persistence_transfer::UploadStream::from_validated(
                        validation.finish().map_err(|_| ClientError::Protocol)?,
                        key,
                        target,
                    )
                    .map_err(|_| ClientError::Protocol)?;
                self.upload = Some((authority, key, stream));
            }
        }
        for _ in 0..shoop_app_protocol::MAX_COMMANDS_PER_TURN {
            if self.upload_pending.is_none() {
                let Some((authority, _, stream)) = self.upload.as_mut() else {
                    break;
                };
                match stream.next_request().map_err(|_| ClientError::Protocol)? {
                    Some(request) => {
                        self.upload_pending = Some(ClientMessage::Persistence {
                            authority: *authority,
                            request,
                        })
                    }
                    None => {
                        self.upload = None;
                        break;
                    }
                }
            }
            let message = self
                .upload_pending
                .as_ref()
                .expect("pending upload record")
                .clone();
            match self.send_wire(message) {
                Ok(()) => self.upload_pending = None,
                Err(ClientError::Full) => break,
                Err(error) => return Err(error),
            }
        }
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

/// Explicitly acquire shutdown authority for an ownerless host. Never evicts a live UI.
pub fn shutdown_ownerless(entry: &HostEntry, timeout: Duration) -> Result<()> {
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
        match management(entry, request, Duration::from_secs(2))? {
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
