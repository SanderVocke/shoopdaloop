//! JACK's native identities and graph operations stay inside this provider.
use shoop_port_graph::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(crate) const CAPABILITIES: Capabilities = Capabilities {
    discovery: true,
    connections: true,
    publication: true,
    renaming: true,
};
const NATIVE_PORTS: usize = 65536;

pub(crate) struct Generations {
    ports: Box<[AtomicU64]>,
    overflow: AtomicBool,
}
impl Generations {
    pub fn new() -> Self {
        Self {
            ports: (0..NATIVE_PORTS).map(|_| AtomicU64::new(0)).collect(),
            overflow: AtomicBool::new(false),
        }
    }
    pub fn changed(&self, id: jack::PortId) {
        if let Some(generation) = self.ports.get(id as usize) {
            generation.fetch_add(1, Ordering::Release);
        } else {
            self.overflow.store(true, Ordering::Release);
        }
    }
    fn get(&self, id: jack::PortId) -> Result<u64, Error> {
        if self.overflow.load(Ordering::Acquire) {
            return Err(Error::new(
                ErrorKind::Capacity,
                "JACK registration index exceeds the provider limit",
            ));
        }
        self.ports
            .get(id as usize)
            .map(|generation| generation.load(Ordering::Acquire))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::Capacity,
                    "JACK port index exceeds the provider limit",
                )
            })
    }
}

