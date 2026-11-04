//! Native publication waits for callback ownership acknowledgement.
use crate::jack_graph::{backend, Graph};
use crate::jack_ports::{self, Owned, Slot};
use shoop_port_graph::*;
use std::collections::BTreeMap;

struct Pending {
    id: OperationId,
    request: Request,
    slot: Slot,
    ticket: u64,
    recovery: Option<Error>,
}
pub(crate) struct Publications {
    io: jack_ports::Control,
    slots: BTreeMap<LocalPortKey, Slot>,
    pending: Option<Pending>,
}
impl Publications {
    pub fn new(io: jack_ports::Control, banks: [usize; 4]) -> Self {
        let mut key = 1;
        let mut slots = BTreeMap::new();
        for (kind, count) in banks.into_iter().enumerate() {
            for index in 0..count {
                slots.insert(LocalPortKey(key), Slot { kind, index });
                key += 1;
            }
        }
        Self {
            io,
            slots,
            pending: None,
        }
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn start(
        &mut self,
        client: &jack::Client,
        graph: &mut Graph,
        snapshot: &Snapshot,
        worker: &mut Worker,
        id: OperationId,
        request: Request,
    ) -> Result<bool, Error> {
        let _span =
            tracing::debug_span!("rt_host.jack.publication.start", operation = id.0).entered();
        snapshot.check(&request)?;
        match &request.operation {
            Operation::Publish { key, spec } => {
                if snapshot
                    .publication(*key)
                    .is_some_and(|p| matches!(p.state, PublicationState::Published(_)))
                {
                    return Ok(false);
                }
                if spec.name.trim().is_empty()
                    || spec.name.contains([':', ',', '\0'])
                    || spec.name.len() + client.name().len() + 1 > 255
                {
                    return Err(Error::new(
                        ErrorKind::Invalid,
                        "invalid JACK publication name",
                    ));
                }
                if snapshot.publications.len() >= MAX_PUBLICATIONS
                    && snapshot.publication(*key).is_none()
                {
                    return Err(Error::new(
                        ErrorKind::Capacity,
                        "publication capacity exhausted",
                    ));
                }
                let slot = self.io.allocate(spec)?;
                let owned = match Owned::register(client, spec) {
                    Ok(port) => port,
                    Err(error) => {
                        self.io.release(slot);
                        graph.fail(*key, spec.clone(), error.clone());
                        return Err(error);
                    }
                };
                let port = owned.unowned();
                let Some(native) = shoop_jack::port_id(client, &port) else {
                    let error = Error::new(
                        ErrorKind::Backend,
                        "registered port lost its native identity",
                    );
                    let _ = client.unregister_port(port);
                    self.io.release(slot);
                    graph.fail(*key, spec.clone(), error.clone());
                    return Err(error);
                };
                graph.add(native, *key, spec.clone());
                let ticket = match self.io.stage(slot, Some(owned)) {
                    Ok(ticket) => ticket,
                    Err(error) => {
                        let _ = client.unregister_port(port);
                        self.io.release(slot);
                        graph.fail(*key, spec.clone(), error.clone());
                        return Err(error);
                    }
                };
                self.slots.insert(*key, slot);
                self.pending = Some(Pending {
                    id,
                    request,
                    slot,
                    ticket,
                    recovery: None,
                });
                Ok(true)
            }
            Operation::Unpublish { key } => {
                let Some(slot) = self.slots.get(key).copied() else {
                    graph.forget(*key);
                    worker.bind_io(*key, None);
                    return Ok(false);
                };
                let ticket = self.io.stage(slot, None)?;
                graph.phase(*key, 2);
                self.pending = Some(Pending {
                    id,
                    request,
                    slot,
                    ticket,
                    recovery: None,
                });
                Ok(true)
            }
            _ => Err(Error::new(
                ErrorKind::Invalid,
                "not a publication operation",
            )),
        }
    }
    pub fn poll(
        &mut self,
        client: &jack::Client,
        graph: &mut Graph,
        worker: &mut Worker,
    ) -> Option<(OperationId, Request, Result<(), Error>)> {
        let ack = self.io.completed()?;
        let _span = tracing::debug_span!("rt_host.jack.publication.complete").entered();
        let mut pending = self.pending.take().expect("one staged callback handoff");
        assert_eq!(ack.ticket, pending.ticket);
        if let Some(error) = pending.recovery {
            if let Operation::Unpublish { key } = pending.request.operation {
                graph.phase(key, 0);
            }
            return Some((pending.id, pending.request, Err(error)));
        }
        let result = match &pending.request.operation {
            Operation::Publish { key, .. } => {
                graph.phase(*key, 0);
                worker.bind_io(
                    *key,
                    Some(IoBinding {
                        port: self.io.name(pending.slot).into(),
                    }),
                );
                if let Some(retired) = ack.retired {
                    let _ = client.unregister_port(retired.unowned());
                }
                Ok(())
            }
            Operation::Unpublish { key } => {
                if let Some(retired) = ack.retired {
                    if let Err(error) = client.unregister_port(retired.unowned()) {
                        let error = backend(error);
                        match self.io.stage(pending.slot, Some(retired)) {
                            Ok(ticket) => {
                                pending.ticket = ticket;
                                pending.recovery = Some(error);
                                self.pending = Some(pending);
                                return None;
                            }
                            Err(error) => return Some((pending.id, pending.request, Err(error))),
                        }
                    }
                }
                graph.forget(*key);
                self.slots.remove(key);
                self.io.release(pending.slot);
                worker.bind_io(*key, None);
                Ok(())
            }
            _ => unreachable!(),
        };
        Some((pending.id, pending.request, result))
    }
}
