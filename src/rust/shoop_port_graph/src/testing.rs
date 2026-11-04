//! Deterministic provider and shared conformance scenarios for driver implementations.
use crate::*;

pub struct FakeProvider {
    control: Control,
    worker: Worker,
    state: Snapshot,
    next_port: u64,
    active: Option<(OperationId, Request)>,
    failure: Option<Error>,
}
impl FakeProvider {
    pub fn new(epoch: u64) -> Self {
        let capabilities = Capabilities {
            discovery: true,
            connections: true,
            publication: true,
            renaming: true,
        };
        let (control, mut worker) = channel(epoch, "fake", capabilities).unwrap();
        let state = Snapshot {
            epoch,
            provider: Some("fake".into()),
            capabilities,
            status: Status::Ready,
            ..Default::default()
        };
        worker.publish(state.clone()).unwrap();
        let mut state = state;
        state.revision = control.snapshot().revision;
        Self {
            control,
            worker,
            state,
            next_port: 100,
            active: None,
            failure: None,
        }
    }
    pub fn control(&self) -> Control {
        self.control.clone()
    }
    pub fn fail_next(&mut self, error: Error) {
        self.failure = Some(error);
    }
    fn publish(&mut self) {
        self.worker.publish(self.state.clone()).unwrap();
        self.state = (*self.control.snapshot()).clone();
    }
    fn id(&mut self) -> EndpointId {
        let id = EndpointId(self.next_port);
        self.next_port += 1;
        id
    }
    pub fn peer(&mut self, spec: PortSpec) -> EndpointId {
        assert!(self.state.ports.len() < MAX_PORTS);
        let id = self.id();
        self.state.ports.push(Port {
            id,
            name: spec.name,
            locator: Some(alloc::format!("peer/{}", id.0)),
            data_type: spec.data_type,
            direction: spec.direction,
            local: None,
        });
        self.publish();
        id
    }
    pub fn rename_peer(&mut self, id: EndpointId, name: &str) {
        self.state
            .ports
            .iter_mut()
            .find(|p| p.id == id && p.local.is_none())
            .unwrap()
            .name = name.into();
        self.publish();
    }
    pub fn remove_peer(&mut self, id: EndpointId) {
        assert!(self.state.port(id).is_some_and(|p| p.local.is_none()));
        self.state.ports.retain(|p| p.id != id);
        self.state
            .links
            .retain(|link| link.source != id && link.destination != id);
        self.publish();
    }
    pub fn stop(&mut self) {
        self.worker.close();
        self.active = None;
    }
    /// One turn admits work and publishes transitions; the following turn completes it.
    pub fn advance(&mut self) {
        if let Some((id, request)) = self.active.take() {
            let result = if let Some(error) = self.failure.take() {
                Err(error)
            } else {
                self.apply(&request)
            };
            if let Err(error) = &result {
                match request.operation {
                    Operation::Publish { key, .. } => {
                        if let Some(publication) =
                            self.state.publications.iter_mut().find(|p| p.key == key)
                        {
                            if matches!(publication.state, PublicationState::Publishing) {
                                publication.state = PublicationState::Failed(error.clone());
                            }
                        }
                    }
                    Operation::Unpublish { key } => {
                        if let Some(publication) =
                            self.state.publications.iter_mut().find(|p| p.key == key)
                        {
                            if let PublicationState::Removing(port) = publication.state {
                                publication.state = PublicationState::Published(port);
                            }
                        }
                    }
                    _ => {}
                }
            }
            self.publish();
            self.worker.complete(id, result);
            return;
        }
        let Some((id, request)) = self.worker.next_request() else {
            return;
        };
        if let Err(error) = self.state.check(&request) {
            self.worker.complete(id, Err(error));
            return;
        }
        match &request.operation {
            Operation::Publish { key, spec }
                if !self
                    .state
                    .publication(*key)
                    .is_some_and(|p| matches!(p.state, PublicationState::Published(_))) =>
            {
                if self.state.publication(*key).is_none() {
                    if self.state.publications.len() == MAX_PUBLICATIONS {
                        self.worker.complete(
                            id,
                            Err(Error::new(
                                ErrorKind::Capacity,
                                "publication capacity exceeded",
                            )),
                        );
                        return;
                    }
                    self.state.publications.push(Publication {
                        key: *key,
                        spec: spec.clone(),
                        state: PublicationState::Publishing,
                    });
                } else {
                    self.state
                        .publications
                        .iter_mut()
                        .find(|p| p.key == *key)
                        .unwrap()
                        .state = PublicationState::Publishing;
                }
            }
            Operation::Unpublish { key } => {
                if let Some(publication) =
                    self.state.publications.iter_mut().find(|p| p.key == *key)
                {
                    if let PublicationState::Published(port) = publication.state {
                        publication.state = PublicationState::Removing(port);
                    }
                }
            }
            _ => {}
        }
        self.active = Some((id, request));
        self.publish();
    }
    fn apply(&mut self, request: &Request) -> Result<Outcome, Error> {
        match &request.operation {
            Operation::Publish { key, spec } => {
                if let Some(Publication {
                    state: PublicationState::Published(port),
                    ..
                }) = self.state.publication(*key)
                {
                    return Ok(Outcome::Published {
                        key: *key,
                        port: *port,
                    });
                }
                if self.state.ports.len() == MAX_PORTS {
                    return Err(Error::new(
                        ErrorKind::Capacity,
                        "endpoint capacity exceeded",
                    ));
                }
                let id = self.id();
                self.state.ports.push(Port {
                    id,
                    name: spec.name.clone(),
                    locator: None,
                    data_type: spec.data_type,
                    direction: spec.direction,
                    local: Some(*key),
                });
                self.state
                    .publications
                    .iter_mut()
                    .find(|p| p.key == *key)
                    .unwrap()
                    .state = PublicationState::Published(id);
                Ok(Outcome::Published {
                    key: *key,
                    port: id,
                })
            }
            Operation::Unpublish { key } => {
                if let Some(port) = self
                    .state
                    .ports
                    .iter()
                    .find(|p| p.local == Some(*key))
                    .map(|p| p.id)
                {
                    self.state.ports.retain(|p| p.id != port);
                    self.state
                        .links
                        .retain(|link| link.source != port && link.destination != port);
                }
                self.state.publications.retain(|p| p.key != *key);
                Ok(Outcome::Unpublished)
            }
            Operation::Rename { port, name } => {
                self.state.check(request)?;
                let endpoint = self.state.ports.iter_mut().find(|p| p.id == *port).unwrap();
                endpoint.name = name.clone();
                let key = endpoint.local.unwrap();
                self.state
                    .publications
                    .iter_mut()
                    .find(|p| p.key == key)
                    .unwrap()
                    .spec
                    .name = name.clone();
                Ok(Outcome::Renamed)
            }
            Operation::Connect {
                local,
                peer,
                connected,
            } => {
                self.state.check(request)?;
                let link = if self.state.port(*local).unwrap().direction == Direction::Output {
                    Link {
                        source: *local,
                        destination: *peer,
                    }
                } else {
                    Link {
                        source: *peer,
                        destination: *local,
                    }
                };
                if *connected {
                    if !self.state.links.contains(&link) {
                        if self.state.links.len() == MAX_LINKS {
                            return Err(Error::new(
                                ErrorKind::Capacity,
                                "connection capacity exceeded",
                            ));
                        }
                        self.state.links.push(link);
                    }
                } else {
                    self.state.links.retain(|existing| *existing != link);
                }
                Ok(Outcome::Connected)
            }
        }
    }
}

