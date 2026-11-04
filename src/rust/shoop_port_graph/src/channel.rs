use crate::*;
use alloc::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

struct State {
    snapshot: Arc<Snapshot>,
    next: u64,
    inflight: BTreeMap<OperationId, Request>,
    requests: VecDeque<(OperationId, Request)>,
    results: VecDeque<Completion>,
    closed: bool,
    routing: bool,
    bindings: BTreeMap<LocalPortKey, IoBinding>,
}
#[derive(Clone)]
pub struct Control(Arc<Mutex<State>>);
/// Worker half of a bounded control channel. Dropping it closes outstanding operations.
pub struct Worker(Arc<Mutex<State>>);

pub fn channel(
    epoch: u64,
    provider: &str,
    capabilities: Capabilities,
) -> Result<(Control, Worker), Error> {
    if epoch == 0 || !name_valid(provider) || provider.len() > 64 {
        return Err(Error::new(ErrorKind::Invalid, "invalid provider instance"));
    }
    let state = Arc::new(Mutex::new(State {
        snapshot: Arc::new(Snapshot {
            epoch,
            provider: Some(provider.into()),
            capabilities,
            status: Status::Starting,
            ..Default::default()
        }),
        next: 1,
        inflight: BTreeMap::new(),
        requests: VecDeque::new(),
        results: VecDeque::new(),
        closed: false,
        routing: false,
        bindings: BTreeMap::new(),
    }));
    Ok((Control(state.clone()), Worker(state)))
}
impl Control {
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.0.lock().expect("port graph state").snapshot.clone()
    }
    pub fn submit(&self, request: Request) -> Result<OperationId, Error> {
        let mut state = self.0.lock().expect("port graph state");
        if state.closed {
            return Err(Error::new(ErrorKind::Unavailable, "port graph stopped"));
        }
        state.snapshot.check(&request)?;
        if state.inflight.len() == MAX_PENDING || state.next == u64::MAX {
            return Err(Error::new(
                ErrorKind::Busy,
                "port graph operation queue is full",
            ));
        }
        let id = OperationId(state.next);
        state.next += 1;
        state.inflight.insert(id, request.clone());
        state.requests.push_back((id, request));
        Ok(id)
    }
    pub fn take_result(&self) -> Option<Completion> {
        let mut state = self.0.lock().expect("port graph state");
        let result = state.results.pop_front()?;
        state.inflight.remove(&result.id);
        Some(result)
    }
}
impl Controller for Control {
    fn supports_io_binding(&self) -> bool {
        self.0.lock().expect("port graph state").routing
    }
    fn io_binding(&self, key: LocalPortKey) -> Option<IoBinding> {
        self.0
            .lock()
            .expect("port graph state")
            .bindings
            .get(&key)
            .cloned()
    }
    fn snapshot(&mut self) -> Arc<Snapshot> {
        Control::snapshot(self)
    }
    fn submit(&mut self, request: Request) -> Result<OperationId, Error> {
        Control::submit(self, request)
    }
    fn take_result(&mut self) -> Option<Completion> {
        Control::take_result(self)
    }
}
impl Worker {
    pub fn enable_io_bindings(&mut self) {
        self.0.lock().expect("port graph state").routing = true;
    }
    pub fn bind_io(&mut self, key: LocalPortKey, binding: Option<IoBinding>) {
        let mut state = self.0.lock().expect("port graph state");
        if let Some(binding) = binding {
            state.bindings.insert(key, binding);
        } else {
            state.bindings.remove(&key);
        }
    }
    pub fn next_request(&mut self) -> Option<(OperationId, Request)> {
        self.0
            .lock()
            .expect("port graph state")
            .requests
            .pop_front()
    }
    pub fn publish(&mut self, mut snapshot: Snapshot) -> Result<(), Error> {
        let mut state = self.0.lock().expect("port graph state");
        if state.closed {
            return Err(Error::new(ErrorKind::Unavailable, "port graph stopped"));
        }
        snapshot.epoch = state.snapshot.epoch;
        snapshot.provider = state.snapshot.provider.clone();
        snapshot.capabilities = state.snapshot.capabilities;
        snapshot.revision = state.snapshot.revision;
        if snapshot == *state.snapshot {
            return Ok(());
        }
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::Capacity, "graph revision exhausted"))?;
        if !snapshot.valid() {
            return Err(Error::new(ErrorKind::Invalid, "invalid provider snapshot"));
        }
        state.snapshot = Arc::new(snapshot);
        Ok(())
    }
    pub fn complete(&mut self, id: OperationId, result: Result<Outcome, Error>) {
        let mut state = self.0.lock().expect("port graph state");
        if state.inflight.contains_key(&id) && !state.results.iter().any(|result| result.id == id) {
            let result = match result {
                Ok(outcome) => state.snapshot.confirm(&state.inflight[&id], outcome),
                Err(error) if error.valid() => Err(error),
                Err(_) => Err(Error::new(
                    ErrorKind::Backend,
                    "provider returned an invalid error",
                )),
            };
            state.results.push_back(Completion { id, result });
        }
    }
    pub fn close(&mut self) {
        let mut state = self.0.lock().expect("port graph state");
        if state.closed {
            return;
        }
        let mut snapshot = (*state.snapshot).clone();
        snapshot.status = Status::Unavailable;
        snapshot.ports.clear();
        snapshot.links.clear();
        snapshot.publications.clear();
        snapshot.error = Some(Error::new(ErrorKind::Unavailable, "port graph stopped"));
        snapshot.revision = snapshot.revision.saturating_add(1);
        state.snapshot = Arc::new(snapshot);
        state.closed = true;
        state.bindings.clear();
        state.requests.clear();
        let pending: Vec<_> = state
            .inflight
            .keys()
            .copied()
            .filter(|id| !state.results.iter().any(|result| result.id == *id))
            .collect();
        for id in pending {
            state.results.push_back(Completion {
                id,
                result: Err(Error::new(ErrorKind::Unavailable, "port graph stopped")),
            });
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.close();
    }
}
