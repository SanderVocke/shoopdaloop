//! The boundary between the control thread and the audio thread.
//!
//! A real driver calls its process callback on a thread it owns, so the session has
//! to live there and cannot be touched from outside. Control operations are queued
//! which queues `std::function<void()>` and drains it from `PROC_handle_command_queue`.
//!
//! Two things that queue has to get right and are easy to miss:
//!
//! - **Boxes are sent back, not dropped here.** Freeing is as forbidden on the audio
//!   thread as allocating, and `assert_no_alloc` catches both. So an executed command
//!   is pushed to a return queue and dropped by whoever owns the handle.
//! - **The return queue is as large as the command queue.** At most `capacity`
//!   commands can be in flight, so returning one can never fail, and the audio
//!   thread never has to choose between leaking and freeing.
//!
//! Commands are `FnMut` rather than `FnOnce` because they are called through the box
//! and the box then has to survive to be sent back. Each is called exactly once.

use crate::composite_plan::{CompiledCompositePlan, LoopIdentity};
use crate::composite_timeline::{
    BoundaryTargetAction, BoundaryTraceEntry, CompositeBoundaryTimeline,
    CompositeTimelineControlError, CompositeTimelineCounters, CompositeTimelineFaultRecord,
};
use crate::loop_mode::LoopMode;
use crate::session::{ReclaimedCompositeTimeline, RejectedCompositeTimeline, Session};

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;

/// A unit of control work, run on the audio thread between cycles.
pub type Command = Box<dyn FnMut(&mut Session) + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommandSequence(u64);

