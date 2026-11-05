//! Coordinates logical application ports with an optional external graph provider.
use super::*;
use shoop_port_graph as graph;

pub(super) struct Publication {
    port: PortId,
    operation: Option<graph::OperationId>,
    binding: Option<u64>,
    ready: bool,
    removing: bool,
    error: Option<String>,
}
fn key(id: PortId) -> HostGraphLocalKey {
    HostGraphLocalKey((1 << 32) | id.raw())
}
pub(super) fn complete(
    entries: &mut BTreeMap<HostGraphLocalKey, Publication>,
    id: graph::OperationId,
    result: &core::result::Result<graph::Outcome, graph::Error>,
) {
    for entry in entries
        .values_mut()
        .filter(|entry| entry.operation == Some(id))
    {
        entry.operation = None;
        entry.error = result.as_ref().err().map(|error| error.message.clone());
        if entry.removing && result.is_ok() {
            entry.ready = true;
        }
    }
}
pub(super) fn engine_complete(
    entries: &mut BTreeMap<HostGraphLocalKey, Publication>,
    completion: &BackendMutationCompletion,
) {
    for entry in entries
        .values_mut()
        .filter(|entry| entry.binding == Some(completion.sequence))
    {
        entry.binding = None;
        entry.ready = completion.result.is_ok();
        entry.error = completion.result.as_ref().err().cloned();
    }
}
impl Service {
    pub(super) fn reconcile_graph(&mut self, next: &graph::Snapshot) {
        if !self.dynamic_ports() {
            return;
        }
        for id in self.ports.keys() {
            let Some(local) = self.graph_endpoint(*id) else {
                continue;
            };
            for link in &self.host_graph.links {
                let peer = if link.source == local {
                    link.destination
                } else if link.destination == local {
                    link.source
                } else {
                    continue;
                };
                let Some(previous) = self.host_graph.port(peer) else {
                    continue;
                };
                let Some(locator) = &previous.locator else {
                    continue;
                };
                if next.port(peer).is_none() {
                    self.connection_intent
                        .entry((*id, locator.clone()))
                        .or_insert(Intent {
                            connected: true,
                            pending: None,
                            error: None,
                        });
                }
            }
        }
        for ((_, selector), intent) in &mut self.connection_intent {
            let previous = self
                .host_graph
                .ports
                .iter()
                .find(|port| port.locator.as_deref() == Some(selector));
            if previous.is_none()
                && next
                    .ports
                    .iter()
                    .any(|port| port.locator.as_deref() == Some(selector))
            {
                intent.error = None;
            }
        }
    }
    pub(super) fn dynamic_ports(&self) -> bool {
        self.host_graph_controller
            .as_ref()
            .is_some_and(|controller| controller.supports_io_binding())
    }
    fn publication_ready(&self, id: PortId) -> bool {
        !self.dynamic_ports()
            || self.ports.get(&id).is_some_and(|port| {
                matches!(port.view.owner, ApplicationPortOwner::GlobalFxControl)
            })
            || self
                .publications
                .get(&key(id))
                .is_some_and(|entry| entry.ready && !entry.removing)
    }
    pub(super) fn initial_publications_ready(&self) -> bool {
        self.ports
            .iter()
            .filter(|(_, port)| match port.view.owner {
                ApplicationPortOwner::Track { track_id, .. } => self
                    .tracks
                    .get(&track_id)
                    .is_some_and(|track| track.view.is_sync),
                ApplicationPortOwner::Bus { bus_id } => self
                    .buses
                    .get(&bus_id)
                    .is_some_and(|bus| bus.backend == MASTER_BUS_ID),
                _ => false,
            })
            .all(|(id, _)| self.publication_ready(*id))
    }
    pub(super) fn effect_publications(&self, effect: &Effect) -> Vec<HostGraphLocalKey> {
        if !self.dynamic_ports() {
            return Vec::new();
        }
        self.ports
            .iter()
            .filter_map(|(id, port)| {
                let owned = match (&port.view.owner, effect) {
                    (
                        ApplicationPortOwner::Track { track_id, .. },
                        Effect::TrackCreated(owner, _) | Effect::TrackRemoved(owner),
                    ) => track_id == owner,
                    (
                        ApplicationPortOwner::Bus { bus_id },
                        Effect::BusCreated(owner) | Effect::BusRemoved(owner),
                    ) => bus_id == owner,
                    _ => false,
                };
                owned.then_some(key(*id))
            })
            .collect()
    }
    pub(super) fn advance_publications(&mut self) {
        if !self.dynamic_ports() {
            return;
        }
        let controller = self.host_graph_controller.as_mut().unwrap();
        let epoch = self.host_graph.epoch;
        let mut retired = Vec::new();
        for (key, entry) in &mut self.publications {
            if !self.ports.contains_key(&entry.port) {
                if entry.removing && entry.ready {
                    retired.push(*key);
                    continue;
                }
                if entry.operation.is_none()
                    && entry.binding.is_none()
                    && (!entry.removing || entry.error.is_some())
                {
                    match controller.submit(graph::Request {
                        epoch,
                        operation: graph::Operation::Unpublish { key: *key },
                    }) {
                        Ok(operation) => {
                            entry.operation = Some(operation);
                            if !entry.removing {
                                entry.error = None;
                            }
                            entry.removing = true;
                            entry.ready = false;
                        }
                        Err(error)
                            if matches!(
                                error.kind,
                                graph::ErrorKind::Busy | graph::ErrorKind::Unavailable
                            ) => {}
                        Err(error) => {
                            entry.error = Some(error.message);
                            entry.removing = true;
                            entry.ready = false;
                        }
                    }
                }
            }
        }
        for key in retired {
            self.publications.remove(&key);
        }
        // Replacement may reuse physical names. Retire the previous session first.
        if self
            .publications
            .values()
            .any(|entry| !self.ports.contains_key(&entry.port))
        {
            return;
        }
        for (id, port) in &mut self.ports {
            if port.view.connection_policy != ConnectionPolicy::UserManaged
                || matches!(port.view.owner, ApplicationPortOwner::GlobalFxControl)
            {
                continue;
            }
            let local = key(*id);
            port.view.publication = Some(local);
            let entry = self.publications.entry(local).or_insert(Publication {
                port: *id,
                operation: None,
                binding: None,
                ready: false,
                removing: false,
                error: None,
            });
            if entry.ready
                || entry.operation.is_some()
                || entry.binding.is_some()
                || entry.error.is_some()
            {
                continue;
            }
            if let Some(binding) = controller.io_binding(local) {
                let before = self.backend.mutation_cursor();
                match self
                    .backend
                    .set_port_connected(port.backend, &binding.port, true)
                {
                    Ok(()) => {
                        let sequence = self.backend.mutation_cursor();
                        entry.binding = (sequence > before).then_some(sequence);
                        entry.ready = sequence == before;
                    }
                    Err(error) => entry.error = Some(error.to_string()),
                }
            } else {
                let spec = graph::PortSpec {
                    name: port.view.name.clone(),
                    data_type: port.view.data_type,
                    direction: port.view.direction,
                };
                match controller.submit(graph::Request {
                    epoch,
                    operation: graph::Operation::Publish { key: local, spec },
                }) {
                    Ok(operation) => entry.operation = Some(operation),
                    Err(error)
                        if error.kind == graph::ErrorKind::Busy
                            || error.kind == graph::ErrorKind::Unavailable => {}
                    Err(error) => entry.error = Some(error.message),
                }
            }
        }
        if let Some(error) = self
            .publications
            .values()
            .find(|entry| {
                entry.error.is_some()
                    && self
                        .ports
                        .get(&entry.port)
                        .is_some_and(|port| match port.view.owner {
                            ApplicationPortOwner::Track { track_id, .. } => self
                                .tracks
                                .get(&track_id)
                                .is_some_and(|track| track.view.is_sync),
                            ApplicationPortOwner::Bus { bus_id } => self
                                .buses
                                .get(&bus_id)
                                .is_some_and(|bus| bus.backend == MASTER_BUS_ID),
                            _ => false,
                        })
            })
            .and_then(|entry| entry.error.clone())
        {
            self.init_error = Some(error);
        }
    }
    pub(super) fn graph_endpoint(&self, id: PortId) -> Option<graph::EndpointId> {
        self.host_graph
            .publication(key(id))
            .and_then(|publication| match publication.state {
                graph::PublicationState::Published(id) => Some(id),
                _ => None,
            })
    }
    pub(super) fn graph_host(&self, selector: &str) -> Option<&graph::Port> {
        self.host_graph
            .ports
            .iter()
            .find(|port| port.locator.as_deref() == Some(selector))
    }
    pub(super) fn graph_linked(&self, port: PortId, host: &str) -> bool {
        let Some(local) = self.graph_endpoint(port) else {
            return false;
        };
        let Some(peer) = self.graph_host(host) else {
            return false;
        };
        self.host_graph.links.iter().any(|link| {
            (link.source == local && link.destination == peer.id)
                || (link.destination == local && link.source == peer.id)
        })
    }
    pub(super) fn advance_graph_connections(&mut self) {
        if !self.dynamic_ports() {
            return;
        }
        if self.options.master_auto_connect && !self.master_graph_auto_done {
            let _ = self.connect_master();
        }
        let settled: Vec<_> = self
            .connection_intent
            .iter()
            .filter(|((port, host), intent)| {
                intent.pending.is_none()
                    && intent.error.is_none()
                    && self.graph_host(host).is_some()
                    && self.graph_linked(*port, host) == intent.connected
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in settled {
            self.connection_intent.remove(&key);
        }
        let candidates: Vec<_> = self
            .connection_intent
            .iter()
            .filter(|(key, intent)| {
                intent.pending.is_none()
                    && intent.error.is_none()
                    && !self
                        .graph_connections
                        .values()
                        .any(|candidate| candidate == *key)
                    && self.graph_linked(key.0, &key.1) != intent.connected
            })
            .map(|(key, intent)| (key.clone(), intent.connected))
            .collect();
        for ((port, host), connected) in candidates {
            let Some(local) = self.graph_endpoint(port) else {
                continue;
            };
            let Some(peer) = self.graph_host(&host).map(|port| port.id) else {
                continue;
            };
            let request = graph::ConnectionRequest {
                epoch: self.host_graph.epoch,
                local,
                peer,
                connected,
            };
            match self
                .host_graph_controller
                .as_mut()
                .unwrap()
                .submit(request.into())
            {
                Ok(operation) => {
                    self.graph_connections
                        .insert(operation, (port, host.clone()));
                    self.connection_intent
                        .get_mut(&(port, host))
                        .unwrap()
                        .pending = Some(u64::MAX);
                }
                Err(error) if error.kind == graph::ErrorKind::Busy => break,
                Err(error) => {
                    self.connection_intent.get_mut(&(port, host)).unwrap().error =
                        Some(error.message)
                }
            }
        }
    }
    pub(super) fn completed_operations(&mut self) -> Vec<OperationId> {
        let ids: Vec<_> = self.pending.keys().copied().collect();
        let mut done = Vec::new();
        for id in ids {
            let pending = &self.pending[&id];
            if self
                .composite_structure
                .as_ref()
                .is_some_and(|job| job.operation == Some(id) && !job.done())
            {
                continue;
            }
            if matches!(
                pending.effect,
                Effect::Persistence(_) | Effect::HostGraph(_)
            ) || (self.shutdown_ready && self.shutdown == Some(id))
                || pending.completed.len() as u64
                    != pending
                        .last
                        .checked_sub(pending.first)
                        .map_or(0, |count| count + 1)
            {
                continue;
            }
            if matches!(pending.effect, Effect::SessionPublished) {
                if let Some(error) = self
                    .publications
                    .values()
                    .find_map(|entry| entry.error.clone())
                {
                    self.pending.get_mut(&id).unwrap().error = Some(CommandError {
                        reason: Refusal::Engine,
                        message: error,
                    });
                } else if self
                    .publications
                    .values()
                    .any(|entry| entry.removing || !entry.ready)
                    || self.ports.keys().any(|id| !self.publication_ready(*id))
                    || !self.graph_connections.is_empty()
                    || self.connection_intent.iter().any(|((port, host), intent)| {
                        self.graph_endpoint(*port).is_some()
                            && self.graph_host(host).is_some()
                            && intent.error.is_none()
                            && (intent.pending.is_some()
                                || self.graph_linked(*port, host) != intent.connected)
                    })
                {
                    continue;
                }
            }
            let pending = &self.pending[&id];
            if let Effect::Connection(port, ref host) = pending.effect {
                if let Some(intent) = self.connection_intent.get(&(port, host.clone())) {
                    if let Some(error) = &intent.error {
                        self.pending.get_mut(&id).unwrap().error = Some(CommandError {
                            reason: Refusal::Engine,
                            message: error.clone(),
                        });
                    } else if intent.pending.is_some()
                        || self.graph_linked(port, host) != intent.connected
                    {
                        continue;
                    }
                }
            }
            let pending = &self.pending[&id];
            let created = matches!(
                pending.effect,
                Effect::TrackCreated(..) | Effect::BusCreated(_)
            );
            if created && !pending.rollback && self.dynamic_ports() {
                let error = pending
                    .error
                    .as_ref()
                    .map(|error| error.message.clone())
                    .or_else(|| {
                        pending.publications.iter().find_map(|key| {
                            self.publications
                                .get(key)
                                .and_then(|entry| entry.error.clone())
                        })
                    });
                if let Some(error) = error {
                    let before = self.backend.mutation_cursor();
                    let result = match pending.effect {
                        Effect::TrackCreated(track, _) => {
                            self.backend.remove_track(self.tracks[&track].backend)
                        }
                        Effect::BusCreated(bus) => {
                            self.backend.remove_bus(self.buses[&bus].backend)
                        }
                        _ => unreachable!(),
                    };
                    let pending = self.pending.get_mut(&id).unwrap();
                    pending.error = Some(CommandError {
                        reason: Refusal::Engine,
                        message: error,
                    });
                    if result.is_err() {
                        done.push(id);
                        continue;
                    }
                    pending.rollback = true;
                    pending.first = before.saturating_add(1);
                    pending.last = self.backend.mutation_cursor();
                    pending.completed.clear();
                    continue;
                }
                if !pending
                    .publications
                    .iter()
                    .all(|key| self.publications.get(key).is_some_and(|entry| entry.ready))
                {
                    continue;
                }
            }
            let pending = &self.pending[&id];
            if pending.rollback
                || (pending.error.is_none()
                    && matches!(
                        pending.effect,
                        Effect::TrackRemoved(_) | Effect::BusRemoved(_)
                    ))
            {
                if !pending.retiring {
                    match pending.effect {
                        Effect::TrackRemoved(track) | Effect::TrackCreated(track, _) => {
                            self.forget_track(track)
                        }
                        Effect::BusRemoved(bus) | Effect::BusCreated(bus) => self.forget_bus(bus),
                        _ => {}
                    }
                    self.pending.get_mut(&id).unwrap().retiring = true;
                }
                let pending = &self.pending[&id];
                if let Some(error) = pending.publications.iter().find_map(|key| {
                    self.publications
                        .get(key)
                        .filter(|entry| entry.removing)
                        .and_then(|entry| entry.error.clone())
                }) {
                    self.pending.get_mut(&id).unwrap().error = Some(CommandError {
                        reason: Refusal::Engine,
                        message: error,
                    });
                    done.push(id);
                    continue;
                }
                if pending
                    .publications
                    .iter()
                    .any(|key| self.publications.contains_key(key))
                {
                    continue;
                }
            }
            done.push(id);
        }
        done
    }
}
