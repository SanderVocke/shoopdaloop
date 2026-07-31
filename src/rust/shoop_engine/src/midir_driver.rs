//! MIDI capture and playback for hosts without JACK, via `midir`.
//!
//! Behind the `midir` feature. Deliberately not a driver of its own: `midir` gives no
//! audio clock, so it pairs with whichever audio driver is running -- the cpal one, in
//! practice. What it provides is a [`MidiCapture`] to drain into an
//! [`ExternalMidiPort`] from that driver's callback, and a [`MidiPlayback`] to send
//! what a port produced.
//!
//! **Timing is coarser than JACK's, unavoidably.** `midir` timestamps in host-clock
//! microseconds with no relationship to the audio callback's frame counter, so a
//! message cannot be placed at the frame it truly arrived at. Everything pending is
//! staged at frame 0 of the next cycle, which costs up to one buffer of jitter. That
//! is the price of the non-JACK path and there is no way to avoid it without a shared
//! clock; JACK's single callback is what makes sample-exact MIDI possible there.
//!
//! Messages longer than [`crate::midi_storage::MAX_MSG_BYTES`] are refused and counted
//! rather than truncated, so sysex is dropped visibly instead of arriving corrupt.

use crate::external_midi_port::ExternalMidiPort;
use crate::midi_storage::{MidiStorageElem, MAX_MSG_BYTES};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use rtrb::{Consumer, Producer, RingBuffer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MidirError {
    #[error("could not open MIDI input: {0}")]
    Init(#[from] midir::InitError),
    #[error("could not connect the MIDI input: {0}")]
    ConnectInput(#[from] midir::ConnectError<MidiInput>),
    #[error("could not connect the MIDI output: {0}")]
    ConnectOutput(#[from] midir::ConnectError<MidiOutput>),
    #[error("no MIDI port matching {0:?}")]
    NoSuchPort(String),
    #[error("could not send: {0}")]
    Send(#[from] midir::SendError),
}

/// Messages held between the MIDI callback and the audio cycle that consumes them.
///
/// A generous ring: MIDI is sparse, and the cost of a slot is a few bytes, so there is
/// no reason to make a dense passage of notes contend for room.
const RING_CAPACITY: usize = 1024;

/// The audio side of a `midir` input: drained once per cycle.
pub struct MidiCapture {
    rx: Consumer<MidiStorageElem>,
    /// Messages the callback refused for being too long.
    n_refused: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// Messages dropped because the ring was full.
    n_dropped: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl MidiCapture {
    /// Drains everything pending, all retimed to frame 0 of the next audio cycle.
    pub fn drain_pending(&mut self) -> Vec<MidiStorageElem> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.pop() {
            out.push(e.at_time(0));
        }
        out
    }

    /// Stages everything pending into `port`, all at frame 0.
    ///
    /// Returns how many were staged. Call from the audio callback before the cycle runs,
    /// so the port's `prepare` picks them up.
    pub fn drain_into(&mut self, port: &mut ExternalMidiPort) -> usize {
        let events = self.drain_pending();
        let mut n = 0;
        for e in events {
            if port.push_incoming(0, e.data()) {
                n += 1;
            }
        }
        n
    }

    /// Messages refused for exceeding the maximum payload size.
    pub fn n_refused(&self) -> u32 {
        self.n_refused.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Messages dropped because the ring filled up.
    pub fn n_dropped(&self) -> u32 {
        self.n_dropped.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// A live `midir` input connection. Dropping it stops capture.
///
/// Held separately from [`MidiCapture`] because the capture end goes to the audio
/// thread while this stays with whoever opened it.
pub struct MidiCaptureConnection {
    _conn: MidiInputConnection<()>,
}

fn make_callback(
    mut tx: Producer<MidiStorageElem>,
    n_refused: std::sync::Arc<std::sync::atomic::AtomicU32>,
    n_dropped: std::sync::Arc<std::sync::atomic::AtomicU32>,
) -> impl FnMut(u64, &[u8], &mut ()) + Send + 'static {
    move |_timestamp, bytes, _| {
        use std::sync::atomic::Ordering;
        if bytes.is_empty() || bytes.len() > MAX_MSG_BYTES {
            n_refused.fetch_add(1, Ordering::Relaxed);
            return;
        }
        match MidiStorageElem::new(0, bytes) {
            Some(e) => {
                if tx.push(e).is_err() {
                    n_dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            None => {
                n_refused.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn split_capture() -> (
    Producer<MidiStorageElem>,
    MidiCapture,
    std::sync::Arc<std::sync::atomic::AtomicU32>,
    std::sync::Arc<std::sync::atomic::AtomicU32>,
) {
    let (tx, rx) = RingBuffer::new(RING_CAPACITY);
    let n_refused = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let n_dropped = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let capture = MidiCapture {
        rx,
        n_refused: std::sync::Arc::clone(&n_refused),
        n_dropped: std::sync::Arc::clone(&n_dropped),
    };
    (tx, capture, n_refused, n_dropped)
}

/// Opens the first input port whose name contains `pattern`.
pub fn open_input(
    client_name: &str,
    port_name: &str,
    pattern: &str,
) -> Result<(MidiCapture, MidiCaptureConnection), MidirError> {
    let mut input = MidiInput::new(client_name)?;
    input.ignore(midir::Ignore::None);

    let port = input
        .ports()
        .into_iter()
        .find(|p| {
            input
                .port_name(p)
                .map(|n| n.contains(pattern))
                .unwrap_or(false)
        })
        .ok_or_else(|| MidirError::NoSuchPort(pattern.to_string()))?;

    let (tx, capture, refused, dropped) = split_capture();
    let conn = input.connect(&port, port_name, make_callback(tx, refused, dropped), ())?;
    Ok((capture, MidiCaptureConnection { _conn: conn }))
}

/// Creates a virtual input port that other applications can send to.
///
/// Unix only, which is what `midir` supports it on.
#[cfg(unix)]
pub fn create_virtual_input(
    client_name: &str,
    port_name: &str,
) -> Result<(MidiCapture, MidiCaptureConnection), MidirError> {
    use midir::os::unix::VirtualInput;

    let mut input = MidiInput::new(client_name)?;
    input.ignore(midir::Ignore::None);

    let (tx, capture, refused, dropped) = split_capture();
    let conn = input.create_virtual(port_name, make_callback(tx, refused, dropped), ())?;
    Ok((capture, MidiCaptureConnection { _conn: conn }))
}

/// The sending half: hands a port's output to a `midir` connection.
pub struct MidiPlayback {
    conn: MidiOutputConnection,
    n_failed: u32,
}

impl MidiPlayback {
    /// Sends everything `port` produced this cycle.
    ///
    /// Timestamps are dropped: `midir` sends immediately and has no way to schedule
    /// within a buffer, which is the same one-buffer imprecision as capture.
    pub fn send_from(&mut self, port: &ExternalMidiPort) -> usize {
        let mut n = 0;
        for e in port.outgoing() {
            match self.conn.send(e.data()) {
                Ok(()) => n += 1,
                Err(_) => self.n_failed += 1,
            }
        }
        n
    }

    pub fn send_events(&mut self, events: &[MidiStorageElem]) -> usize {
        let mut n = 0;
        for e in events {
            match self.conn.send(e.data()) {
                Ok(()) => n += 1,
                Err(_) => self.n_failed += 1,
            }
        }
        n
    }

    pub fn n_failed(&self) -> u32 {
        self.n_failed
    }
}

/// Opens the first output port whose name contains `pattern`.
pub fn open_output(
    client_name: &str,
    port_name: &str,
    pattern: &str,
) -> Result<MidiPlayback, MidirError> {
    let output = MidiOutput::new(client_name)?;
    let port = output
        .ports()
        .into_iter()
        .find(|p| {
            output
                .port_name(p)
                .map(|n| n.contains(pattern))
                .unwrap_or(false)
        })
        .ok_or_else(|| MidirError::NoSuchPort(pattern.to_string()))?;
    let conn = output.connect(&port, port_name)?;
    Ok(MidiPlayback { conn, n_failed: 0 })
}
