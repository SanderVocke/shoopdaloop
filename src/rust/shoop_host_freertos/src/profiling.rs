use alloc::vec::Vec;
use core::time::Duration;
use freertos_profiling::{Snapshot, Task, MAX_TASKS};
use profiling_protocol::{broker::Provider, Action, Descriptor, Name, Outcome, Status};

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct TaskFfi {
    state: u32,
    priority: u32,
    base_priority: u32,
    reserved: u32,
    id: u64,
    runtime: u64,
    stack_free_bytes: u64,
    name: [u8; 16],
}
#[derive(Default)]
#[repr(C)]
struct SnapshotFfi {
    captured_us: u64,
    total_runtime: u64,
    collection_us: u64,
    counter_hz: u64,
    count: u32,
    complete: u32,
    counter_bits: u32,
    reserved: u32,
    tasks: [TaskFfi; MAX_TASKS],
}
const _: () = assert!(core::mem::size_of::<TaskFfi>() == 56);
const _: () = assert!(core::mem::size_of::<SnapshotFfi>() == 1840);
extern "C" {
    fn shoop_profile_tasks(output: *mut SnapshotFfi) -> i32;
}
pub(super) struct Tasks {
    generation: u64,
    status: Status,
    elapsed: Duration,
    force: bool,
    pending: Option<Result<Vec<u8>, Outcome>>,
}
impl Tasks {
    pub fn new(epoch: u64) -> Self {
        Self {
            generation: 0,
            status: Status {
                stream: freertos_profiling::stream(epoch),
                requested: false,
                active: false,
                interval_ms: 1000,
                outcome: Outcome::Applied,
                dropped: 0,
                coalesced: 0,
            },
            elapsed: Duration::ZERO,
            force: false,
            pending: None,
        }
    }
}
impl Provider for Tasks {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            stream: self.status.stream,
            name: Name::new("host.freertos.tasks").expect("static name"),
            available: true,
            min_interval_ms: 1000,
            max_interval_ms: 60000,
            snapshot: true,
            inspect: false,
        }
    }
    fn control(&mut self, action: Action) -> Status {
        self.status.outcome = Outcome::Applied;
        match action {
            Action::Subscribe {
                enabled,
                interval_ms,
            } => {
                if enabled && !self.status.active {
                    let Some(generation) = self.generation.checked_add(1) else {
                        self.status.outcome = Outcome::Capacity;
                        return self.status;
                    };
                    self.generation = generation;
                }
                self.status.requested = enabled;
                self.status.active = enabled;
                self.status.interval_ms = interval_ms.clamp(1000, 60000);
                self.elapsed = Duration::ZERO;
                self.force = enabled;
                if !enabled {
                    self.pending = None;
                }
            }
            Action::Snapshot => {
                self.generation = self.generation.max(1);
                self.force = true;
            }
            Action::Discover => {}
            Action::Inspect(_) => self.status.outcome = Outcome::Unsupported,
        }
        self.status
    }
    fn advance(&mut self, elapsed: Duration) {
        self.elapsed = self.elapsed.saturating_add(elapsed);
        if !self.force
            && (!self.status.active
                || self.elapsed < Duration::from_millis(u64::from(self.status.interval_ms)))
        {
            return;
        }
        self.force = false;
        self.elapsed = Duration::ZERO;
        let mut raw = SnapshotFfi::default();
        let valid =
            unsafe { shoop_profile_tasks(&mut raw) } != 0 && raw.count as usize <= MAX_TASKS;
        let mut tasks = Vec::new();
        if tasks
            .try_reserve_exact(raw.count.min(MAX_TASKS as u32) as usize)
            .is_err()
        {
            self.pending = Some(Err(Outcome::Capacity));
            return;
        }
        if valid {
            for t in raw.tasks[..raw.count as usize].iter().filter(|t| t.id != 0) {
                let length = t.name.iter().position(|b| *b == 0).unwrap_or(t.name.len());
                let name = core::str::from_utf8(&t.name[..length]).unwrap_or("<non-UTF8 task>");
                tasks.push(Task {
                    id: t.id,
                    name: Name::new(name).expect("bounded kernel name"),
                    state: t.state.min(5) as u8,
                    priority: t.priority,
                    base_priority: Some(t.base_priority),
                    runtime: Some(t.runtime),
                    stack_free_bytes: Some(t.stack_free_bytes),
                });
            }
        }
        let sample = Snapshot {
            generation: self.generation,
            clock_domain: self.status.stream.epoch,
            captured_us: raw.captured_us,
            collection_us: raw.collection_us,
            total_runtime: (valid && raw.count != 0).then_some(raw.total_runtime),
            counter_bits: if valid { raw.counter_bits as u8 } else { 0 },
            counter_hz: raw.counter_hz,
            complete: valid && raw.complete != 0,
            tasks,
        };
        self.pending =
            Some(profiling_protocol::encode_report(&sample).map_err(|_| Outcome::Capacity));
    }
    fn take_report(&mut self) -> Option<Result<Vec<u8>, Outcome>> {
        self.pending.take()
    }
}
