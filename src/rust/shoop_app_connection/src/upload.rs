use shoop_app_protocol::{codec, ClientMessage, WireMessage};
use shoop_transport::{CarrierError, RecordPort, MAX_RECORD_BYTES};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Full,
    Disconnected,
    Protocol,
}
#[derive(Default)]
pub struct Upload {
    upload: Option<(
        shoop_app_protocol::Authority,
        shoop_app_protocol::persistence::Key,
        shoop_app_protocol::persistence_transfer::UploadStream,
    )>,
    upload_pending: Option<ClientMessage>,
    upload_outstanding: usize,
    upload_finished: bool,
    upload_validation: Option<(
        shoop_app_protocol::Authority,
        shoop_app_protocol::persistence::Key,
        shoop_app_protocol::persistence::Target,
        shoop_app_protocol::persistence_image::Validation,
    )>,
}
impl Upload {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn active(&self) -> bool {
        self.upload.is_some() || self.upload_validation.is_some()
    }
    pub fn outstanding(&self) -> usize {
        self.upload_outstanding
    }
    fn clear_upload(&mut self) {
        self.upload = None;
        self.upload_validation = None;
        self.upload_pending = None;
        self.upload_outstanding = 0;
        self.upload_finished = false;
    }
    pub fn send(
        &mut self,
        port: &mut impl RecordPort,
        message: ClientMessage,
    ) -> Result<(), Error> {
        match message {
            ClientMessage::StageImage {
                authority,
                key,
                target,
                image,
            } => {
                if self.upload.is_some() || self.upload_validation.is_some() {
                    return Err(Error::Full);
                }
                let validation =
                    shoop_app_protocol::persistence_image::Validation::for_transfer(image)
                        .map_err(|_| Error::Protocol)?;
                self.upload_validation = Some((authority, key, target, validation));
                return Ok(());
            }
            ClientMessage::CaptureImage {
                authority,
                key,
                target,
            } => {
                return self.send_wire(
                    port,
                    ClientMessage::Persistence {
                        authority,
                        request: shoop_app_protocol::persistence::Request::Capture { key, target },
                    },
                )
            }
            ClientMessage::Persistence { ref request, .. } if matches!(request, shoop_app_protocol::persistence::Request::Cancel { key } if self.upload.as_ref().is_some_and(|(_, staged, _)| staged == key) || self.upload_validation.as_ref().is_some_and(|(_, staged, _, _)| staged == key)) =>
            {
                self.clear_upload();
            }
            ClientMessage::Detach { .. }
            | ClientMessage::Shutdown { .. }
            | ClientMessage::Attach { .. } => {
                self.clear_upload();
            }
            _ => {}
        }
        self.send_wire(port, message)
    }
    fn send_wire(
        &mut self,
        port: &mut impl RecordPort,
        message: ClientMessage,
    ) -> Result<(), Error> {
        let mut bytes = [0; MAX_RECORD_BYTES];
        let length = codec::encode(&WireMessage::Client(message), &mut bytes)
            .map_err(|_| Error::Protocol)?;
        port.post(&bytes[..length]).map_err(|error| match error {
            CarrierError::Full => Error::Full,
            _ => Error::Disconnected,
        })
    }

    pub fn pump(&mut self, port: &mut impl RecordPort) -> Result<(), Error> {
        if let Some((_, _, _, validation)) = self.upload_validation.as_mut() {
            if validation.step(256 * 1024).map_err(|_| Error::Protocol)? {
                let (authority, key, target, validation) = self.upload_validation.take().unwrap();
                let stream =
                    shoop_app_protocol::persistence_transfer::UploadStream::from_validated(
                        validation.finish().map_err(|_| Error::Protocol)?,
                        key,
                        target,
                    )
                    .map_err(|_| Error::Protocol)?;
                self.upload = Some((authority, key, stream));
            }
        }
        for _ in 0..shoop_app_protocol::MAX_PERSISTENCE_RECORDS_PER_TURN {
            if self.upload_finished
                || self.upload_outstanding >= shoop_app_protocol::MAX_PERSISTENCE_RECORDS_PER_TURN
            {
                break;
            }
            if self.upload_pending.is_none() {
                let Some((authority, _, stream)) = self.upload.as_mut() else {
                    break;
                };
                match stream.next_request().map_err(|_| Error::Protocol)? {
                    Some(request) => {
                        self.upload_pending = Some(ClientMessage::Persistence {
                            authority: *authority,
                            request,
                        })
                    }
                    None => {
                        self.upload_finished = true;
                        break;
                    }
                }
            }
            let message = self
                .upload_pending
                .as_ref()
                .expect("pending upload record")
                .clone();
            match self.send_wire(port, message) {
                Ok(()) => {
                    self.upload_pending = None;
                    self.upload_outstanding += 1;
                }
                Err(Error::Full) => break,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    pub fn received(&mut self, message: &WireMessage) {
        if let WireMessage::Persistence { authority, reply } = message {
            if self
                .upload
                .as_ref()
                .is_some_and(|(owner, key, _)| owner == authority && *key == reply.key())
            {
                if matches!(
                    reply,
                    shoop_app_protocol::persistence::Reply::Refused { .. }
                ) {
                    self.clear_upload();
                } else {
                    self.upload_outstanding = self.upload_outstanding.saturating_sub(1);
                }
            }
        }
        if self.upload_finished && self.upload_outstanding == 0 {
            self.clear_upload();
        }
    }
}