pub fn finish(
    control: &mut dyn Controller,
    id: OperationId,
    mut progress: impl FnMut(),
) -> Result<Outcome, Error> {
    for _ in 0..1000 {
        progress();
        if let Some(completion) = control.take_result() {
            assert_eq!(completion.id, id);
            return completion.result;
        }
    }
    panic!("provider did not finish operation {id:?}");
}
pub fn perform(
    control: &mut dyn Controller,
    operation: Operation,
    progress: impl FnMut(),
) -> Result<Outcome, Error> {
    let epoch = control.snapshot().epoch;
    let id = control.submit(Request { epoch, operation })?;
    finish(control, id, progress)
}

/// Run unchanged against real providers and the deterministic provider.
pub fn connection_contract(
    control: &mut dyn Controller,
    local: EndpointId,
    peer: EndpointId,
    incompatible: EndpointId,
    foreign: EndpointId,
    mut progress: impl FnMut(),
) {
    let snapshot = control.snapshot();
    assert!(snapshot.valid());
    assert_eq!(snapshot.status, Status::Ready);
    assert!(snapshot.capabilities.connections);
    let epoch = snapshot.epoch;
    let wrong = Request {
        epoch: epoch.wrapping_add(1).max(1),
        operation: Operation::Connect {
            local,
            peer,
            connected: true,
        },
    };
    assert_eq!(
        control.submit(wrong).unwrap_err().kind,
        ErrorKind::WrongEpoch
    );
    for (target, other, kind) in [
        (local, incompatible, ErrorKind::Incompatible),
        (foreign, peer, ErrorKind::Invalid),
    ] {
        assert_eq!(
            control
                .submit(Request {
                    epoch,
                    operation: Operation::Connect {
                        local: target,
                        peer: other,
                        connected: true
                    }
                })
                .unwrap_err()
                .kind,
            kind
        );
    }
    for connected in [true, true, false, false] {
        assert_eq!(
            perform(
                control,
                Operation::Connect {
                    local,
                    peer,
                    connected
                },
                &mut progress
            )
            .unwrap(),
            Outcome::Connected
        );
        assert_eq!(control.snapshot().connected(local, peer), connected);
    }
    if snapshot.capabilities.renaming {
        assert_eq!(
            perform(
                control,
                Operation::Rename {
                    port: local,
                    name: "contract_renamed".into()
                },
                &mut progress
            )
            .unwrap(),
            Outcome::Renamed
        );
        let renamed = control.snapshot();
        assert_eq!(
            renamed.port(local).unwrap().local,
            snapshot.port(local).unwrap().local
        );
        assert!(renamed.valid());
    }
    if !snapshot.capabilities.publication {
        let spec = PortSpec {
            name: "extra".into(),
            data_type: DataType::Audio,
            direction: Direction::Output,
        };
        assert_eq!(
            control
                .submit(Request {
                    epoch,
                    operation: Operation::Publish {
                        key: LocalPortKey(9999),
                        spec
                    }
                })
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
    }
}
