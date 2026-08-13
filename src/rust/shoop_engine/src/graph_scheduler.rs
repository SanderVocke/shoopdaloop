//! Coalesces graph-change notifications into batched, bounded-latency applies.
//!
//! Rebuilding the processing schedule is expensive -- [`crate::session::Session::apply_graph_changes`]
//! rebuilds the node specs, the schedule and the per-loop MIDI maps, and resizes the
//! scratch buffers -- so doing it once per mutation is wasteful when wiring up a track
//! makes a dozen mutations in a row. This gathers them.
//!
//! It cannot live on the audio thread, because that work allocates. It cannot live only
//! in the mutation path either: whichever mutation happens to be last would have to know
//! it was last. So a small thread owns the timing, and mutators only say "something
//! changed".
//!
//! # Deadline, not idle-debounce
//!
//! The obvious implementation -- push the deadline out on every change -- starves under a
//! steady stream of changes, which is exactly what a session load or an automated test
//! produces. Instead the *first* change arms a deadline and later changes join that batch
//! without moving it. Worst-case staleness is therefore exactly one window, whatever the
//! change rate, and no bound-checking or escape hatch is needed to guarantee it.
//!
//! What makes deferral affordable at all is that a stale graph is not fatal: `Session::process`
//! runs the last-applied schedule and counts the cycle, so the window costs slightly stale
//! routing rather than dropped audio.

use crate::realtime_lock_guard::Mutex;
use std::sync::{Arc, Condvar};
use std::thread;
use std::time::{Duration, Instant};

/// How long changes are gathered before the schedule is rebuilt.
///
/// Short enough that a user never perceives it on a connection they just made, long
/// enough that building a track coalesces into one rebuild rather than a dozen.
pub const DEFAULT_WINDOW: Duration = Duration::from_millis(10);

#[derive(Default)]
struct State {
    /// When the current batch is due. `None` means no change is pending.
    deadline: Option<Instant>,
    /// Bumped by every [`GraphScheduler::arm`], so a change can be identified.
    ///
    /// Counting applies instead is not enough: an apply already in flight when a change
    /// arrives cannot have seen it, so a flusher waiting for "one more apply" could return
    /// having missed exactly the change it was called for.
    dirty_gen: u64,
    /// The highest `dirty_gen` an apply is known to have observed.
    applied_gen: u64,
    /// Applies performed, for the coalescing tests and diagnostics.
    applies: u64,
    stop: bool,
}

struct Shared {
    state: Mutex<State>,
    /// Signals both directions: mutator -> worker that work arrived, worker -> flusher
    /// that an apply completed.
    cv: Condvar,
}

/// Batches graph-change notifications and applies them on a worker thread.
///
/// Dropping this stops the worker and waits for it, so a pending batch is never lost.
pub struct GraphScheduler {
    shared: Arc<Shared>,
    worker: Option<thread::JoinHandle<()>>,
    window: Duration,
}

