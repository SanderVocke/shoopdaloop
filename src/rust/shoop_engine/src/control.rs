//! Handle-per-object control API used by the application-facing backend interface.
//!
//! Keeps that shape rather than reshaping around snapshots: Python and QML consume
//! those types, so `Loop`, `AudioChannel` and the rest stay handles. What changes is
//! what a handle holds -- an index plus a shared [`EngineHandle`], where the C API
//! version held a `Mutex<*mut T>`.
//!
//! Which primitive each call uses follows from what it needs:
//!
//! - a mutation queues a command and returns immediately, as the C API's setters do;
//! - a read that a published snapshot covers reads the snapshot;
//! - anything else -- a channel's audio data, a peak, a count -- blocks on
//!   [`EngineHandle::send_and_wait`].
//!
//! The mutex is only ever taken on the control thread. The engine is reached solely
//! through the queues inside the handle, so this never touches the session directly.

use crate::channel_mode::ChannelMode;
use crate::engine::{EngineHandle, LoopSnapshot, SendError, WaitError, DEFAULT_WAIT_TIMEOUT};
use crate::loop_mode::LoopMode;
use crate::session::Session;
use crate::state::{AudioChannelState, AudioPortState, MidiChannelState, MidiPortState};

use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("the control handle was poisoned by a panic on another thread")]
    Poisoned,
    #[error(transparent)]
    Send(#[from] SendError),
    #[error(transparent)]
    Wait(#[from] WaitError),
    #[error("no loop at index {0}")]
    NoSuchLoop(usize),
    #[error("no channel at index {0} on loop {1}")]
    NoSuchChannel(usize, usize),
    #[error("no port at index {0}")]
    NoSuchPort(usize),
    #[error(transparent)]
    WrongPortKind(#[from] PortKindError),
}

type Shared = Arc<Mutex<EngineHandle>>;

fn with<T>(
    shared: &Shared,
    f: impl FnOnce(&mut EngineHandle) -> Result<T, ControlError>,
) -> Result<T, ControlError> {
    let mut guard = shared.lock().map_err(|_| ControlError::Poisoned)?;
    f(&mut guard)
}

/// Queues a mutation. Applied at the next cycle boundary.
fn mutate(
    shared: &Shared,
    f: impl FnMut(&mut Session) + Send + 'static,
) -> Result<(), ControlError> {
    let mut f = f;
    with(shared, |h| Ok(h.send(Box::new(move |s| f(s)))?))
}

/// Asks the engine something and waits for the answer.
fn query<T: Send + 'static>(
    shared: &Shared,
    f: impl FnOnce(&mut Session) -> T + Send + 'static,
) -> Result<T, ControlError> {
    with(shared, |h| Ok(h.send_and_wait(f, DEFAULT_WAIT_TIMEOUT)?))
}

/// The session, as the control side sees it. Hands out the other handles.
#[derive(Clone)]
pub struct Backend {
    shared: Shared,
}

impl Backend {
    /// Wraps a handle produced by [`crate::engine::split`].
    pub fn new(handle: EngineHandle) -> Self {
        Self {
            shared: Arc::new(Mutex::new(handle)),
        }
    }

    /// Adds a loop and returns a handle to it.
    ///
    /// Blocking, unlike the setters: the caller needs the index before it can do
    /// anything with the loop, and guessing it would race any other creator.
    pub fn create_loop(&self) -> Result<Loop, ControlError> {
        let idx = query(&self.shared, |s: &mut Session| {
            let idx = s.create_loop();
            // Rescheduling here rather than leaving it to the caller: a session with a
            // stale graph refuses to run, so a half-applied change would silence
            // everything until someone noticed.
            let _ = s.apply_graph_changes();
            idx
        })?;
        Ok(Loop {
            shared: Arc::clone(&self.shared),
            idx,
        })
    }

    /// A handle to an existing loop, if there is one at `idx`.
    pub fn loop_at(&self, idx: usize) -> Result<Loop, ControlError> {
        let exists = query(&self.shared, move |s: &mut Session| s.loop_(idx).is_some())?;
        if !exists {
            return Err(ControlError::NoSuchLoop(idx));
        }
        Ok(Loop {
            shared: Arc::clone(&self.shared),
            idx,
        })
    }

    pub fn n_loops(&self) -> Result<usize, ControlError> {
        query(&self.shared, |s: &mut Session| s.n_loops())
    }

    /// Latest published state of every loop, without blocking.
    ///
    /// Returns nothing until a cycle has run and published one. This is the call a UI
    /// polling at frame rate should use; anything else blocks the audio thread's
    /// progress behind its own.
    pub fn poll_loops(&self) -> Result<Vec<LoopSnapshot>, ControlError> {
        with(&self.shared, |h| {
            Ok(h.poll().map(|s| s.loops.clone()).unwrap_or_default())
        })
    }

    /// Counters the engine and driver publish: cycles, xruns, DSP load.
    pub fn stats(&self) -> Result<Arc<crate::engine::Stats>, ControlError> {
        with(&self.shared, |h| Ok(Arc::clone(h.stats())))
    }
}

/// One loop.
#[derive(Clone)]
pub struct Loop {
    shared: Shared,
    idx: usize,
}

impl Loop {
    pub fn index(&self) -> usize {
        self.idx
    }

    /// This loop's last published state, or `None` if no cycle has published one yet.
    pub fn poll_state(&self) -> Result<Option<LoopSnapshot>, ControlError> {
        let idx = self.idx;
        with(&self.shared, |h| {
            Ok(h.poll().and_then(|s| s.loops.get(idx).cloned()))
        })
    }

    /// This loop's state, asked for directly.
    ///
    /// For a caller that needs it now rather than as of the last cycle. Prefer
    /// [`Self::poll_state`] when polling.
    pub fn get_state(&self) -> Result<LoopSnapshot, ControlError> {
        let idx = self.idx;
        query(&self.shared, move |s: &mut Session| {
            s.loop_(idx).map(|l| {
                let next = l.first_planned_transition();
                LoopSnapshot {
                    mode: l.mode(),
                    length: l.length(),
                    position: l.position(),
                    next_mode: next.map(|(m, _)| m),
                    next_mode_delay: next.map(|(_, d)| d),
                }
            })
        })?
        .ok_or(ControlError::NoSuchLoop(idx))
    }

    pub fn set_length(&self, length: u32) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(l) = s.loop_mut(idx) {
                l.set_length(length);
            }
        })
    }

    pub fn set_position(&self, position: u32) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(l) = s.loop_mut(idx) {
                l.set_position(position);
            }
        })
    }

    /// Switches mode at the next cycle boundary.
    pub fn set_mode(&self, mode: LoopMode) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            let _ = s.set_loop_mode(idx, mode);
        })
    }

    /// Plans a transition, which lands when the sync source says so.
    pub fn plan_transition(
        &self,
        mode: LoopMode,
        n_cycles_delay: Option<u32>,
        to_sync_cycle: Option<u32>,
    ) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(l) = s.loop_mut(idx) {
                l.plan_transition(mode, n_cycles_delay, to_sync_cycle);
            }
        })
    }

    pub fn clear(&self, length: u32) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(l) = s.loop_mut(idx) {
                l.clear(length);
            }
        })
    }

    /// Follows another loop, or stops following when given `None`.
    pub fn set_sync_source(&self, source: Option<&Loop>) -> Result<(), ControlError> {
        let idx = self.idx;
        let src = source.map(|l| l.idx);
        mutate(&self.shared, move |s: &mut Session| {
            let _ = s.set_loop_sync_source(idx, src);
        })
    }

    /// Makes this loop inert: stopped, emptied, and detached from anything syncing to it.
    ///
    /// The handle stays valid and the slot is kept, so nothing else's index moves. A caller that
    /// tracks its own grid should forget the cell; the engine simply stops it mattering.
    pub fn remove(&self) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if s.remove_loop(idx).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }

    pub fn add_audio_channel(
        &self,
        chunk_size: usize,
        mode: ChannelMode,
    ) -> Result<AudioChannel, ControlError> {
        let idx = self.idx;
        let (session_idx, chan_idx) = query(&self.shared, move |s: &mut Session| {
            let added = s
                .add_audio_channel(idx, chunk_size, mode)
                .ok()
                .map(|session_idx| {
                    (
                        session_idx,
                        s.loop_(idx).map_or(0, |l| l.n_audio_channels() - 1),
                    )
                });
            let _ = s.apply_graph_changes();
            added
        })?
        .ok_or(ControlError::NoSuchLoop(idx))?;
        Ok(AudioChannel {
            shared: Arc::clone(&self.shared),
            loop_idx: idx,
            chan_idx,
            session_idx,
        })
    }

    pub fn add_midi_channel(
        &self,
        capacity: usize,
        mode: ChannelMode,
    ) -> Result<MidiChannel, ControlError> {
        let idx = self.idx;
        let (session_idx, chan_idx) = query(&self.shared, move |s: &mut Session| {
            let added = s
                .add_midi_channel(idx, capacity, mode)
                .ok()
                .map(|session_idx| {
                    (
                        session_idx,
                        s.loop_(idx).map_or(0, |l| l.n_midi_channels() - 1),
                    )
                });
            let _ = s.apply_graph_changes();
            added
        })?
        .ok_or(ControlError::NoSuchLoop(idx))?;
        Ok(MidiChannel {
            shared: Arc::clone(&self.shared),
            loop_idx: idx,
            chan_idx,
            session_idx,
        })
    }
}