pub(crate) struct Graph {
    next: u64,
    identities: BTreeMap<(jack::PortId, u64), EndpointId>,
    owned: BTreeMap<jack::PortId, (LocalPortKey, PortSpec)>,
    live: BTreeMap<EndpointId, (jack::PortId, u64)>,
    stages: BTreeMap<LocalPortKey, u8>,
    failed: BTreeMap<LocalPortKey, (PortSpec, Error)>,
}
impl Graph {
    pub fn new() -> Self {
        Self {
            next: 1,
            identities: BTreeMap::new(),
            owned: BTreeMap::new(),
            live: BTreeMap::new(),
            stages: BTreeMap::new(),
            failed: BTreeMap::new(),
        }
    }
    pub fn add(&mut self, native: jack::PortId, key: LocalPortKey, spec: PortSpec) {
        self.failed.remove(&key);
        self.owned.insert(native, (key, spec));
        self.stages.insert(key, 1);
    }
    pub fn phase(&mut self, key: LocalPortKey, phase: u8) {
        self.stages.insert(key, phase);
    }
    pub fn forget(&mut self, key: LocalPortKey) {
        self.owned.retain(|_, (owned, _)| *owned != key);
        self.stages.remove(&key);
        self.failed.remove(&key);
    }
    pub fn fail(&mut self, key: LocalPortKey, spec: PortSpec, error: Error) {
        self.forget(key);
        self.failed.insert(key, (spec, error));
    }
    pub fn read(
        &mut self,
        client: &jack::Client,
        generations: &Generations,
    ) -> Result<Snapshot, Error> {
        let names = client.ports(None, None, jack::PortFlags::empty());
        if names.len() > MAX_PORTS {
            return Err(Error::new(
                ErrorKind::Capacity,
                "JACK graph exceeds the port limit",
            ));
        }
        let mut ports = BTreeMap::new();
        let mut native = BTreeMap::new();
        let mut live = BTreeMap::new();
        let mut publications: Vec<_> = self
            .failed
            .iter()
            .map(|(key, (spec, error))| Publication {
                key: *key,
                spec: spec.clone(),
                state: PublicationState::Failed(error.clone()),
            })
            .collect();
        let mut hidden = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for name in names {
            if !name_valid(&name) {
                return Err(Error::new(
                    ErrorKind::Capacity,
                    "JACK name exceeds graph bounds",
                ));
            }
            let port = client.port_by_name(&name).ok_or_else(stale)?;
            let data_type = match port.port_type().map_err(backend)?.as_str() {
                "32 bit float mono audio" => DataType::Audio,
                "8 bit raw midi" => DataType::Midi,
                _ => continue,
            };
            let direction = if port.flags().contains(jack::PortFlags::IS_INPUT) {
                Direction::Input
            } else {
                Direction::Output
            };
            let native_id = shoop_jack::port_id(client, &port).ok_or_else(stale)?;
            if let Some((key, spec)) = self.owned.get(&native_id) {
                if self.stages.get(key) == Some(&1) {
                    hidden.insert(name);
                    publications.push(Publication {
                        key: *key,
                        spec: spec.clone(),
                        state: PublicationState::Publishing,
                    });
                    continue;
                }
            }
            let generation = generations.get(native_id)?;
            let identity = (native_id, generation);
            seen.insert(identity);
            let id = if let Some(id) = self.identities.get(&identity) {
                *id
            } else {
                let id = EndpointId(self.next);
                self.next = self.next.checked_add(1).ok_or_else(|| {
                    Error::new(ErrorKind::Capacity, "endpoint identity space exhausted")
                })?;
                self.identities.insert(identity, id);
                id
            };
            let local = self.owned.get(&native_id).map(|(key, _)| *key);
            if let Some(key) = local {
                publications.push(Publication {
                    key,
                    spec: self.owned[&native_id].1.clone(),
                    state: if self.stages.get(&key) == Some(&2) {
                        PublicationState::Removing(id)
                    } else {
                        PublicationState::Published(id)
                    },
                });
            }
            ports.insert(
                name.clone(),
                Port {
                    id,
                    name: name.clone(),
                    locator: Some(if local.is_some() {
                        format!("local/{}", self.owned[&native_id].1.name)
                    } else {
                        name
                    }),
                    data_type,
                    direction,
                    local,
                    physical: port.flags().contains(jack::PortFlags::IS_PHYSICAL),
                },
            );
            native.insert(id, port);
            live.insert(id, identity);
        }
        let mut links = Vec::new();
        for source in ports
            .values()
            .filter(|port| port.direction == Direction::Output)
        {
            for destination in native[&source.id].get_connections() {
                if hidden.contains(&destination) {
                    continue;
                }
                let destination = ports.get(&destination).ok_or_else(stale)?;
                if source.local.is_some() || destination.local.is_some() {
                    if links.len() == MAX_LINKS {
                        return Err(Error::new(
                            ErrorKind::Capacity,
                            "JACK graph exceeds the connection limit",
                        ));
                    }
                    links.push(Link {
                        source: source.id,
                        destination: destination.id,
                    });
                }
            }
        }
        links.sort();
        publications.sort_by_key(|publication| publication.key);
        self.live = live;
        self.identities
            .retain(|identity, _| seen.contains(identity));
        Ok(Snapshot {
            status: Status::Ready,
            ports: ports.into_values().collect(),
            links,
            publications,
            ..Default::default()
        })
    }
    fn resolve(
        &self,
        client: &jack::Client,
        generations: &Generations,
        id: EndpointId,
    ) -> Result<jack::Port<jack::Unowned>, Error> {
        let &(native, generation) = self.live.get(&id).ok_or_else(stale)?;
        if generations.get(native)? != generation {
            return Err(stale());
        }
        client.port_by_id(native).ok_or_else(stale)
    }
    pub fn apply(
        &mut self,
        client: &jack::Client,
        generations: &Generations,
        snapshot: &Snapshot,
        request: &Request,
    ) -> Result<Outcome, Error> {
        snapshot.check(request)?;
        let _span = tracing::debug_span!("rt_host.jack.graph.apply").entered();
        match &request.operation {
            Operation::Connect {
                local,
                peer,
                connected,
            } => {
                let local_port = self.resolve(client, generations, *local)?;
                let peer_port = self.resolve(client, generations, *peer)?;
                let (source, destination) =
                    if snapshot.port(*local).unwrap().direction == Direction::Output {
                        (
                            local_port.name().map_err(backend)?,
                            peer_port.name().map_err(backend)?,
                        )
                    } else {
                        (
                            peer_port.name().map_err(backend)?,
                            local_port.name().map_err(backend)?,
                        )
                    };
                if local_port
                    .is_connected_to(&peer_port.name().map_err(backend)?)
                    .map_err(backend)?
                    != *connected
                {
                    if *connected {
                        client
                            .connect_ports_by_name(&source, &destination)
                            .map_err(backend)?;
                    } else {
                        client
                            .disconnect_ports_by_name(&source, &destination)
                            .map_err(backend)?;
                    }
                }
                Ok(Outcome::Connected)
            }
            Operation::Rename { port, name } => {
                if name.len() + client.name().len() + 1 > 255
                    || name.contains([':', ',', '\0'])
                    || name.trim().is_empty()
                {
                    return Err(Error::new(
                        ErrorKind::Invalid,
                        "invalid JACK short port name",
                    ));
                }
                let mut native_port = self.resolve(client, generations, *port)?;
                native_port.set_name(name).map_err(backend)?;
                self.owned
                    .get_mut(&shoop_jack::port_id(client, &native_port).ok_or_else(stale)?)
                    .ok_or_else(stale)?
                    .1
                    .name = name.clone();
                Ok(Outcome::Renamed)
            }
            Operation::Publish { .. } | Operation::Unpublish { .. } => Err(Error::new(
                ErrorKind::Unsupported,
                "JACK publication requires a prepared callback binding",
            )),
        }
    }
    pub fn outcome(snapshot: &Snapshot, request: &Request) -> Result<Outcome, Error> {
        match &request.operation {
            Operation::Publish { key, .. } => match snapshot.publication(*key).map(|p| &p.state) {
                Some(PublicationState::Published(port)) => Ok(Outcome::Published {
                    key: *key,
                    port: *port,
                }),
                _ => Err(Error::new(
                    ErrorKind::Unavailable,
                    "publication was not observed",
                )),
            },
            Operation::Unpublish { .. } => Ok(Outcome::Unpublished),
            Operation::Connect { .. } => Ok(Outcome::Connected),
            Operation::Rename { .. } => Ok(Outcome::Renamed),
        }
    }
    pub fn confirm(
        snapshot: &Snapshot,
        request: &Request,
        outcome: Outcome,
    ) -> Result<Outcome, Error> {
        if snapshot.status != Status::Ready {
            return Err(Error::new(
                ErrorKind::Unavailable,
                "JACK graph is unavailable",
            ));
        }
        let observed = match &request.operation {
            Operation::Connect {
                local,
                peer,
                connected,
            } => {
                snapshot.port(*local).is_some()
                    && snapshot.port(*peer).is_some()
                    && snapshot.connected(*local, *peer) == *connected
            }
            Operation::Rename { port, name } => snapshot.port(*port).is_some_and(|p| {
                p.name
                    .rsplit_once(':')
                    .is_some_and(|(_, short)| short == name)
            }),
            Operation::Publish { key, .. } => snapshot
                .publication(*key)
                .is_some_and(|p| matches!(p.state, PublicationState::Published(_))),
            Operation::Unpublish { key } => snapshot.publication(*key).is_none(),
        };
        if observed {
            Ok(outcome)
        } else {
            Err(Error::new(
                ErrorKind::Backend,
                "JACK did not retain the requested graph state",
            ))
        }
    }
}
pub(crate) fn backend(error: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::Backend, error.to_string())
}
fn stale() -> Error {
    Error::new(
        ErrorKind::StalePort,
        "JACK endpoint disappeared or was replaced",
    )
}