impl CommandSequence {
    pub const NONE: Self = Self(0);

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

struct SequencedCommand {
    sequence: CommandSequence,
    command: Command,
}

pub struct CommandReservation {
    sequence: CommandSequence,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SendError {
    #[error("the command queue is full")]
    Full,
    #[error("the engine is gone")]
    Disconnected,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WaitError {
    #[error("could not queue the command: {0}")]
    Send(#[from] SendError),
    #[error("the engine did not run the command within {0:?}; is it being driven?")]
    Timeout(std::time::Duration),
}

/// How long [`EngineHandle::send_and_wait`] waits, and how often it looks.
///
/// condition variable so that the audio thread only ever has to store a result, never
/// signal anything.
pub const DEFAULT_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1000);

/// How long to spin, yielding, before falling back to sleeping.
///
/// The answer normally arrives within one cycle -- tens of microseconds to a few
/// milliseconds -- and a waiter that sleeps immediately pays a full sleep quantum for it. That
/// did not matter while nothing outside the tests used this, and mattered a great deal as soon
/// as every control read became a round trip: a 1 ms floor per read turns a few thousand reads
/// into seconds, which presents as an application that has stopped rather than one that is
/// slow.
///
/// Yielding rather than busy-spinning, so a waiter never keeps the audio thread off a core.
const SPIN_BUDGET: std::time::Duration = std::time::Duration::from_micros(2000);

/// Sleep between checks once the spin budget is spent, for a waiter that is going to be a
/// while -- an engine that is not being driven at all, most likely.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_micros(500);

/// Counters the audio thread publishes for the control thread to read.
///
/// Atomics rather than a queued snapshot: these are polled, so the reader wants the
/// latest value and does not care about missing intermediate ones.
#[derive(Debug, Default)]
pub struct Stats {
    pub cycles: AtomicU32,
    pub frames: AtomicU32,
    /// Cycles that ran out of sub-blocks before finishing.
    pub stuck_cycles: AtomicU32,
    /// Commands that arrived and were applied.
    pub commands_applied: AtomicU32,
    /// Diagnostic trace publications skipped because every preallocated box was in use.
    pub trace_snapshots_dropped: AtomicU32,
    /// The newest command sequence that finished executing.
    pub last_applied_command: AtomicU64,
    /// Cycles run against a schedule older than the session's topology.
    ///
    /// These are processed, not refused -- see [`Session::process`]. Mirrored from the
    /// session, as `stuck_cycles` is.
    pub stale_cycles: AtomicU32,
    /// Xruns the backend reported. Set by the driver, not by the engine.
    pub xruns: AtomicU32,
    /// Cycles where a duplex driver's capture ring had less than a full cycle in it.
    ///
    /// Nonzero means the input and output streams have drifted apart, or the input
    /// stream has not started yet. The affected frames are silence.
    pub capture_underruns: AtomicU32,
    /// Samples a duplex driver's capture ring had to drop because it was full.
    pub capture_overruns: AtomicU32,
    /// Whether the session's topology has outrun its schedule, as of the last cycle.
    ///
    /// Published so the control side can tell whether a rebuild is needed without asking --
    /// asking costs a full round trip to the audio thread, and the scheduler would otherwise
    /// pay for one on every window whether or not anything had changed.
    ///
    /// A `true` reading can be trusted at once. A `false` reading only means the graph was
    /// current when this was last written, so a caller must also satisfy itself that nothing
    /// is still queued that could dirty it -- see [`EngineHandle::n_pending`].
    pub graph_stale: AtomicBool,
    /// Backend DSP load, as a percentage scaled by 100 so it fits an integer.
    ///
    /// Scaled rather than a float because there is no portable atomic `f32`, and a
    /// polled load figure does not need more resolution than a hundredth of a percent.
    pub dsp_load_centi_percent: AtomicU32,
}

impl Stats {
    pub fn dsp_load_percent(&self) -> f32 {
        self.dsp_load_centi_percent.load(Ordering::Relaxed) as f32 / 100.0
    }
    pub fn set_dsp_load_percent(&self, percent: f32) {
        let scaled = (percent.max(0.0) * 100.0).round();
        self.dsp_load_centi_percent
            .store(scaled as u32, Ordering::Relaxed);
    }
}

/// Bounded session-level composite diagnostics.
///
/// Ordinary composite state uses `CompositeStateMirror`; this queue exists only for trace and
/// timeline diagnostics whose history does not belong to any one object.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CompositeTraceSnapshot {
    pub composite_timeline_counters: CompositeTimelineCounters,
    pub composite_timeline_fault: CompositeTimelineFaultRecord,
    pub composite_timeline_version: u64,
    pub n_retired_composite_plans: usize,
    pub composite_trace: Vec<BoundaryTraceEntry>,
    pub cycle: u32,
    pub n_composite_trace_entries: usize,
}

impl CompositeTraceSnapshot {
    pub fn truncated(&self) -> bool {
        self.n_composite_trace_entries > self.composite_trace.len()
    }
}
/// Owns the session on the audio thread.
pub struct Engine {
    session: Session,
    commands: Consumer<SequencedCommand>,
    returns: Producer<SequencedCommand>,
    filled: Producer<Box<CompositeTraceSnapshot>>,
    empties: Consumer<Box<CompositeTraceSnapshot>>,
    stats: Arc<Stats>,
    alive: Arc<AtomicBool>,
}

/// The control-thread side. Queues commands and reclaims them once run.
pub struct EngineHandle {
    commands: Producer<SequencedCommand>,
    returns: Consumer<SequencedCommand>,
    filled: Consumer<Box<CompositeTraceSnapshot>>,
    empties: Producer<Box<CompositeTraceSnapshot>>,
    current: Option<Box<CompositeTraceSnapshot>>,
    stats: Arc<Stats>,
    alive: Arc<AtomicBool>,
    next_sequence: u64,
}

/// Builds a paired engine and handle around `session`.
///
/// `capacity` bounds how many commands can be outstanding; beyond that
/// [`EngineHandle::send`] refuses rather than blocking or growing.
pub fn split(session: Session, capacity: usize) -> (Engine, EngineHandle) {
    let (cmd_tx, cmd_rx) = RingBuffer::new(capacity);
    let (ret_tx, ret_rx) = RingBuffer::new(capacity);

    // Three snapshots in circulation: one being filled, one in flight, one being read.
    const N_SNAPSHOTS: usize = 3;
    let (filled_tx, filled_rx) = RingBuffer::new(N_SNAPSHOTS);
    let (mut empties_tx, empties_rx) = RingBuffer::new(N_SNAPSHOTS);
    let composite_trace_room = session
        .composite_timeline()
        .n_history_trace_entries()
        .max(64);
    for _ in 0..N_SNAPSHOTS {
        let _ = empties_tx.push(Box::new(CompositeTraceSnapshot {
            composite_trace: Vec::with_capacity(composite_trace_room),
            ..Default::default()
        }));
    }

    let stats = Arc::new(Stats::default());
    let alive = Arc::new(AtomicBool::new(true));
    (
        Engine {
            session,
            commands: cmd_rx,
            returns: ret_tx,
            filled: filled_tx,
            empties: empties_rx,
            stats: Arc::clone(&stats),
            alive: Arc::clone(&alive),
        },
        EngineHandle {
            commands: cmd_tx,
            returns: ret_rx,
            filled: filled_rx,
            empties: empties_tx,
            current: None,
            stats,
            alive,
            next_sequence: 1,
        },
    )
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}

impl Engine {
    pub fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }

    /// Whether any control work is waiting, without applying it.
    ///
    /// For a driver deciding whether it is worth pumping between cycles. Cheap enough to check
    /// in a wait loop, which is the point: the alternative is pumping unconditionally at some
    /// fixed rate and burning a core to do nothing.
    pub fn has_pending_commands(&self) -> bool {
        !self.commands.is_empty()
    }

    /// Escape hatch for a session not yet being driven by a callback.
    ///
    /// Tests and the dummy driver own the engine outright, so there is no other
    /// thread to race with. A real driver must not expose this.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Runs one cycle: applies whatever control work is waiting, then processes.
    ///
    /// Commands run before processing so a mode change lands on the cycle boundary
    /// rather than part-way through a buffer.
    pub fn process(&mut self, n_frames: usize) {
        crate::realtime_alloc_guard::forbid_alloc_if_enabled(|| self.process_inner(n_frames));
    }

    /// Runs one cycle without applying control work first.
    ///
    /// For a driver that has to stage its input buffers between the two: control work has to
    /// land before the cycle runs, but the buffers can only be staged once it has, so a driver
    /// pumps, stages through [`Self::session_mut`], then calls this.
    ///
    /// Reaching for `session_mut().process(..)` instead skips engine counters and should only
    /// be used by code that intentionally bypasses the driver boundary.
    pub fn run_cycle(&mut self, n_frames: usize) {
        crate::realtime_alloc_guard::forbid_alloc_if_enabled(|| self.cycle_inner(n_frames));
    }

    fn process_inner(&mut self, n_frames: usize) {
        self.apply_commands();
        self.publish_graph_staleness();
        self.cycle_inner(n_frames);
    }

    fn cycle_inner(&mut self, n_frames: usize) {
        self.session.process(n_frames);
        self.stats.cycles.fetch_add(1, Ordering::Relaxed);
        self.stats
            .frames
            .fetch_add(n_frames as u32, Ordering::Relaxed);
        self.stats
            .stuck_cycles
            .store(self.session.n_stuck_cycles(), Ordering::Relaxed);
        // A stale graph no longer costs the cycle, but is still published so it is
        // visible instead of looking like ordinary silence.
        self.stats
            .stale_cycles
            .store(self.session.n_stale_cycles(), Ordering::Relaxed);
        self.publish_trace();
    }

    /// Fills and publishes a diagnostic trace snapshot, if a box is free.
    fn publish_trace(&mut self) {
        let Ok(mut snap) = self.empties.pop() else {
            self.stats
                .trace_snapshots_dropped
                .fetch_add(1, Ordering::Relaxed);
            return;
        };

        snap.cycle = self.stats.cycles.load(Ordering::Relaxed);
        snap.composite_timeline_version = self.session.composite_timeline_version();
        snap.n_retired_composite_plans = self.session.composite_timeline().n_retired_plans();
        snap.n_composite_trace_entries =
            self.session.composite_timeline().n_history_trace_entries();

        let timeline = self.session.composite_timeline();
        snap.composite_timeline_counters = timeline.counters();
        snap.composite_timeline_fault = timeline.fault();
        snap.composite_trace.clear();
        let trace_room = snap.composite_trace.capacity();
        snap.composite_trace
            .extend(timeline.history_trace().take(trace_room));

        let _ = self.filled.push(snap);
    }

