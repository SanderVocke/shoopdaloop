//! The boundary between the control thread and the audio thread.
//!
//! A real driver calls its process callback on a thread it owns, so the session has
//! to live there and cannot be touched from outside. Control operations are queued
//! as closures and run at a cycle boundary, mirroring the C++ `WithCommandQueue`,
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

use crate::loop_mode::LoopMode;
use crate::session::Session;

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use thiserror::Error;

/// A unit of control work, run on the audio thread between cycles.
pub type Command = Box<dyn FnMut(&mut Session) + Send>;

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
/// The same figures the C++ `CommandQueue` uses. It polls rather than waiting on a
/// condition variable so that the audio thread only ever has to store a result, never
/// signal anything.
pub const DEFAULT_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1000);
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_micros(1000);

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
    /// Cycles the engine refused because its graph needed rebuilding.
    pub refused_cycles: AtomicU32,
    /// Xruns the backend reported. Set by the driver, not by the engine.
    pub xruns: AtomicU32,
    /// Cycles where a duplex driver's capture ring had less than a full cycle in it.
    ///
    /// Nonzero means the input and output streams have drifted apart, or the input
    /// stream has not started yet. The affected frames are silence.
    pub capture_underruns: AtomicU32,
    /// Samples a duplex driver's capture ring had to drop because it was full.
    pub capture_overruns: AtomicU32,
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

#[derive(Clone, Debug, PartialEq)]
pub struct LoopState {
    pub mode: LoopMode,
    pub length: u32,
    pub position: u32,
    pub maybe_next_mode: Option<LoopMode>,
    pub maybe_next_mode_delay: Option<u32>,
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            mode: LoopMode::Unknown,
            length: 0,
            position: 0,
            maybe_next_mode: None,
            maybe_next_mode_delay: None,
        }
    }
}

/// One loop's state, as the control side polls it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopSnapshot {
    pub mode: LoopMode,
    pub length: u32,
    pub position: u32,
    pub next_mode: Option<LoopMode>,
    pub next_mode_delay: Option<u32>,
}

/// A cycle's published state.
///
/// Shipped between the threads as a box that is refilled and reused, so publishing
/// never allocates. `loops` is only grown by the control thread: see `truncated`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StateSnapshot {
    pub loops: Vec<LoopSnapshot>,
    /// Cycle this was taken at, so a reader can tell fresh from stale.
    pub cycle: u32,
    /// Loops the session had, which may exceed `loops.len()`.
    ///
    /// The audio thread fills only as far as the box already has room for, because
    /// growing the vector there would allocate. A reader seeing this exceed
    /// `loops.len()` should hand back bigger boxes; [`EngineHandle::poll`] does.
    pub n_loops: usize,
}

impl StateSnapshot {
    pub fn truncated(&self) -> bool {
        self.n_loops > self.loops.len()
    }
}

/// Owns the session on the audio thread.
pub struct Engine {
    session: Session,
    commands: Consumer<Command>,
    returns: Producer<Command>,
    /// Snapshots filled and published for the control side.
    filled: Producer<Box<StateSnapshot>>,
    /// Boxes to refill, handed back by the control side.
    empties: Consumer<Box<StateSnapshot>>,
    stats: Arc<Stats>,
}

/// The control-thread side. Queues commands and reclaims them once run.
pub struct EngineHandle {
    commands: Producer<Command>,
    returns: Consumer<Command>,
    filled: Consumer<Box<StateSnapshot>>,
    empties: Producer<Box<StateSnapshot>>,
    /// Most recent snapshot taken, held so callers can borrow it.
    current: Option<Box<StateSnapshot>>,
    stats: Arc<Stats>,
}

/// Builds a paired engine and handle around `session`.
///
/// `capacity` bounds how many commands can be outstanding; beyond that
/// [`EngineHandle::send`] refuses rather than blocking or growing.
pub fn split(session: Session, capacity: usize) -> (Engine, EngineHandle) {
    let (cmd_tx, cmd_rx) = RingBuffer::new(capacity);
    let (ret_tx, ret_rx) = RingBuffer::new(capacity);

    // Three snapshots in circulation: one being filled, one in flight, one being read.
    // Fewer would make the audio thread skip publishing whenever the reader is mid-poll.
    const N_SNAPSHOTS: usize = 3;
    let (filled_tx, filled_rx) = RingBuffer::new(N_SNAPSHOTS);
    let (mut empties_tx, empties_rx) = RingBuffer::new(N_SNAPSHOTS);
    let room = session.n_loops().max(8);
    for _ in 0..N_SNAPSHOTS {
        let _ = empties_tx.push(Box::new(StateSnapshot {
            loops: Vec::with_capacity(room),
            ..Default::default()
        }));
    }

    let stats = Arc::new(Stats::default());
    (
        Engine {
            session,
            commands: cmd_rx,
            returns: ret_tx,
            filled: filled_tx,
            empties: empties_rx,
            stats: Arc::clone(&stats),
        },
        EngineHandle {
            commands: cmd_tx,
            returns: ret_rx,
            filled: filled_rx,
            empties: empties_tx,
            current: None,
            stats,
        },
    )
}

