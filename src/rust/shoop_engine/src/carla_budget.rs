use crate::carla_scheduling::SchedulingRequest;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{JoinHandle, Thread};
use std::time::{Duration, Instant};

struct State {
    target: libc::pid_t,
    owner: Thread,
    stop: AtomicBool,
    sequence: AtomicU64,
    active: AtomicU64,
    deadline: AtomicU64,
    policy: AtomicI32,
    priority: AtomicI32,
    acknowledged: AtomicU64,
    error: AtomicI32,
    overruns: AtomicU64,
}

pub(crate) struct BudgetMonitor {
    state: Arc<State>,
    thread: Option<JoinHandle<()>>,
}

impl BudgetMonitor {
    pub fn new() -> io::Result<Self> {
        let target = unsafe { libc::syscall(libc::SYS_gettid) } as libc::pid_t;
        if target <= 0 {
            return Err(io::Error::last_os_error());
        }
        let state = Arc::new(State {
            target,
            owner: std::thread::current(),
            stop: AtomicBool::new(false),
            sequence: AtomicU64::new(0),
            active: AtomicU64::new(0),
            deadline: AtomicU64::new(0),
            policy: AtomicI32::new(libc::SCHED_OTHER),
            priority: AtomicI32::new(0),
            acknowledged: AtomicU64::new(0),
            error: AtomicI32::new(0),
            overruns: AtomicU64::new(0),
        });
        let thread_state = Arc::clone(&state);
        let (ready, startup) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("carla-dsp-budget".to_owned())
            .spawn(move || {
                let _owner = std::thread::current();
                if unsafe { libc::sched_getscheduler(0) } != libc::SCHED_OTHER {
                    let _ = ready.send(Err(io::Error::from(io::ErrorKind::PermissionDenied)));
                    return;
                }
                let _ = ready.send(Ok(()));
                monitor(thread_state)
            })?;
        let monitor = Self {
            state,
            thread: Some(thread),
        };
        startup
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
        Ok(monitor)
    }

    pub fn arm(&self, request: SchedulingRequest, deadline: Instant) -> io::Result<BudgetTicket> {
        let encoded = crate::carla_deadline::encode(deadline)?;
        let sequence = self
            .state
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        if sequence == 0 {
            return Err(io::ErrorKind::Other.into());
        }
        self.state.deadline.store(encoded, Ordering::Relaxed);
        self.state.policy.store(request.policy, Ordering::Relaxed);
        self.state
            .priority
            .store(request.priority + 1, Ordering::Relaxed);
        self.state.active.store(sequence, Ordering::Release);
        let wake = self
            .thread
            .as_ref()
            .expect("live budget monitor")
            .thread()
            .clone();
        wake.unpark();
        let ticket = BudgetTicket {
            state: Arc::clone(&self.state),
            sequence,
            wake,
        };
        while self.state.acknowledged.load(Ordering::Acquire) != sequence {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(io::ErrorKind::TimedOut.into());
            };
            std::thread::park_timeout(remaining);
        }
        let error = self.state.error.load(Ordering::Relaxed);
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(ticket)
    }

    pub fn overruns(&self) -> u64 {
        self.state.overruns.load(Ordering::Relaxed)
    }
}

pub(crate) struct BudgetTicket {
    state: Arc<State>,
    sequence: u64,
    wake: Thread,
}

impl Drop for BudgetTicket {
    fn drop(&mut self) {
        let _ = self.state.active.compare_exchange(
            self.sequence,
            0,
            Ordering::Release,
            Ordering::Relaxed,
        );
        self.wake.unpark();
    }
}

impl Drop for BudgetMonitor {
    fn drop(&mut self) {
        // Joining before the DSP thread exits pins its native TID against reuse.
        self.state.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

fn promote(policy: i32, priority: i32) -> io::Result<()> {
    let requested = libc::sched_param {
        sched_priority: priority,
    };
    if unsafe { libc::sched_setscheduler(0, policy, &requested) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut actual = libc::sched_param { sched_priority: 0 };
    if unsafe { libc::sched_getscheduler(0) } != policy
        || unsafe { libc::sched_getparam(0, &mut actual) } != 0
        || actual.sched_priority != priority
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

fn monitor(state: Arc<State>) {
    let mut acknowledged = 0;
    let mut last_overrun = 0;
    while !state.stop.load(Ordering::Acquire) {
        let sequence = state.active.load(Ordering::Acquire);
        if sequence == 0 {
            std::thread::park();
            continue;
        }
        let policy = state.policy.load(Ordering::Relaxed);
        let priority = state.priority.load(Ordering::Relaxed);
        let deadline = crate::carla_deadline::decode(state.deadline.load(Ordering::Relaxed));
        if state.active.load(Ordering::Acquire) != sequence {
            continue;
        }
        if acknowledged != sequence {
            let result = promote(policy, priority);
            state.error.store(
                result
                    .err()
                    .map(|error| error.raw_os_error().unwrap_or(libc::EPERM))
                    .unwrap_or(0),
                Ordering::Relaxed,
            );
            state.acknowledged.store(sequence, Ordering::Release);
            state.owner.unpark();
            acknowledged = sequence;
        }
        if state.error.load(Ordering::Relaxed) != 0 {
            std::thread::park();
            continue;
        }
        let Ok(Some(deadline)) = deadline else {
            std::process::abort();
        };
        if Instant::now() < deadline {
            std::thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
            continue;
        }
        if state.active.load(Ordering::Acquire) != sequence {
            continue;
        }
        let normal = libc::sched_param { sched_priority: 0 };
        if unsafe { libc::sched_setscheduler(state.target, libc::SCHED_OTHER, &normal) } != 0 {
            std::process::abort();
        }
        if last_overrun != sequence {
            state.overruns.fetch_add(1, Ordering::Relaxed);
            last_overrun = sequence;
        }
        // Keep enforcing until scope exit: promotion can race the first expiry check.
        std::thread::park_timeout(Duration::from_micros(100));
    }
    let normal = libc::sched_param { sched_priority: 0 };
    if unsafe { libc::sched_getscheduler(0) } != libc::SCHED_OTHER
        && unsafe { libc::sched_setscheduler(0, libc::SCHED_OTHER, &normal) } != 0
    {
        std::process::abort();
    }
}