impl GraphScheduler {
    /// Starts the worker. `apply` is called with no lock held by this module.
    pub fn start(window: Duration, apply: Box<dyn Fn() + Send>) -> Self {
        let _span = tracing::info_span!(
            "engine.graph.scheduler_start",
            window_us = window.as_micros() as u64
        )
        .entered();
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            cv: Condvar::new(),
        });

        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("shoop-graph-apply".to_string())
            .spawn(move || Self::run(worker_shared, apply))
            .expect("spawn graph scheduler thread");

        Self {
            shared,
            worker: Some(worker),
            window,
        }
    }

    fn run(shared: Arc<Shared>, apply: Box<dyn Fn() + Send>) {
        let _worker_span = tracing::info_span!("worker.engine.graph_scheduler").entered();
        loop {
            let covering_gen = {
                let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if state.stop {
                        return;
                    }
                    match state.deadline {
                        None => {
                            state = shared.cv.wait(state).unwrap_or_else(|e| e.into_inner());
                        }
                        Some(deadline) => {
                            let now = Instant::now();
                            if now >= deadline {
                                // Cleared before the apply, not after: a change arriving
                                // while the schedule is being built belongs to the *next*
                                // batch, because the build may not have observed it. The
                                // generation is snapshotted here for the same reason.
                                state.deadline = None;
                                break state.dirty_gen;
                            }
                            let (s, _) = shared
                                .cv
                                .wait_timeout(state, deadline - now)
                                .unwrap_or_else(|e| e.into_inner());
                            state = s;
                        }
                    }
                }
            };

            {
                let _span =
                    tracing::info_span!("engine.graph.apply", generation = covering_gen).entered();
                apply();
            }

            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.applied_gen = state.applied_gen.max(covering_gen);
            state.applies += 1;
            shared.cv.notify_all();
        }
    }

    /// Notes that the graph changed. Cheap, and safe to call redundantly.
    ///
    /// Arms a deadline if none is pending; if one is, this change joins that batch and
    /// the deadline does **not** move.
    pub fn arm(&self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let coalesced = state.deadline.is_some();
        let _span = tracing::trace_span!(
            "engine.graph.arm",
            generation = state.dirty_gen.wrapping_add(1),
            coalesced
        )
        .entered();
        // Always, even when a batch is already pending: this is what tells a later flush
        // that a change exists which no in-flight apply can have seen.
        state.dirty_gen += 1;
        if state.deadline.is_none() {
            state.deadline = Some(Instant::now() + self.window);
            self.shared.cv.notify_all();
        }
    }

    /// Applies any pending batch now and returns once it has landed.
    ///
    /// For the points where a caller needs the schedule to be current before it goes on:
    /// startup, driver activation, and the test suite's "let everything settle" call.
    pub fn flush_blocking(&self) {
        let _span = tracing::info_span!("engine.graph.flush").entered();
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let target = state.dirty_gen;
        if state.applied_gen >= target {
            return;
        }
        // Bring the deadline forward rather than applying inline, so the worker stays the
        // only thread that ever applies and there is no second path to get wrong.
        state.deadline = Some(Instant::now());
        self.shared.cv.notify_all();

        // Waits on the generation, not on an apply count: an apply that was already
        // running cannot have observed `target`, so counting one would return too early.
        while state.applied_gen < target {
            state = self
                .shared
                .cv
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Notifications received since start. Diagnostics and tests.
    pub fn n_arms(&self) -> u64 {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .dirty_gen
    }

    /// Applies performed since start. Diagnostics and tests.
    pub fn n_applies(&self) -> u64 {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .applies
    }
}

impl Drop for GraphScheduler {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.stop = true;
            self.shared.cv.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            // The apply closure upgrades the session's last weak reference. If that is
            // released on this worker, scheduler destruction also runs here; joining the
            // current thread would panic with EDEADLK. Dropping its handle safely detaches
            // the already-stopping worker instead.
            if worker.thread().id() != thread::current().id() {
                let _ = worker.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn counter() -> (Arc<AtomicU32>, Box<dyn Fn() + Send>) {
        let n = Arc::new(AtomicU32::new(0));
        let n2 = Arc::clone(&n);
        (
            n,
            Box::new(move || {
                n2.fetch_add(1, Ordering::Relaxed);
            }),
        )
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn nothing_is_applied_until_something_changes() {
        let (n, apply) = counter();
        let s = GraphScheduler::start(Duration::from_millis(5), apply);
        thread::sleep(Duration::from_millis(30));
        check!(n.load(Ordering::Relaxed) == 0);
        drop(s);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn one_change_is_applied_within_the_window() {
        let (n, apply) = counter();
        let s = GraphScheduler::start(Duration::from_millis(5), apply);
        s.arm();
        thread::sleep(Duration::from_millis(60));
        check!(n.load(Ordering::Relaxed) == 1);
        drop(s);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn many_changes_inside_one_window_produce_one_apply() {
        let (n, apply) = counter();
        let window = Duration::from_millis(50);
        let s = GraphScheduler::start(window, apply);
        for _ in 0..100 {
            s.arm();
        }
        // Force the pending batch and wait for completion instead of relying on the worker
        // receiving CPU within a fixed wall-clock interval on loaded CI hosts.
        s.flush_blocking();
        check!(n.load(Ordering::Relaxed) == 1);
        drop(s);
    }

    /// The property that makes this starvation-free: continued churn must not push the
    /// deadline out. An idle-debounce would replace the first deadline on every arm.
    #[tracy_nextest_capture::tracy_capture_test]
    fn continuous_churn_does_not_postpone_the_deadline() {
        let (n, apply) = counter();
        let s = GraphScheduler::start(Duration::from_secs(30), apply);

        s.arm();
        let first_deadline = s
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .deadline;
        for _ in 0..100 {
            s.arm();
        }
        let deadline_after_churn = s
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .deadline;

        check!(deadline_after_churn == first_deadline);
        s.flush_blocking();
        check!(n.load(Ordering::Relaxed) == 1);
        drop(s);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn flush_applies_immediately_and_waits_for_it() {
        let (n, apply) = counter();
        // A window long enough that only the flush can explain the apply.
        let window = Duration::from_secs(30);
        let s = GraphScheduler::start(window, apply);
        s.arm();

        let start = Instant::now();
        s.flush_blocking();

        check!(n.load(Ordering::Relaxed) == 1);
        check!(start.elapsed() < Duration::from_secs(5));
        drop(s);
    }

    /// A flush must wait for an apply that actually observed the caller's change.
    ///
    /// The trap: count applies instead of generations, and a flush issued while a slow
    /// apply is already running is satisfied by *that* apply -- which started before the
    /// change existed and cannot have seen it. Here the first apply blocks long enough for
    /// a change to be armed behind it.
    #[tracy_nextest_capture::tracy_capture_test]
    fn flush_waits_for_an_apply_that_saw_the_change() {
        let started = Arc::new(AtomicU32::new(0));
        let finished = Arc::new(AtomicU32::new(0));
        let (s2, f2) = (Arc::clone(&started), Arc::clone(&finished));
        let apply: Box<dyn Fn() + Send> = Box::new(move || {
            let first = s2.fetch_add(1, Ordering::SeqCst) == 0;
            if first {
                thread::sleep(Duration::from_millis(150));
            }
            f2.fetch_add(1, Ordering::SeqCst);
        });

        let window = Duration::from_millis(5);
        let s = GraphScheduler::start(window, apply);

        // First batch: let the worker pick it up and get stuck inside apply().
        s.arm();
        while started.load(Ordering::SeqCst) == 0 {
            thread::sleep(Duration::from_millis(1));
        }

        // Second change, arriving while the first apply is mid-flight.
        s.arm();
        s.flush_blocking();

        check!(finished.load(Ordering::SeqCst) >= 2);
        drop(s);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn flush_with_nothing_pending_is_a_no_op() {
        let (n, apply) = counter();
        let s = GraphScheduler::start(Duration::from_millis(5), apply);
        s.flush_blocking();
        check!(n.load(Ordering::Relaxed) == 0);
        drop(s);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn a_pending_batch_is_not_lost_when_the_scheduler_is_dropped() {
        let (n, apply) = counter();
        let window = Duration::from_millis(5);
        let s = GraphScheduler::start(window, apply);
        s.arm();
        s.flush_blocking();
        drop(s);
        check!(n.load(Ordering::Relaxed) == 1);
    }
}