    /// Applies queued control work without running a cycle.
    ///
    /// Two callers need this, and both are cases where cycles are not arriving:
    ///
    /// - A driver that is spinning but not processing. The dummy driver in controlled mode
    ///   hands out no frames until a test asks for them, so a blocking control call made in
    ///   the meantime would sit until it timed out rather than being answered.
    /// - An engine no driver has taken yet. Between [`split`] and a driver activating there
    ///   is no audio thread at all, and session construction still has to work.
    ///
    /// Commands still land at a cycle boundary: between cycles is exactly where this runs.
    /// The allocation guard applies as it does to [`Self::process`], because in the first
    /// case this *is* the audio thread.
    pub fn pump(&mut self) {
        crate::realtime_alloc_guard::forbid_alloc_if_enabled(|| {
            self.apply_commands();
            self.publish_graph_staleness();
        });
    }

    /// Publishes whether the schedule has fallen behind the topology.
    ///
    /// Written after commands are applied, because applying them is what makes it stale, and
    /// the control side's decision to rebuild is only as good as the moment this reflects.
    pub(crate) fn publish_graph_staleness(&self) {
        self.stats
            .graph_stale
            .store(!self.session.graph_up_to_date(), Ordering::Relaxed);
    }

    fn apply_commands(&mut self) {
        let accepted = self.commands.slots();
        let mut applied = 0u32;
        for _ in 0..accepted {
            let Ok(mut queued) = self.commands.pop() else {
                break;
            };
            crate::realtime_allow_alloc_once!("Engine::apply_commands command execution", || {
                (queued.command)(&mut self.session)
            });
            self.stats
                .last_applied_command
                .store(queued.sequence.get(), Ordering::Release);
            applied += 1;
            // Hand it back to be freed off this thread. Cannot fail: the return
            // queue is as large as the command queue.
            let _ = self.returns.push(queued);
        }
        if applied > 0 {
            self.stats
                .commands_applied
                .fetch_add(applied, Ordering::Relaxed);
        }
    }
}

/// Waits for a result queued by [`EngineHandle::send_for_result`].
///
/// A free function rather than a method, so it is impossible to call while holding the
/// handle: it does not have one. Polls rather than blocking on a condition variable, so the
/// audio thread only ever has to store a result and never has to signal anything.
pub fn wait_for_result<T>(
    mut rx: Consumer<T>,
    timeout: std::time::Duration,
) -> Result<T, WaitError> {
    wait_until(timeout, || rx.pop().ok())
}

pub fn wait_for_command(
    stats: &Stats,
    sequence: CommandSequence,
    timeout: std::time::Duration,
) -> Result<(), WaitError> {
    wait_until(timeout, || {
        (stats.last_applied_command.load(Ordering::Acquire) >= sequence.get()).then_some(())
    })
}

fn wait_until<T>(
    timeout: std::time::Duration,
    mut poll: impl FnMut() -> Option<T>,
) -> Result<T, WaitError> {
    let started = std::time::Instant::now();
    let deadline = started + timeout;
    let spin_until = started + SPIN_BUDGET;
    loop {
        if let Some(value) = poll() {
            return Ok(value);
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return Err(WaitError::Timeout(timeout));
        }
        // Yield while the answer is plausibly imminent, sleep once it clearly is not. The
        // audio thread is never signalled either way: it stores the result and moves on.
        if now < spin_until {
            std::thread::yield_now();
        } else {
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

fn grow_to<T>(values: &mut Vec<T>, wanted: usize) {
    if wanted > values.capacity() {
        values.reserve(wanted - values.capacity());
    }
}

impl EngineHandle {
    pub fn is_connected(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    #[cfg(feature = "app_backend")]
    pub(crate) fn connected_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.alive)
    }

    pub fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }

    /// Queues control work, reclaiming anything already run.
    ///
    /// Reclaiming here rather than in a separate step keeps the queue from silently
    /// filling up in a caller that only ever sends.
    pub fn send(&mut self, command: Command) -> Result<CommandSequence, SendError> {
        let reservation = self.try_reserve()?;
        Ok(self.send_reserved(reservation, command))
    }

    pub fn try_reserve(&mut self) -> Result<CommandReservation, SendError> {
        self.reclaim();
        if !self.alive.load(Ordering::Acquire) {
            return Err(SendError::Disconnected);
        }
        if self.commands.slots() == 0 {
            return Err(SendError::Full);
        }
        Ok(CommandReservation {
            sequence: CommandSequence(self.next_sequence),
        })
    }

    pub fn send_reserved(
        &mut self,
        reservation: CommandReservation,
        command: Command,
    ) -> CommandSequence {
        let sequence = reservation.sequence;
        self.commands
            .push(SequencedCommand { sequence, command })
            .unwrap_or_else(|_| unreachable!("a reserved command slot must remain available"));
        self.next_sequence = self.next_sequence.wrapping_add(1).max(1);
        sequence
    }

    /// Queues control work and waits for its result.
    ///
    /// For the reads a caller cannot answer from a snapshot -- a channel's audio data,
    /// say. The result comes back through a single-slot queue, so the audio thread
    /// stores it and moves on; this side polls.
    ///
    /// which runs the command on the calling thread when it decides the process thread
    /// is idle, this refuses: the handle has no session to run it against, and reaching
    /// around the engine to find one is how two threads end up in it at once.
    pub fn send_and_wait<T, F>(
        &mut self,
        f: F,
        timeout: std::time::Duration,
    ) -> Result<T, WaitError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> T + Send + 'static,
    {
        let (_, rx) = self.send_for_result(f)?;
        wait_for_result(rx, timeout)
    }

    /// Queues work and hands back the slot its result will arrive in.
    ///
    /// The half of [`Self::send_and_wait`] that needs this handle, split out from the half
    /// that waits. A caller reaching this handle through a mutex -- which anything shared
    /// between GUI threads must -- has to be able to release that mutex *before* waiting.
    ///
    /// Holding it across the wait is a mistake worth spelling out, because it does not look
    /// like one and it does not fail: every control operation then queues behind a full
    /// round trip to the audio thread, and a caller doing this in a loop starves every other
    /// thread. What that looks like from outside is not a deadlock but a GUI that has stopped
    /// responding, which is a good deal harder to diagnose.
    pub fn send_for_result<T, F>(
        &mut self,
        f: F,
    ) -> Result<(CommandSequence, Consumer<T>), SendError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> T + Send + 'static,
    {
        let reservation = self.try_reserve()?;
        Ok(self.send_for_result_reserved(reservation, f))
    }

    pub fn send_for_result_reserved<T, F>(
        &mut self,
        reservation: CommandReservation,
        f: F,
    ) -> (CommandSequence, Consumer<T>)
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> T + Send + 'static,
    {
        let (mut tx, rx) = RingBuffer::<T>::new(1);
        let mut f = Some(f);
        let sequence = self.send_reserved(
            reservation,
            Box::new(move |s: &mut Session| {
                if let Some(f) = f.take() {
                    let _ = tx.push(f(s));
                }
            }),
        );
        (sequence, rx)
    }

    /// Queues a validated timeline for callback-boundary installation.
    ///
    /// The displaced timeline is returned through the result queue so its plans are
    /// destroyed by the control thread. The returned receiver also carries the
    /// activation-time generation/topology recheck result.
    pub fn send_composite_timeline(
        &mut self,
        timeline: CompositeBoundaryTimeline,
    ) -> Result<Consumer<Result<ReclaimedCompositeTimeline, RejectedCompositeTimeline>>, SendError>
    {
        self.send_for_result(move |session| session.install_prepared_composite_timeline(timeline))
            .map(|(_, receiver)| receiver)
    }

    /// Arms a synchronized composite transition at callback-start acceptance.
    pub fn send_composite_transition(
        &mut self,
        source: LoopIdentity,
        mode: LoopMode,
        delay: u32,
    ) -> Result<Consumer<Result<u64, CompositeTimelineControlError>>, SendError> {
        self.send_for_result(move |session| {
            session.accept_composite_transition(source, mode, delay)
        })
        .map(|(_, receiver)| receiver)
    }

    /// Queues an unsynchronized mode change and exact prevalidated iteration seek.
    pub fn send_composite_immediate_transition(
        &mut self,
        source: LoopIdentity,
        mode: LoopMode,
        iteration: i64,
    ) -> Result<Consumer<Result<u64, CompositeTimelineControlError>>, SendError> {
        self.send_for_result(move |session| {
            session.accept_composite_immediate_transition(source, mode, iteration)
        })
        .map(|(_, receiver)| receiver)
    }

    /// Changes record-pass completion behavior at callback-start acceptance.
    pub fn send_composite_play_after_record(
        &mut self,
        source: LoopIdentity,
        enabled: bool,
    ) -> Result<Consumer<Result<u64, CompositeTimelineControlError>>, SendError> {
        self.send_for_result(move |session| {
            session.accept_composite_play_after_record(source, enabled)
        })
        .map(|(_, receiver)| receiver)
    }

    /// Returns plans displaced by deferred activation for control-thread destruction.
    pub fn send_composite_plan_reclamation(
        &mut self,
        capacity: usize,
    ) -> Result<Consumer<Vec<CompiledCompositePlan>>, SendError> {
        let storage = Vec::with_capacity(capacity);
        self.send_for_result(move |session| session.reclaim_composite_plans(storage))
            .map(|(_, receiver)| receiver)
    }

    /// Queues recovery from a latched composite timeline fault.
    pub fn send_composite_fault_reset(&mut self) -> Result<Consumer<u64>, SendError> {
        self.send_for_result(Session::accept_composite_fault_reset)
            .map(|(_, receiver)| receiver)
    }

    /// Queues a composite/basic target action for callback-start acceptance.
    ///
    /// `None` uses the callback-start sample. A timestamp retains its exact future
    /// sample boundary; a past timestamp is reported by the result receiver.
    pub fn send_composite_control(
        &mut self,
        target: LoopIdentity,
        action: BoundaryTargetAction,
        at_sample: Option<u64>,
    ) -> Result<Consumer<Result<u64, CompositeTimelineControlError>>, SendError> {
        self.send_for_result(move |session| {
            session.accept_composite_control(target, action, at_sample)
        })
        .map(|(_, receiver)| receiver)
    }

    /// Frees commands the engine has finished with. Safe to call at any time.
    pub fn reclaim(&mut self) -> usize {
        let mut n = 0;
        while self.returns.pop().is_ok() {
            n += 1;
        }
        n
    }

    /// Takes the newest session-level composite diagnostics.
    pub fn poll_trace(&mut self) -> Option<&CompositeTraceSnapshot> {
        while let Ok(snap) = self.filled.pop() {
            if let Some(old) = self.current.replace(snap) {
                self.recycle(old);
            }
        }
        if let Some(current) = self.current.as_mut() {
            grow_to(
                &mut current.composite_trace,
                current.n_composite_trace_entries,
            );
        }
        self.current.as_deref()
    }

    fn recycle(&mut self, mut snap: Box<CompositeTraceSnapshot>) {
        snap.composite_trace.clear();
        let _ = self.empties.push(snap);
    }

    /// Commands queued but not yet applied.
    ///
    /// A lower bound: `slots` reports free space conservatively, so this may read
    /// high if the engine has just consumed something. Good enough for reporting,
    /// not for control flow.
    pub fn n_pending(&self) -> usize {
        self.commands
            .buffer()
            .capacity()
            .saturating_sub(self.commands.slots())
    }
}

/// A driver moves the engine onto its own thread, so this has to hold. Checked here
/// rather than discovered when a driver is written.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Engine>();
    assert_send::<Session>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_mode::ChannelMode;
    use crate::dummy_port::{DummyAudioPort, PortId};
    use crate::loop_mode::LoopMode;
    use crate::port::PortDirection;
    use crate::session::Port;
    use assert2::{check, let_assert};

    fn engine() -> (Engine, EngineHandle) {
        split(Session::default(), 16)
    }

    /// A blocking read, which is how the control side gets at anything a snapshot does
    /// not carry. Driven from this thread here; a real driver's callback does it.
    #[test]
    fn send_and_wait_returns_a_result_from_the_engine() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;

        let (mut e, mut h) = engine();
        let l = e.session_mut().create_loop();
        e.session_mut().loop_mut(l).expect("loop").set_length(42);
        e.session_mut().apply_graph_changes().expect("schedule");

        // The engine has to be running for the wait to be satisfied, so drive it from
        // another thread while this one blocks.
        let stop = Arc::new(AtomicBool::new(false));
        let stop_writer = Arc::clone(&stop);
        let driver = std::thread::spawn(move || {
            while !stop_writer.load(Ordering::Relaxed) {
                e.process(4);
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
            e
        });

        let got = h.send_and_wait(
            move |s: &mut Session| s.loop_(0).map(|l| l.length()),
            DEFAULT_WAIT_TIMEOUT,
        );

        stop.store(true, Ordering::Relaxed);
        let _engine = driver.join().expect("driver thread");

        let_assert!(Ok(Some(42)) = got);
    }

    #[test]
    fn send_and_wait_times_out_when_nothing_is_driving_the_engine() {
        let (_e, mut h) = engine();

        // No cycles will run, so the command is never applied. Better to say so than
        // to hang.
        let short = std::time::Duration::from_millis(20);
        let got: Result<(), _> = h.send_and_wait(|_: &mut Session| (), short);
        check!(got == Err(WaitError::Timeout(short)));
    }

    #[test]
    /// DSP load is stored scaled, so check it survives the round trip and that a
    /// nonsense reading is clamped rather than wrapping.
    fn dsp_load_round_trips() {
        let s = Stats::default();
        check!(s.dsp_load_percent() == 0.0);

        s.set_dsp_load_percent(12.34);
        check!(s.dsp_load_percent() == 12.34);

        s.set_dsp_load_percent(-5.0);
        check!(s.dsp_load_percent() == 0.0);
    }

    /// What `pump` is for: control work answered while no cycles are being run.
    ///
    /// A driver spinning in controlled mode processes nothing until frames are requested, and
    /// an engine no driver has taken yet has no thread at all. Without this, a blocking call
    /// in either state waits out its whole timeout.
    #[test]
    fn pump_applies_commands_without_advancing_anything() {
        let (mut e, mut h) = engine();
        let l = e.session_mut().create_loop();
        e.session_mut().loop_mut(l).expect("loop").set_length(64);
        e.session_mut()
            .set_loop_mode(l, LoopMode::Playing)
            .expect("mode");
        e.session_mut().apply_graph_changes().expect("schedule");

        let_assert!(
            Ok(_) = h.send(Box::new(|s: &mut Session| {
                let _ = s.set_loop_mode(0, LoopMode::Stopped);
            }))
        );

        e.pump();

        // The command landed...
        check!(e.session().loop_(0).expect("loop").mode() == LoopMode::Stopped);
        check!(e.stats().commands_applied.load(Ordering::Relaxed) == 1);
        // ...without a cycle running, so nothing advanced.
        check!(e.stats().cycles.load(Ordering::Relaxed) == 0);
        check!(e.session().loop_(0).expect("loop").position() == 0);
        // And the box came back to be freed on this side, as after a cycle.
        check!(h.reclaim() == 1);
    }

    #[test]
    fn a_command_is_applied_on_the_next_cycle() {
        let (mut e, mut h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        let_assert!(
            Ok(_) = h.send(Box::new(|s: &mut Session| {
                s.create_loop();
            }))
        );
        // Not yet: it lands when the cycle runs.
        check!(e.session().n_loops() == 0);
        check!(h.n_pending() == 1);

        e.process(4);
        check!(e.session().n_loops() == 1);
        check!(h.n_pending() == 0);
        check!(e.stats().commands_applied.load(Ordering::Relaxed) == 1);
    }

    #[test]
    fn commands_are_applied_in_order() {
        let (mut e, mut h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        for _ in 0..3 {
            let_assert!(
                Ok(_) = h.send(Box::new(|s: &mut Session| {
                    s.create_loop();
                }))
            );
        }
        e.process(4);
        check!(e.session().n_loops() == 3);
    }

    #[test]
    fn a_full_queue_refuses_rather_than_growing() {
        let (mut e, mut h) = split(Session::default(), 2);
        e.session_mut().apply_graph_changes().expect("schedule");

        let first = h.send(Box::new(|_: &mut Session| {})).expect("first");
        let second = h.send(Box::new(|_: &mut Session| {})).expect("second");
        check!(first.get() == 1);
        check!(second.get() == 2);
        check!(h.send(Box::new(|_: &mut Session| {})) == Err(SendError::Full));

        // Draining makes room again without consuming a sequence for the refusal.
        e.process(4);
        let third = h.send(Box::new(|_: &mut Session| {})).expect("third");
        check!(third.get() == 3);
        check!(e.stats().last_applied_command.load(Ordering::Acquire) == second.get());
    }

    #[test]
    fn a_payload_can_be_retained_until_queue_capacity_is_reserved() {
        let (mut e, mut h) = split(Session::default(), 1);
        h.send(Box::new(|_: &mut Session| {})).expect("fill queue");

        let payload = vec![1u8, 2, 3, 4];
        check!(matches!(h.try_reserve(), Err(SendError::Full)));
        check!(payload.len() == 4);

        e.pump();
        let reservation = h.try_reserve().expect("room after pump");
        let sequence = h.send_reserved(
            reservation,
            Box::new(move |s: &mut Session| {
                let loop_idx = s.create_loop();
                s.loop_mut(loop_idx)
                    .expect("created loop")
                    .set_length(payload.len() as u32);
            }),
        );
        e.pump();

        check!(sequence.get() == 2);
        check!(e.session().loop_(0).expect("loop").length() == 4);
    }

    #[test]
    fn command_fences_observe_applied_sequence() {
        let (mut e, mut h) = engine();
        let sequence = h.send(Box::new(|_: &mut Session| {})).expect("queue");
        let stats = Arc::clone(h.stats());
        let driver = std::thread::spawn(move || {
            e.pump();
            e
        });

        wait_for_command(&stats, sequence, DEFAULT_WAIT_TIMEOUT).expect("fence");
        let _ = driver.join().expect("engine");
    }

    #[test]
    fn sending_after_engine_drop_reports_disconnected() {
        let (e, mut h) = engine();
        drop(e);
        check!(h.send(Box::new(|_: &mut Session| {})) == Err(SendError::Disconnected));
    }

    #[test]
    fn executed_commands_come_back_to_be_freed() {
        let (mut e, mut h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        for _ in 0..3 {
            let_assert!(Ok(_) = h.send(Box::new(|_: &mut Session| {})));
        }
        e.process(4);

        // Three ran, so three boxes are waiting to be dropped on this side.
        check!(h.reclaim() == 3);
        check!(h.reclaim() == 0);
    }

    #[test]
    fn cycles_and_frames_are_counted() {
        let (mut e, _h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        e.process(4);
        e.process(8);

        check!(e.stats().cycles.load(Ordering::Relaxed) == 2);
        check!(e.stats().frames.load(Ordering::Relaxed) == 12);
        check!(e.stats().stale_cycles.load(Ordering::Relaxed) == 0);
    }

    #[test]
    fn a_stale_graph_still_runs_and_is_counted() {
        let (mut e, mut h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        // Adding a port leaves the schedule out of date. The cycle runs anyway, against
        // the last-applied schedule, so existing audio keeps flowing while the next
        // schedule is built; the staleness is counted rather than costing the cycle.
        let_assert!(
            Ok(_) = h.send(Box::new(|s: &mut Session| {
                s.add_port(Port::Dummy(DummyAudioPort::new(
                    PortId(1),
                    "in",
                    PortDirection::Input,
                    4,
                )));
            }))
        );
        e.process(4);

        check!(!e.session().graph_up_to_date());
        check!(e.stats().stale_cycles.load(Ordering::Relaxed) == 1);
        check!(e.stats().cycles.load(Ordering::Relaxed) == 1);
    }

    #[test]
    fn a_command_can_reconfigure_and_reschedule() {
        let (mut e, mut h) = engine();

        // Structural work and the reschedule it needs go in one command, so the
        // graph is never left stale at a cycle boundary.
        let_assert!(
            Ok(_) = h.send(Box::new(|s: &mut Session| {
                let p = s.add_port(Port::Dummy(DummyAudioPort::new(
                    PortId(1),
                    "in",
                    PortDirection::Input,
                    4,
                )));
                let l = s.create_loop();
                if let Ok(c) = s.add_audio_channel(l, 64, ChannelMode::Direct) {
                    let _ = s.connect_channel_input(c, p);
                }
                let _ = s.set_loop_mode(l, LoopMode::Recording);
                let _ = s.apply_graph_changes();
            }))
        );

        e.process(4);

        check!(e.session().graph_up_to_date());
        check!(e.session().n_loops() == 1);
        check!(e.stats().cycles.load(Ordering::Relaxed) == 1);
        check!(e.session().loop_(0).expect("loop").length() == 4);
    }
}