impl Engine {
    pub fn stats(&self) -> &Arc<Stats> {
        &self.stats
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

    /// Gives the session back when the engine is torn down.
    pub fn into_session(self) -> Session {
        self.session
    }

    /// Runs one cycle: applies whatever control work is waiting, then processes.
    ///
    /// Commands run before processing so a mode change lands on the cycle boundary
    /// rather than part-way through a buffer.
    pub fn process(&mut self, n_frames: usize) {
        crate::realtime_alloc_guard::forbid_alloc_if_enabled(|| self.process_inner(n_frames));
    }

    fn process_inner(&mut self, n_frames: usize) {
        self.apply_commands();

        match self.session.process(n_frames) {
            Ok(()) => {
                self.stats.cycles.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .frames
                    .fetch_add(n_frames as u32, Ordering::Relaxed);
            }
            Err(_) => {
                // A stale graph is refused rather than run, and counted so it is
                // visible instead of looking like silence.
                self.stats.refused_cycles.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.stats
            .stuck_cycles
            .store(self.session.n_stuck_cycles(), Ordering::Relaxed);

        self.publish_state();
    }

    /// Fills and publishes a snapshot, if there is a box free to fill.
    ///
    /// Skipped rather than queued when the control side has not returned one: it polls,
    /// so it wants the newest state, and dropping an intermediate costs nothing.
    fn publish_state(&mut self) {
        let Ok(mut snap) = self.empties.pop() else {
            return;
        };

        snap.cycle = self.stats.cycles.load(Ordering::Relaxed);
        snap.n_loops = self.session.n_loops();
        snap.loops.clear();
        // Only as far as the box already has room for: pushing past capacity would
        // allocate. `n_loops` records the shortfall so the reader can fix it.
        let room = snap.loops.capacity();
        for i in 0..self.session.n_loops().min(room) {
            let Some(l) = self.session.loop_(i) else {
                break;
            };
            let next = l.first_planned_transition();
            snap.loops.push(LoopSnapshot {
                mode: l.mode(),
                length: l.length(),
                position: l.position(),
                next_mode: next.map(|(m, _)| m),
                next_mode_delay: next.map(|(_, d)| d),
            });
        }

        let _ = self.filled.push(snap);
    }

    fn apply_commands(&mut self) {
        let mut applied = 0u32;
        while let Ok(mut cmd) = self.commands.pop() {
            crate::realtime_allow_alloc_once!("Engine::apply_commands command execution", || {
                cmd(&mut self.session)
            });
            applied += 1;
            // Hand it back to be freed off this thread. Cannot fail: the return
            // queue is as large as the command queue.
            let _ = self.returns.push(cmd);
        }
        if applied > 0 {
            self.stats
                .commands_applied
                .fetch_add(applied, Ordering::Relaxed);
        }
    }
}

impl EngineHandle {
    pub fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }

    /// Queues control work, reclaiming anything already run.
    ///
    /// Reclaiming here rather than in a separate step keeps the queue from silently
    /// filling up in a caller that only ever sends.
    pub fn send(&mut self, command: Command) -> Result<(), SendError> {
        self.reclaim();
        self.commands.push(command).map_err(|e| match e {
            rtrb::PushError::Full(_) => SendError::Full,
        })
    }

    /// Queues control work and waits for its result.
    ///
    /// For the reads a caller cannot answer from a snapshot -- a channel's audio data,
    /// say. The result comes back through a single-slot queue, so the audio thread
    /// stores it and moves on; this side polls.
    ///
    /// Times out rather than hanging if nothing is driving the engine. Unlike the C++,
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
        let (mut tx, mut rx) = RingBuffer::<T>::new(1);
        let mut f = Some(f);
        self.send(Box::new(move |s: &mut Session| {
            if let Some(f) = f.take() {
                let _ = tx.push(f(s));
            }
        }))?;

        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Ok(v) = rx.pop() {
                return Ok(v);
            }
            if std::time::Instant::now() >= deadline {
                return Err(WaitError::Timeout(timeout));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    /// Frees commands the engine has finished with. Safe to call at any time.
    pub fn reclaim(&mut self) -> usize {
        let mut n = 0;
        while self.returns.pop().is_ok() {
            n += 1;
        }
        n
    }

    /// Takes the newest published state, returning older boxes to be refilled.
    ///
    /// Grows the boxes when the engine reports it had more loops than would fit, so
    /// the shortfall corrects itself without the audio thread ever allocating.
    pub fn poll(&mut self) -> Option<&StateSnapshot> {
        while let Ok(snap) = self.filled.pop() {
            if let Some(old) = self.current.replace(snap) {
                self.recycle(old);
            }
        }
        // Grow the box we are holding. It goes back to the pool on the next poll, so
        // a few polls after loops are added every box in circulation has room. The
        // audio thread never allocates; it just publishes a short snapshot until then.
        if let Some(c) = self.current.as_mut() {
            if c.n_loops > c.loops.capacity() {
                let extra = c.n_loops - c.loops.capacity();
                c.loops.reserve(extra);
            }
        }
        self.current.as_deref()
    }

    fn recycle(&mut self, mut snap: Box<StateSnapshot>) {
        snap.loops.clear();
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
    fn state_is_published_after_a_cycle() {
        let (mut e, mut h) = engine();
        let l = e.session_mut().create_loop();
        e.session_mut().loop_mut(l).expect("loop").set_length(16);
        e.session_mut().apply_graph_changes().expect("schedule");
        e.session_mut()
            .set_loop_mode(l, LoopMode::Playing)
            .expect("mode");

        // Nothing has run, so there is nothing to read yet.
        check!(h.poll().is_none());

        e.process(4);

        let_assert!(Some(snap) = h.poll());
        check!(snap.n_loops == 1);
        check!(snap.loops.len() == 1);
        check!(snap.loops[0].mode == LoopMode::Playing);
        check!(snap.loops[0].length == 16);
        check!(snap.loops[0].position == 4);
        check!(snap.cycle == 1);
    }

    #[test]
    fn polling_keeps_only_the_newest_state() {
        let (mut e, mut h) = engine();
        let l = e.session_mut().create_loop();
        e.session_mut().loop_mut(l).expect("loop").set_length(64);
        e.session_mut().apply_graph_changes().expect("schedule");
        e.session_mut()
            .set_loop_mode(l, LoopMode::Playing)
            .expect("mode");

        e.process(4);
        e.process(4);

        // Two cycles ran; the reader wants where the loop is now, not where it was.
        let_assert!(Some(snap) = h.poll());
        check!(snap.loops[0].position == 8);
        check!(snap.cycle == 2);
    }

    /// The audio thread cannot grow the snapshot, so it publishes a short one and says
    /// so; the handle grows the boxes and later snapshots are complete.
    #[test]
    fn more_loops_than_fit_are_reported_then_accommodated() {
        let (mut e, mut h) = split(Session::default(), 16);
        for _ in 0..20 {
            e.session_mut().create_loop();
        }
        e.session_mut().apply_graph_changes().expect("schedule");

        e.process(4);
        let_assert!(Some(snap) = h.poll());
        check!(snap.n_loops == 20);
        check!(snap.truncated());
        check!(snap.loops.len() < 20);

        // A few cycles later every box in circulation has been refitted.
        for _ in 0..6 {
            e.process(4);
            h.poll();
        }
        let_assert!(Some(snap) = h.poll());
        check!(!snap.truncated());
        check!(snap.loops.len() == 20);
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

    #[test]
    fn a_command_is_applied_on_the_next_cycle() {
        let (mut e, mut h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        let_assert!(
            Ok(()) = h.send(Box::new(|s: &mut Session| {
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
                Ok(()) = h.send(Box::new(|s: &mut Session| {
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

        let_assert!(Ok(()) = h.send(Box::new(|_: &mut Session| {})));
        let_assert!(Ok(()) = h.send(Box::new(|_: &mut Session| {})));
        check!(h.send(Box::new(|_: &mut Session| {})) == Err(SendError::Full));

        // Draining makes room again.
        e.process(4);
        let_assert!(Ok(()) = h.send(Box::new(|_: &mut Session| {})));
    }

    #[test]
    fn executed_commands_come_back_to_be_freed() {
        let (mut e, mut h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        for _ in 0..3 {
            let_assert!(Ok(()) = h.send(Box::new(|_: &mut Session| {})));
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
        check!(e.stats().refused_cycles.load(Ordering::Relaxed) == 0);
    }

    #[test]
    fn a_stale_graph_is_refused_and_counted() {
        let (mut e, mut h) = engine();
        e.session_mut().apply_graph_changes().expect("schedule");

        // Adding a port leaves the schedule out of date, and the session refuses to
        // run rather than processing a graph it no longer matches.
        let_assert!(
            Ok(()) = h.send(Box::new(|s: &mut Session| {
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
        check!(e.stats().refused_cycles.load(Ordering::Relaxed) == 1);
        check!(e.stats().cycles.load(Ordering::Relaxed) == 0);
    }

    #[test]
    fn a_command_can_reconfigure_and_reschedule() {
        let (mut e, mut h) = engine();

        // Structural work and the reschedule it needs go in one command, so the
        // graph is never left stale at a cycle boundary.
        let_assert!(
            Ok(()) = h.send(Box::new(|s: &mut Session| {
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
