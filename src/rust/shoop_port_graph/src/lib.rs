//! External port graphs, independent of drivers, engines, applications and carriers.
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    sync::Arc,
    vec::Vec,
};
use core::fmt;

#[cfg(feature = "std")]
mod channel;
#[cfg(feature = "std")]
pub use channel::{channel, Control, Worker};
#[cfg(feature = "testing")]
pub mod testing;

pub const MAX_PORTS: usize = 4096;
pub const MAX_LINKS: usize = 8192;
pub const MAX_PUBLICATIONS: usize = 4096;
pub const MAX_PENDING: usize = 64;

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name(pub u64);
    };
}
// Runtime endpoint identity, scoped to Snapshot::epoch; never reused in that epoch.
identity!(EndpointId);
// Caller-owned logical key, independent of runtime endpoints and display names.
identity!(LocalPortKey);
identity!(OperationId);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DataType {
    Audio,
    Midi,
}
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Capabilities {
    pub discovery: bool,
    pub connections: bool,
    pub publication: bool,
    pub renaming: bool,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Status {
    #[default]
    Unsupported,
    Starting,
    Ready,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ErrorKind {
    Unsupported,
    Unavailable,
    WrongEpoch,
    StalePort,
    Invalid,
    Incompatible,
    Capacity,
    Busy,
    Backend,
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}
impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        let mut message = message.into();
        let mut end = message.len().min(512);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        Self { kind, message }
    }
    pub fn valid(&self) -> bool {
        self.message.len() <= 512
    }
}
impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}
impl core::error::Error for Error {}

