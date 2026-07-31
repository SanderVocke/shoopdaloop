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

use crate::channel_mode::ChannelMode;
use crate::loop_mode::LoopMode;
use crate::session::{ChannelKind, Session};
use crate::state::{AudioChannelState, AudioPortSnapshot, MidiChannelState, MidiPortSnapshot};

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
/// never allocates. None of the vectors is grown by the audio thread: see `truncated`.
///
/// Covers everything a UI polls at frame rate -- loops, channels and ports -- because the
/// alternative is a blocking round trip per object per frame, which at one audio cycle each
/// costs more than the frame budget as soon as a session has a handful of tracks. What is
/// *not* here is anything a poll does not need: audio data, MIDI event lists and FX-chain
/// state are asked for individually, and FX-chain state does not live in the session at all.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StateSnapshot {
    pub loops: Vec<LoopSnapshot>,
    pub audio_channels: Vec<AudioChannelState>,
    pub midi_channels: Vec<MidiChannelState>,
    /// Indexed by the session's port arena, so an entry may be either kind. A port of the
    /// other kind leaves `None` here rather than shifting the indices.
    pub audio_ports: Vec<Option<AudioPortSnapshot>>,
    pub midi_ports: Vec<Option<MidiPortSnapshot>>,
    /// Cycle this was taken at, so a reader can tell fresh from stale.
    pub cycle: u32,
    /// Loops the session had, which may exceed `loops.len()`.
    ///
    /// The audio thread fills only as far as the box already has room for, because
    /// growing the vector there would allocate. A reader seeing this exceed
    /// `loops.len()` should hand back bigger boxes; [`EngineHandle::poll`] does.
    pub n_loops: usize,
    /// Channels the session had, which may exceed the channel vectors' lengths.
    ///
    /// One count for both vectors: they are indexed by the session's single channel arena,
    /// so an audio channel and a MIDI channel never share an index and both vectors are
    /// sized to the arena. The slot of the other kind is left at its default.
    pub n_channels: usize,
    /// Ports the session had, which may exceed the port vectors' lengths.
    pub n_ports: usize,
}

impl StateSnapshot {
    /// Whether the audio thread ran out of room in any vector.
    ///
    /// One flag for all of them: a reader's only response is to hand back bigger boxes, and
    /// [`EngineHandle::poll`] grows whichever vectors are actually short.
    pub fn truncated(&self) -> bool {
        self.n_loops > self.loops.len()
            || self.n_channels > self.audio_channels.len()
            || self.n_channels > self.midi_channels.len()
            || self.n_ports > self.audio_ports.len()
            || self.n_ports > self.midi_ports.len()
    }
}

impl Default for AudioChannelState {
    fn default() -> Self {
        Self {
            mode: ChannelMode::Disabled,
            gain: 0.0,
            output_peak: 0.0,
            length: 0,
            start_offset: 0,
            played_back_sample: None,
            n_preplay_samples: 0,
            data_dirty: false,
        }
    }
}

