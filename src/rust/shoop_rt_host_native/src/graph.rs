//! JACK graph queries and mutations run on the driver's control worker.
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

pub const MAX_PORTS: usize = 4096;
pub const MAX_LINKS: usize = 8192;
const MAX_PENDING: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataType {
    Audio,
    Midi,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Input,
    Output,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Port {
    pub name: String,
    pub data_type: DataType,
    pub direction: Direction,
    pub owned: bool,
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Link {
    pub source: String,
    pub destination: String,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Snapshot {
    pub epoch: u64,
    pub revision: u64,
    pub available: bool,
    pub ports: Vec<Port>,
    /// Only connections touching this driver's bank are included.
    pub links: Vec<Link>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("JACK graph is unavailable")]
    Unavailable,
    #[error("JACK graph changed; refresh before retrying")]
    Stale,
    #[error("connection must join an owned bank port and a compatible peer")]
    Invalid,
    #[error("JACK graph operation queue is full")]
    Busy,
    #[error("{0}")]
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct Request {
    pub epoch: u64,
    pub revision: u64,
    pub local: String,
    pub peer: String,
    pub connected: bool,
}

struct State {
    snapshot: Arc<Snapshot>,
    next: u64,
    inflight: usize,
    requests: VecDeque<(u64, Request)>,
    results: VecDeque<(u64, Result<(), Error>)>,
    closed: bool,
}

/// Cloneable, bounded control handle. Never accessed by the process callback.
#[derive(Clone)]
pub struct Control(Arc<Mutex<State>>);
impl Control {
    pub(crate) fn new(epoch: u64) -> Self {
        Self(Arc::new(Mutex::new(State {
            snapshot: Arc::new(Snapshot {
                epoch,
                ..Snapshot::default()
            }),
            next: 1,
            inflight: 0,
            requests: VecDeque::new(),
            results: VecDeque::new(),
            closed: false,
        })))
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.0.lock().expect("graph state").snapshot.clone()
    }
    pub fn submit(&self, request: Request) -> Result<u64, Error> {
        let mut state = self.0.lock().expect("graph state");
        if state.closed || !state.snapshot.available {
            return Err(Error::Unavailable);
        }
        if state.inflight == MAX_PENDING || state.next == u64::MAX {
            return Err(Error::Busy);
        }
        if [&request.local, &request.peer]
            .iter()
            .any(|name| name.is_empty() || name.len() > 256 || name.contains('\0'))
        {
            return Err(Error::Invalid);
        }
        let id = state.next;
        state.next += 1;
        state.inflight += 1;
        state.requests.push_back((id, request));
        Ok(id)
    }
    pub fn take_result(&self) -> Option<(u64, Result<(), Error>)> {
        let mut state = self.0.lock().expect("graph state");
        let result = state.results.pop_front()?;
        state.inflight -= 1;
        Some(result)
    }
    pub(crate) fn next_request(&self) -> Option<(u64, Request)> {
        self.0.lock().expect("graph state").requests.pop_front()
    }
    pub(crate) fn complete(&self, id: u64, result: Result<(), Error>) {
        self.0
            .lock()
            .expect("graph state")
            .results
            .push_back((id, result));
    }
    pub(crate) fn publish(&self, mut snapshot: Snapshot, dirty: bool) {
        let mut state = self.0.lock().expect("graph state");
        snapshot.epoch = state.snapshot.epoch;
        snapshot.revision = state.snapshot.revision;
        if dirty || snapshot != *state.snapshot {
            snapshot.revision = snapshot.revision.saturating_add(1);
            state.snapshot = Arc::new(snapshot);
        }
    }
    pub(crate) fn close(&self) {
        self.publish(Snapshot::default(), false);
        let mut state = self.0.lock().expect("graph state");
        state.closed = true;
        while let Some((id, _)) = state.requests.pop_front() {
            state.results.push_back((id, Err(Error::Unavailable)));
        }
    }
}

pub(crate) fn read(client: &jack::Client) -> Result<Snapshot, Error> {
    let names = client.ports(None, None, jack::PortFlags::empty());
    if names.len() > MAX_PORTS {
        return Err(Error::Failed("JACK graph exceeds the port limit".into()));
    }
    let mut ports = BTreeMap::new();
    for name in names {
        if name.len() > 256 || name.contains('\0') {
            return Err(Error::Failed(
                "JACK port name exceeds protocol bounds".into(),
            ));
        }
        let port = client.port_by_name(&name).ok_or(Error::Stale)?;
        let data_type = match port.port_type().map_err(failure)?.as_str() {
            "32 bit float mono audio" => DataType::Audio,
            "8 bit raw midi" => DataType::Midi,
            _ => continue,
        };
        let flags = port.flags();
        let direction = if flags.contains(jack::PortFlags::IS_INPUT) {
            Direction::Input
        } else if flags.contains(jack::PortFlags::IS_OUTPUT) {
            Direction::Output
        } else {
            continue;
        };
        let owned = name
            .split_once(':')
            .is_some_and(|(owner, _)| owner == client.name());
        ports.insert(
            name.clone(),
            Port {
                name,
                data_type,
                direction,
                owned,
            },
        );
    }
    let mut links = Vec::new();
    for source in ports
        .values()
        .filter(|port| port.direction == Direction::Output)
    {
        let port = client.port_by_name(&source.name).ok_or(Error::Stale)?;
        for destination in port.get_connections() {
            let destination = ports.get(&destination).ok_or(Error::Stale)?;
            if source.owned || destination.owned {
                if links.len() == MAX_LINKS {
                    return Err(Error::Failed(
                        "JACK graph exceeds the connection limit".into(),
                    ));
                }
                links.push(Link {
                    source: source.name.clone(),
                    destination: destination.name.clone(),
                });
            }
        }
    }
    links.sort();
    Ok(Snapshot {
        available: true,
        ports: ports.into_values().collect(),
        links,
        ..Snapshot::default()
    })
}

pub(crate) fn apply(
    client: &jack::Client,
    snapshot: &Snapshot,
    request: &Request,
) -> Result<(), Error> {
    let _span =
        tracing::debug_span!("rt_host.jack.graph.apply", connected = request.connected).entered();
    if !snapshot.available {
        return Err(Error::Unavailable);
    }
    if request.epoch != snapshot.epoch || request.revision != snapshot.revision {
        return Err(Error::Stale);
    }
    let local = snapshot
        .ports
        .iter()
        .find(|port| port.name == request.local)
        .ok_or(Error::Stale)?;
    let peer = snapshot
        .ports
        .iter()
        .find(|port| port.name == request.peer)
        .ok_or(Error::Stale)?;
    if !local.owned
        || local.name == peer.name
        || local.data_type != peer.data_type
        || local.direction == peer.direction
    {
        return Err(Error::Invalid);
    }
    let (source, destination) = if local.direction == Direction::Output {
        (&local.name, &peer.name)
    } else {
        (&peer.name, &local.name)
    };
    let port = client.port_by_name(source).ok_or(Error::Stale)?;
    if port.is_connected_to(destination).map_err(failure)? == request.connected {
        return Ok(());
    }
    if request.connected {
        client
            .connect_ports_by_name(source, destination)
            .map_err(failure)
    } else {
        client
            .disconnect_ports_by_name(source, destination)
            .map_err(failure)
    }
}
fn failure(error: impl std::fmt::Display) -> Error {
    let mut text = error.to_string();
    let mut end = text.len().min(512);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    Error::Failed(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test(no_wasm = "native JACK control queue")]
    fn bounded_queue_reserves_completions_and_close_refuses_queued_operations() {
        let control = Control::new(7);
        control.publish(
            Snapshot {
                available: true,
                ..Default::default()
            },
            false,
        );
        let request = Request {
            epoch: 7,
            revision: 1,
            local: "host:out".into(),
            peer: "peer:in".into(),
            connected: true,
        };
        let ids: Vec<_> = (0..MAX_PENDING)
            .map(|_| control.submit(request.clone()).unwrap())
            .collect();
        let (id, _) = control.next_request().unwrap();
        assert_eq!(control.submit(request.clone()), Err(Error::Busy));
        control.complete(id, Ok(()));
        assert_eq!(control.submit(request.clone()), Err(Error::Busy));
        assert_eq!(control.take_result(), Some((id, Ok(()))));
        let last = control.submit(request.clone()).unwrap();
        control.close();
        assert_eq!(control.submit(request), Err(Error::Unavailable));
        assert!(!control.snapshot().available);
        let results: Vec<_> = std::iter::from_fn(|| control.take_result()).collect();
        assert_eq!(results.len(), MAX_PENDING);
        assert_eq!(results.last().unwrap().0, last);
        assert_eq!(results[0].0, ids[1]);
        assert!(results
            .iter()
            .all(|(_, result)| *result == Err(Error::Unavailable)));
    }
}