pub fn name_valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= 256 && !name.contains('\0')
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Port {
    pub id: EndpointId,
    pub name: String,
    /// Opaque provider-specific selector for future restoration, not a runtime ID.
    pub locator: Option<String>,
    pub data_type: DataType,
    pub direction: Direction,
    pub local: Option<LocalPortKey>,
    pub physical: bool,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Link {
    pub source: EndpointId,
    pub destination: EndpointId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PortSpec {
    pub name: String,
    pub data_type: DataType,
    pub direction: Direction,
}
impl PortSpec {
    pub fn valid(&self) -> bool {
        name_valid(&self.name)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PublicationState {
    Publishing,
    Published(EndpointId),
    Removing(EndpointId),
    Failed(Error),
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Publication {
    pub key: LocalPortKey,
    pub spec: PortSpec,
    pub state: PublicationState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Snapshot {
    pub epoch: u64,
    pub revision: u64,
    pub provider: Option<String>,
    pub capabilities: Capabilities,
    pub status: Status,
    pub ports: Vec<Port>,
    /// Observed links touching local endpoints.
    pub links: Vec<Link>,
    pub publications: Vec<Publication>,
    pub error: Option<Error>,
}
impl Snapshot {
    pub fn port(&self, id: EndpointId) -> Option<&Port> {
        self.ports.iter().find(|port| port.id == id)
    }
    pub fn publication(&self, key: LocalPortKey) -> Option<&Publication> {
        self.publications.iter().find(|p| p.key == key)
    }
    pub fn valid(&self) -> bool {
        if self.ports.len() > MAX_PORTS
            || self.links.len() > MAX_LINKS
            || self.publications.len() > MAX_PUBLICATIONS
            || self
                .provider
                .as_ref()
                .is_some_and(|name| !name_valid(name) || name.len() > 64)
            || self.error.as_ref().is_some_and(|error| !error.valid())
            || (self.status == Status::Ready
                && (self.epoch == 0 || self.revision == 0 || self.provider.is_none()))
            || (self.status != Status::Ready
                && (!self.ports.is_empty()
                    || !self.links.is_empty()
                    || !self.publications.is_empty()))
            || (self.status == Status::Unsupported && self.capabilities != Capabilities::default())
        {
            return false;
        }
        let mut ports = BTreeMap::new();
        let mut locals = BTreeMap::new();
        for port in &self.ports {
            if port.id.0 == 0
                || !name_valid(&port.name)
                || port
                    .locator
                    .as_ref()
                    .is_some_and(|value| !name_valid(value))
                || ports.insert(port.id, port).is_some()
                || port
                    .local
                    .is_some_and(|key| key.0 == 0 || locals.insert(key, port).is_some())
            {
                return false;
            }
        }
        let mut publications = BTreeSet::new();
        for publication in &self.publications {
            if publication.key.0 == 0
                || !publication.spec.valid()
                || !publications.insert(publication.key)
            {
                return false;
            }
            match &publication.state {
                PublicationState::Published(id) | PublicationState::Removing(id) => {
                    let Some(port) = ports.get(id) else {
                        return false;
                    };
                    if port.local != Some(publication.key)
                        || port.data_type != publication.spec.data_type
                        || port.direction != publication.spec.direction
                    {
                        return false;
                    }
                }
                PublicationState::Failed(error) => {
                    if !error.valid() || locals.contains_key(&publication.key) {
                        return false;
                    }
                }
                PublicationState::Publishing => {
                    if locals.contains_key(&publication.key) {
                        return false;
                    }
                }
            }
        }
        if locals.keys().any(|key| !publications.contains(key)) {
            return false;
        }
        let mut links = BTreeSet::new();
        self.links.iter().all(|link| {
            let (Some(source), Some(destination)) =
                (ports.get(&link.source), ports.get(&link.destination))
            else {
                return false;
            };
            source.direction == Direction::Output
                && destination.direction == Direction::Input
                && source.data_type == destination.data_type
                && (source.local.is_some() || destination.local.is_some())
                && links.insert(link)
        })
    }
    pub fn check(&self, request: &Request) -> Result<(), Error> {
        use ErrorKind::*;
        if !request.valid() {
            return Err(Error::new(Invalid, "invalid port graph request"));
        }
        let supported = match &request.operation {
            Operation::Connect { .. } => self.capabilities.connections,
            Operation::Rename { .. } => self.capabilities.renaming,
            Operation::Publish { .. } | Operation::Unpublish { .. } => {
                self.capabilities.publication
            }
        };
        if !supported {
            return Err(Error::new(
                Unsupported,
                "the provider does not support this port graph operation",
            ));
        }
        if self.status != Status::Ready {
            return Err(Error::new(Unavailable, "the port graph is unavailable"));
        }
        if request.epoch != self.epoch {
            return Err(Error::new(WrongEpoch, "the port graph instance changed"));
        }
        match &request.operation {
            Operation::Connect { local, peer, .. } => {
                let local = self
                    .port(*local)
                    .ok_or_else(|| Error::new(StalePort, "local endpoint disappeared"))?;
                let peer = self
                    .port(*peer)
                    .ok_or_else(|| Error::new(StalePort, "peer endpoint disappeared"))?;
                if local.local.is_none() || local.id == peer.id {
                    return Err(Error::new(Invalid, "connection needs a local endpoint"));
                }
                if local.direction == peer.direction || local.data_type != peer.data_type {
                    return Err(Error::new(
                        Incompatible,
                        "incompatible port direction or data type",
                    ));
                }
            }
            Operation::Rename { port, .. } => {
                if self
                    .port(*port)
                    .ok_or_else(|| Error::new(StalePort, "endpoint disappeared"))?
                    .local
                    .is_none()
                {
                    return Err(Error::new(Invalid, "only local endpoints can be renamed"));
                }
            }
            Operation::Publish { key, spec } => {
                if let Some(publication) = self.publication(*key) {
                    if publication.spec != *spec {
                        return Err(Error::new(
                            Invalid,
                            "logical port key already has a different specification",
                        ));
                    }
                    if matches!(
                        publication.state,
                        PublicationState::Publishing | PublicationState::Removing(_)
                    ) {
                        return Err(Error::new(Busy, "publication is changing"));
                    }
                }
            }
            Operation::Unpublish { key } => {
                if let Some(publication) = self.publication(*key) {
                    if matches!(
                        publication.state,
                        PublicationState::Publishing | PublicationState::Removing(_)
                    ) {
                        return Err(Error::new(Busy, "publication is changing"));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn confirm(&self, request: &Request, outcome: Outcome) -> Result<Outcome, Error> {
        self.check(request)?;
        let observed = match (&request.operation, outcome) {
            (
                Operation::Connect {
                    local,
                    peer,
                    connected,
                },
                Outcome::Connected,
            ) => self.connected(*local, *peer) == *connected,
            (
                Operation::Publish { key, .. },
                Outcome::Published {
                    key: observed,
                    port,
                },
            ) => {
                *key == observed
                    && self
                        .publication(*key)
                        .is_some_and(|p| p.state == PublicationState::Published(port))
            }
            (Operation::Rename { port, name }, Outcome::Renamed) => self
                .port(*port)
                .and_then(|p| p.local)
                .and_then(|key| self.publication(key))
                .is_some_and(|p| p.spec.name == *name),
            (Operation::Unpublish { key }, Outcome::Unpublished) => {
                self.publication(*key).is_none()
                    && !self.ports.iter().any(|port| port.local == Some(*key))
            }
            _ => false,
        };
        if observed {
            Ok(outcome)
        } else {
            Err(Error::new(
                ErrorKind::Backend,
                "provider result does not match the observed graph",
            ))
        }
    }

    pub fn connected(&self, local: EndpointId, peer: EndpointId) -> bool {
        self.links.iter().any(|link| {
            (link.source == local && link.destination == peer)
                || (link.source == peer && link.destination == local)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Operation {
    Connect {
        local: EndpointId,
        peer: EndpointId,
        connected: bool,
    },
    Publish {
        key: LocalPortKey,
        spec: PortSpec,
    },
    Rename {
        port: EndpointId,
        name: String,
    },
    Unpublish {
        key: LocalPortKey,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Request {
    pub epoch: u64,
    pub operation: Operation,
}
impl Request {
    pub fn valid(&self) -> bool {
        self.epoch != 0
            && match &self.operation {
                Operation::Connect { local, peer, .. } => local.0 != 0 && peer.0 != 0,
                Operation::Publish { key, spec } => key.0 != 0 && spec.valid(),
                Operation::Rename { port, name } => port.0 != 0 && name_valid(name),
                Operation::Unpublish { key } => key.0 != 0,
            }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConnectionRequest {
    pub epoch: u64,
    pub local: EndpointId,
    pub peer: EndpointId,
    pub connected: bool,
}
impl ConnectionRequest {
    pub fn valid(&self) -> bool {
        self.epoch != 0 && self.local.0 != 0 && self.peer.0 != 0
    }
}
impl From<ConnectionRequest> for Request {
    fn from(value: ConnectionRequest) -> Self {
        Self {
            epoch: value.epoch,
            operation: Operation::Connect {
                local: value.local,
                peer: value.peer,
                connected: value.connected,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Outcome {
    Connected,
    Published { key: LocalPortKey, port: EndpointId },
    Renamed,
    Unpublished,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    pub id: OperationId,
    pub result: Result<Outcome, Error>,
}

/// Opaque host-private engine route for a published endpoint; never serialized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IoBinding {
    pub port: String,
}

/// Control-side interface. Completion confirms an observed state, never permanent connectivity.
pub trait Controller: Send {
    fn supports_io_binding(&self) -> bool {
        false
    }
    fn io_binding(&self, _: LocalPortKey) -> Option<IoBinding> {
        None
    }
    fn snapshot(&mut self) -> Arc<Snapshot>;
    fn submit(&mut self, request: Request) -> Result<OperationId, Error>;
    fn take_result(&mut self) -> Option<Completion>;
}