impl Default for MidiChannelState {
    fn default() -> Self {
        Self {
            mode: ChannelMode::Disabled,
            n_events_triggered: 0,
            n_notes_active: 0,
            length: 0,
            start_offset: 0,
            played_back_sample: None,
            n_preplay_samples: 0,
            data_dirty: false,
        }
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
    // Sized for what the session already has, with a floor so a session built up after the
    // split does not publish truncated for its first few cycles. Undersizing is not an error
    // -- `poll` grows the boxes -- so the floor is a convenience, not a correctness matter.
    let loop_room = session.n_loops().max(8);
    let channel_room = session.n_channels().max(16);
    let port_room = session.n_ports().max(16);
    for _ in 0..N_SNAPSHOTS {
        let _ = empties_tx.push(Box::new(StateSnapshot {
            loops: Vec::with_capacity(loop_room),
            audio_channels: Vec::with_capacity(channel_room),
            midi_channels: Vec::with_capacity(channel_room),
            audio_ports: Vec::with_capacity(port_room),
            midi_ports: Vec::with_capacity(port_room),
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

    /// Runs one cycle and publishes, without applying control work first.
    ///
    /// For a driver that has to stage its input buffers between the two: control work has to
    /// land before the cycle runs, but the buffers can only be staged once it has, so a driver
    /// pumps, stages through [`Self::session_mut`], then calls this.
    ///
    /// Reaching for `session_mut().process(..)` instead is the mistake this exists to prevent.
    /// It looks equivalent and silently is not: it skips the counters and the state snapshot,
    /// so every `poll` returns nothing, every reader falls back to a blocking round trip, and
    /// the only symptom is that the application is inexplicably slow.
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
        snap.n_channels = self.session.n_channels();
        snap.n_ports = self.session.n_ports();

        // Each vector is filled only as far as the box already has room for: pushing past
        // capacity would allocate. The `n_*` counts above record any shortfall so the reader
        // can hand back bigger boxes -- see `EngineHandle::poll`.
        snap.loops.clear();
        for i in 0..snap.n_loops.min(snap.loops.capacity()) {
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

        // Both channel vectors are indexed by the session's single channel arena, so each
        // index is filled in exactly one of them and left at its default in the other. That
        // costs a slot per channel and buys an index a handle can use without a second map.
        snap.audio_channels.clear();
        snap.midi_channels.clear();
        let channel_room = snap
            .audio_channels
            .capacity()
            .min(snap.midi_channels.capacity());
        for i in 0..snap.n_channels.min(channel_room) {
            let mut audio = AudioChannelState::default();
            let mut midi = MidiChannelState::default();
            if let Some(m) = self.session.channel_mapping(i) {
                let (loop_idx, kind, channel_idx) = (m.loop_idx, m.kind, m.channel_idx);
                if let Some(l) = self.session.loop_(loop_idx) {
                    match kind {
                        ChannelKind::Audio => {
                            if let Some(c) = l.audio_channel(channel_idx) {
                                audio = AudioChannelState {
                                    mode: c.mode(),
                                    gain: c.gain(),
                                    output_peak: c.output_peak(),
                                    length: c.length() as u32,
                                    start_offset: c.start_offset(),
                                    played_back_sample: c.played_back_sample(),
                                    n_preplay_samples: c.pre_play_samples(),
                                    data_dirty: c.data_seq_nr() != 0,
                                };
                            }
                        }
                        ChannelKind::Midi => {
                            if let Some(c) = l.midi_channel(channel_idx) {
                                midi = MidiChannelState {
                                    mode: c.mode(),
                                    n_events_triggered: c.n_events_triggered(),
                                    n_notes_active: c.n_notes_active(),
                                    length: c.length(),
                                    start_offset: c.start_offset(),
                                    played_back_sample: c.played_back_sample(),
                                    n_preplay_samples: c.pre_play_samples(),
                                    data_dirty: c.data_seq_nr() != 0,
                                };
                            }
                        }
                    }
                }
            }
            snap.audio_channels.push(audio);
            snap.midi_channels.push(midi);
        }

        // Ports likewise: one arena, two vectors, `None` for the kind a port is not. No name
        // is published -- it is a `String`, and this thread must not touch one. Whoever holds
        // a port handle supplies the name; see `state::AudioPortSnapshot::named`.
        snap.audio_ports.clear();
        snap.midi_ports.clear();
        let port_room = snap.audio_ports.capacity().min(snap.midi_ports.capacity());
        for i in 0..snap.n_ports.min(port_room) {
            let mut audio = None;
            let mut midi = None;
            if let Some(p) = self.session.port(i) {
                if let Some(a) = p.audio() {
                    audio = Some(AudioPortSnapshot {
                        input_peak: a.input_peak(),
                        output_peak: a.output_peak(),
                        gain: a.gain(),
                        muted: a.muted(),
                        passthrough_muted: a.passthrough_muted(),
                        ringbuffer_n_samples: a.ringbuffer_n_samples() as u32,
                    });
                } else if let Some(m) = p.midi() {
                    midi = Some(MidiPortSnapshot {
                        n_input_events: m.n_input_events(),
                        n_input_notes_active: m.n_notes_active(),
                        n_output_events: m.n_output_events(),
                        n_output_notes_active: 0,
                        muted: m.muted(),
                        passthrough_muted: m.passthrough_muted(),
                        ringbuffer_n_samples: m.ringbuffer_n_samples(),
                    });
                }
            }
            snap.audio_ports.push(audio);
            snap.midi_ports.push(midi);
        }

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
    fn publish_graph_staleness(&self) {
        self.stats
            .graph_stale
            .store(!self.session.graph_up_to_date(), Ordering::Relaxed);
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

/// Waits for a result queued by [`EngineHandle::send_for_result`].
///
/// A free function rather than a method, so it is impossible to call while holding the
/// handle: it does not have one. Polls rather than blocking on a condition variable, so the
/// audio thread only ever has to store a result and never has to signal anything.
pub fn wait_for_result<T>(
    mut rx: Consumer<T>,
    timeout: std::time::Duration,
) -> Result<T, WaitError> {
    let started = std::time::Instant::now();
    let deadline = started + timeout;
    let spin_until = started + SPIN_BUDGET;
    loop {
        if let Ok(v) = rx.pop() {
            return Ok(v);
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

/// Reserves room for `wanted` items, if the vector is short of it.
///
/// On the control thread only. Allocating is what the audio thread cannot do, which is why
/// it publishes a short snapshot and reports the shortfall instead of growing anything.
fn grow_to<T>(v: &mut Vec<T>, wanted: usize) {
    if wanted > v.capacity() {
        v.reserve(wanted - v.capacity());
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
        let rx = self.send_for_result(f)?;
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
    pub fn send_for_result<T, F>(&mut self, f: F) -> Result<Consumer<T>, SendError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> T + Send + 'static,
    {
        let (mut tx, rx) = RingBuffer::<T>::new(1);
        let mut f = Some(f);
        self.send(Box::new(move |s: &mut Session| {
            if let Some(f) = f.take() {
                let _ = tx.push(f(s));
            }
        }))?;
        Ok(rx)
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
            grow_to(&mut c.loops, c.n_loops);
            grow_to(&mut c.audio_channels, c.n_channels);
            grow_to(&mut c.midi_channels, c.n_channels);
            grow_to(&mut c.audio_ports, c.n_ports);
            grow_to(&mut c.midi_ports, c.n_ports);
        }
        self.current.as_deref()
    }

    fn recycle(&mut self, mut snap: Box<StateSnapshot>) {
        // Cleared, not shrunk: the capacity is the whole point of handing the box back.
        snap.loops.clear();
        snap.audio_channels.clear();
        snap.midi_channels.clear();
        snap.audio_ports.clear();
        snap.midi_ports.clear();
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

    /// Everything the 40 Hz poll needs, from one cycle: loops, channels and ports together.
    ///
    /// The point of publishing these rather than asking for them one at a time -- a blocking
    /// query per object per frame costs an audio cycle each, which a session with a handful
    /// of tracks cannot afford.
    #[test]
    fn one_cycle_publishes_loops_channels_and_ports_together() {
        use crate::external_audio_port::ExternalAudioPort;
        use crate::external_midi_port::ExternalMidiPort;

        let mut s = Session::default();
        let aport = s.add_port(Port::External(ExternalAudioPort::new(
            "aout",
            PortDirection::Output,
            4,
        )));
        let mport = s.add_port(Port::ExternalMidi(ExternalMidiPort::new(
            "mout",
            PortDirection::Output,
        )));
        let l = s.create_loop();
        let ac = s
            .add_audio_channel(l, 64, ChannelMode::Direct)
            .expect("audio channel");
        let mc = s
            .add_midi_channel(l, 256, ChannelMode::Direct)
            .expect("midi channel");
        s.connect_channel_output(ac, aport).expect("connect audio");
        s.connect_channel_output(mc, mport).expect("connect midi");
        s.loop_mut(l).expect("loop").set_length(64);
        s.apply_graph_changes().expect("schedule");

        // Distinguishable values, so a snapshot that reported another object's numbers or a
        // default would not pass.
        s.loop_mut(l)
            .expect("loop")
            .audio_channel_mut(0)
            .expect("channel")
            .set_gain(0.25);
        s.port_mut(aport)
            .expect("port")
            .audio_mut()
            .expect("audio")
            .set_gain(0.75);
        s.port_mut(mport)
            .expect("port")
            .midi_mut()
            .expect("midi")
            .set_muted(true);
        s.set_loop_mode(l, LoopMode::Playing).expect("mode");

        let (mut e, mut h) = split(s, 16);
        e.process(4);

        let_assert!(Some(snap) = h.poll());
        check!(!snap.truncated());

        check!(snap.n_loops == 1);
        check!(snap.loops[0].mode == LoopMode::Playing);
        check!(snap.loops[0].position == 4);

        // One arena, two vectors: each channel index is filled in exactly one of them.
        check!(snap.n_channels == 2);
        check!(snap.audio_channels[ac].gain == 0.25);
        check!(snap.audio_channels[ac].mode == ChannelMode::Direct);
        check!(snap.midi_channels[mc].mode == ChannelMode::Direct);

        check!(snap.n_ports == 2);
        let_assert!(Some(a) = snap.audio_ports[aport]);
        check!(a.gain == 0.75);
        check!(!a.muted);
        // The audio port is not a MIDI port, and says so rather than shifting the indices.
        check!(snap.midi_ports[aport].is_none());
        let_assert!(Some(m) = snap.midi_ports[mport]);
        check!(m.muted);
        check!(snap.audio_ports[mport].is_none());
    }

    /// The published name problem, asserted from the other side.
    ///
    /// Port names are deliberately absent from a snapshot, because filling one would mean the
    /// audio thread cloning a `String`. The name comes from whoever holds the handle.
    #[test]
    fn a_polled_port_becomes_a_full_state_once_named() {
        use crate::external_audio_port::ExternalAudioPort;

        let mut s = Session::default();
        let p = s.add_port(Port::External(ExternalAudioPort::new(
            "out-1",
            PortDirection::Output,
            4,
        )));
        s.port_mut(p)
            .expect("port")
            .audio_mut()
            .expect("audio")
            .set_gain(0.5);
        s.apply_graph_changes().expect("schedule");

        let (mut e, mut h) = split(s, 16);
        e.process(4);

        let_assert!(Some(snap) = h.poll());
        let_assert!(Some(polled) = snap.audio_ports[p]);
        let full = polled.named("out-1");
        check!(full.name == "out-1");
        check!(full.gain == 0.5);
    }

    /// Channels and ports added after the split get the same grow-on-poll treatment as loops.
    #[test]
    fn more_channels_than_fit_are_reported_then_accommodated() {
        let mut s = Session::default();
        let l = s.create_loop();
        s.apply_graph_changes().expect("schedule");

        // Split while the session is small, so the boxes are sized small, then add well past
        // the floor `split` reserves.
        let (mut e, mut h) = split(s, 16);
        h.send(Box::new(move |s: &mut Session| {
            for _ in 0..40 {
                let _ = s.add_audio_channel(l, 8, ChannelMode::Direct);
            }
            let _ = s.apply_graph_changes();
        }))
        .expect("queue has room");
        e.process(4);

        let_assert!(Some(snap) = h.poll());
        check!(snap.n_channels == 40);
        check!(snap.truncated());
        check!(snap.audio_channels.len() < 40);

        // A few cycles later every box in circulation has been refitted.
        for _ in 0..6 {
            e.process(4);
            h.poll();
        }
        let_assert!(Some(snap) = h.poll());
        check!(!snap.truncated());
        check!(snap.audio_channels.len() == 40);
        check!(snap.midi_channels.len() == 40);
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
            Ok(()) = h.send(Box::new(|s: &mut Session| {
                let _ = s.set_loop_mode(0, LoopMode::Stopped);
            }))
        );

        e.pump();

        // The command landed...
        check!(e.session().loop_(0).expect("loop").mode() == LoopMode::Stopped);
        check!(e.stats().commands_applied.load(Ordering::Relaxed) == 1);
        // ...without a cycle running, so nothing advanced and nothing was published.
        check!(e.stats().cycles.load(Ordering::Relaxed) == 0);
        check!(e.session().loop_(0).expect("loop").position() == 0);
        check!(h.poll().is_none());
        // And the box came back to be freed on this side, as after a cycle.
        check!(h.reclaim() == 1);
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
        check!(e.stats().stale_cycles.load(Ordering::Relaxed) == 1);
        check!(e.stats().cycles.load(Ordering::Relaxed) == 1);
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