/// One audio channel of one loop.
#[derive(Clone)]
pub struct AudioChannel {
    shared: Shared,
    loop_idx: usize,
    /// Index within the loop, which is how the loop finds it.
    chan_idx: usize,
    /// Index within the session's channel arena, which is what connections use.
    session_idx: usize,
}

impl AudioChannel {
    pub fn index(&self) -> usize {
        self.chan_idx
    }

    pub fn get_state(&self) -> Result<AudioChannelState, ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        query(&self.shared, move |s: &mut Session| {
            s.loop_(li).and_then(|l| l.audio_channel(ci)).map(|c| {
                AudioChannelState {
                    mode: c.mode(),
                    gain: c.gain(),
                    output_peak: c.output_peak(),
                    length: c.length() as u32,
                    start_offset: c.start_offset(),
                    played_back_sample: c.played_back_sample(),
                    n_preplay_samples: c.pre_play_samples(),
                    // Sequence number rather than a flag: the C API had the caller clear
                    // a dirty bit, which races anything else watching it.
                    data_dirty: c.data_seq_nr() != 0,
                }
            })
        })?
        .ok_or(ControlError::NoSuchChannel(ci, li))
    }

    /// Reads the channel's samples back.
    ///
    /// Two round trips: one for the length, one to fill a buffer sized from it. The
    /// buffer is allocated here and handed over, so the engine never allocates.
    pub fn get_data(&self) -> Result<Vec<f32>, ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        let len = query(&self.shared, move |s: &mut Session| {
            s.loop_(li)
                .and_then(|l| l.audio_channel(ci))
                .map(|c| c.length())
        })?
        .ok_or(ControlError::NoSuchChannel(ci, li))?;

        let mut buf = vec![0.0f32; len];
        buf = query(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_(li).and_then(|l| l.audio_channel(ci)) {
                let n = buf.len().min(c.length());
                for (i, slot) in buf[..n].iter_mut().enumerate() {
                    *slot = c.at(i).unwrap_or(0.0);
                }
            }
            buf
        })?;
        Ok(buf)
    }

    /// Replaces the channel's contents.
    ///
    /// The data is moved into the command, so the engine copies rather than allocating.
    pub fn load_data(&self, data: &[f32]) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        let owned = data.to_vec();
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.audio_channel_mut(ci)) {
                c.load_data(&owned);
            }
        })
    }

    pub fn set_gain(&self, gain: f32) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.audio_channel_mut(ci)) {
                c.set_gain(gain);
            }
        })
    }

    pub fn set_mode(&self, mode: ChannelMode) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.audio_channel_mut(ci)) {
                c.set_mode(mode);
            }
        })
    }

    pub fn set_start_offset(&self, offset: i32) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.audio_channel_mut(ci)) {
                c.set_start_offset(offset);
            }
        })
    }

    pub fn set_n_preplay_samples(&self, n: u32) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.audio_channel_mut(ci)) {
                c.set_pre_play_samples(n);
            }
        })
    }

    pub fn clear(&self, length: usize) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.audio_channel_mut(ci)) {
                c.clear(length);
            }
        })
    }

    /// Disconnects this channel and disables it.
    pub fn remove(&self) -> Result<(), ControlError> {
        let ci = self.session_idx;
        mutate(&self.shared, move |s: &mut Session| {
            if s.remove_audio_channel(ci).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }
}

/// One MIDI channel of one loop.
#[derive(Clone)]
pub struct MidiChannel {
    shared: Shared,
    loop_idx: usize,
    chan_idx: usize,
    session_idx: usize,
}

impl MidiChannel {
    pub fn index(&self) -> usize {
        self.chan_idx
    }

    pub fn get_state(&self) -> Result<MidiChannelState, ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        query(&self.shared, move |s: &mut Session| {
            s.loop_(li)
                .and_then(|l| l.midi_channel(ci))
                .map(|c| MidiChannelState {
                    mode: c.mode(),
                    n_events_triggered: c.n_events_triggered(),
                    n_notes_active: c.n_notes_active(),
                    length: c.length(),
                    start_offset: c.start_offset(),
                    played_back_sample: c.played_back_sample(),
                    n_preplay_samples: c.pre_play_samples(),
                    data_dirty: c.data_seq_nr() != 0,
                })
        })?
        .ok_or(ControlError::NoSuchChannel(ci, li))
    }

    pub fn set_mode(&self, mode: ChannelMode) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.midi_channel_mut(ci)) {
                c.set_mode(mode);
            }
        })
    }

    pub fn set_start_offset(&self, offset: i32) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.midi_channel_mut(ci)) {
                c.set_start_offset(offset);
            }
        })
    }

    pub fn set_n_preplay_samples(&self, n: u32) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.midi_channel_mut(ci)) {
                c.set_pre_play_samples(n);
            }
        })
    }

    pub fn clear(&self) -> Result<(), ControlError> {
        let (li, ci) = (self.loop_idx, self.chan_idx);
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(li).and_then(|l| l.midi_channel_mut(ci)) {
                c.clear();
            }
        })
    }

    /// Disconnects this channel and disables it.
    pub fn remove(&self) -> Result<(), ControlError> {
        let ci = self.session_idx;
        mutate(&self.shared, move |s: &mut Session| {
            if s.remove_midi_channel(ci).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }
}

/// One port of the session, of either data type.
///
/// A single handle rather than one type per data type: the session keeps ports in one
/// arena, so an index is all that identifies one, and a caller asking an audio port
/// for MIDI counts gets an error rather than a different handle type it cannot hold.
#[derive(Clone)]
pub struct Port {
    shared: Shared,
    idx: usize,
}

#[derive(Debug, Error)]
pub enum PortKindError {
    #[error("port {0} is not an audio port")]
    NotAudio(usize),
    #[error("port {0} is not a MIDI port")]
    NotMidi(usize),
}

impl Port {
    pub fn index(&self) -> usize {
        self.idx
    }

    pub fn name(&self) -> Result<String, ControlError> {
        let idx = self.idx;
        query(&self.shared, move |s: &mut Session| {
            s.port(idx).map(|p| p.name().to_string())
        })?
        .ok_or(ControlError::NoSuchPort(idx))
    }

    pub fn get_audio_state(&self) -> Result<AudioPortState, ControlError> {
        let idx = self.idx;
        query(&self.shared, move |s: &mut Session| {
            let p = s.port(idx)?;
            let audio = p.audio()?;
            Some(AudioPortState {
                input_peak: audio.input_peak(),
                output_peak: audio.output_peak(),
                gain: audio.gain(),
                muted: audio.muted(),
                passthrough_muted: audio.passthrough_muted(),
                ringbuffer_n_samples: audio.ringbuffer_n_samples() as u32,
                name: p.name().to_string(),
            })
        })?
        .ok_or(ControlError::WrongPortKind(PortKindError::NotAudio(idx)))
    }

    pub fn get_midi_state(&self) -> Result<MidiPortState, ControlError> {
        let idx = self.idx;
        query(&self.shared, move |s: &mut Session| {
            let p = s.port(idx)?;
            let midi = p.midi()?;
            Some(MidiPortState {
                n_input_events: midi.n_input_events(),
                n_input_notes_active: midi.n_notes_active(),
                n_output_events: midi.n_output_events(),
                n_output_notes_active: 0,
                muted: midi.muted(),
                passthrough_muted: midi.passthrough_muted(),
                ringbuffer_n_samples: midi.ringbuffer_n_samples(),
                name: p.name().to_string(),
            })
        })?
        .ok_or(ControlError::WrongPortKind(PortKindError::NotMidi(idx)))
    }

    /// Ignored by a MIDI port, which has no gain.
    pub fn set_gain(&self, gain: f32) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(a) = s.port_mut(idx).and_then(|p| p.audio_mut()) {
                a.set_gain(gain);
            }
        })
    }

    /// Applies to whichever kind this port is.
    pub fn set_muted(&self, muted: bool) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(p) = s.port_mut(idx) {
                if let Some(a) = p.audio_mut() {
                    a.set_muted(muted);
                } else if let Some(m) = p.midi_mut() {
                    m.set_muted(muted);
                }
            }
        })
    }

    /// Sets the retroactive-capture window, in frames.
    pub fn set_ringbuffer_n_samples(&self, n: u32) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if let Some(p) = s.port_mut(idx) {
                if let Some(a) = p.audio_mut() {
                    a.set_ringbuffer_n_samples(n as usize);
                } else if let Some(m) = p.midi_mut() {
                    m.set_ringbuffer_n_samples(n);
                }
            }
        })
    }

    /// Disconnects this port from everything.
    pub fn remove(&self) -> Result<(), ControlError> {
        let idx = self.idx;
        mutate(&self.shared, move |s: &mut Session| {
            if s.remove_port(idx).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }

    /// Routes this port's output into `other`, inside the engine.
    pub fn connect_internal(&self, other: &Port) -> Result<(), ControlError> {
        let (from, to) = (self.idx, other.idx);
        mutate(&self.shared, move |s: &mut Session| {
            if s.connect_ports_internal(from, to).is_ok() {
                // Connecting changes the graph, so reschedule in the same command
                // rather than leaving the session refusing to run.
                let _ = s.apply_graph_changes();
            }
        })
    }
}

impl Backend {
    /// Adds a port fed by a driver, and returns a handle to it.
    ///
    /// Blocking for the same reason as `create_loop`: the caller needs the index.
    pub fn add_audio_port(
        &self,
        name: &str,
        direction: crate::port::PortDirection,
        ringbuffer_buffer_size: usize,
    ) -> Result<Port, ControlError> {
        let name = name.to_string();
        let idx = query(&self.shared, move |s: &mut Session| {
            let idx = s.add_port(crate::session::Port::External(
                crate::external_audio_port::ExternalAudioPort::new(
                    name,
                    direction,
                    ringbuffer_buffer_size,
                ),
            ));
            let _ = s.apply_graph_changes();
            idx
        })?;
        Ok(Port {
            shared: Arc::clone(&self.shared),
            idx,
        })
    }

    pub fn add_midi_port(
        &self,
        name: &str,
        direction: crate::port::PortDirection,
    ) -> Result<Port, ControlError> {
        let name = name.to_string();
        let idx = query(&self.shared, move |s: &mut Session| {
            let idx = s.add_port(crate::session::Port::ExternalMidi(
                crate::external_midi_port::ExternalMidiPort::new(name, direction),
            ));
            let _ = s.apply_graph_changes();
            idx
        })?;
        Ok(Port {
            shared: Arc::clone(&self.shared),
            idx,
        })
    }

    /// Adds a port that only routes inside the engine.
    pub fn add_internal_audio_port(
        &self,
        name: &str,
        n_frames: usize,
        ringbuffer_buffer_size: usize,
    ) -> Result<Port, ControlError> {
        let name = name.to_string();
        let idx = query(&self.shared, move |s: &mut Session| {
            let idx = s.add_port(crate::session::Port::Internal(
                crate::internal_audio_port::InternalAudioPort::new(
                    name,
                    n_frames,
                    crate::port::PortConnectability::INTERNAL,
                    crate::port::PortConnectability::INTERNAL,
                    ringbuffer_buffer_size,
                ),
            ));
            let _ = s.apply_graph_changes();
            idx
        })?;
        Ok(Port {
            shared: Arc::clone(&self.shared),
            idx,
        })
    }

    pub fn n_ports(&self) -> Result<usize, ControlError> {
        query(&self.shared, |s: &mut Session| s.n_ports())
    }
}

impl AudioChannel {
    /// Reads this channel's input from `port`.
    pub fn connect_input(&self, port: &Port) -> Result<(), ControlError> {
        let (ci, pi) = (self.session_idx, port.idx);
        mutate(&self.shared, move |s: &mut Session| {
            if s.connect_channel_input(ci, pi).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }

    /// Writes this channel's output to `port`.
    pub fn connect_output(&self, port: &Port) -> Result<(), ControlError> {
        let (ci, pi) = (self.session_idx, port.idx);
        mutate(&self.shared, move |s: &mut Session| {
            if s.connect_channel_output(ci, pi).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }
}

impl MidiChannel {
    pub fn connect_input(&self, port: &Port) -> Result<(), ControlError> {
        let (ci, pi) = (self.session_idx, port.idx);
        mutate(&self.shared, move |s: &mut Session| {
            if s.connect_channel_input(ci, pi).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }

    pub fn connect_output(&self, port: &Port) -> Result<(), ControlError> {
        let (ci, pi) = (self.session_idx, port.idx);
        mutate(&self.shared, move |s: &mut Session| {
            if s.connect_channel_output(ci, pi).is_ok() {
                let _ = s.apply_graph_changes();
            }
        })
    }
}
