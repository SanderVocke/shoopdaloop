use crate::{BatchTag, ParameterEvent};
use rt_runtime::{buffer::MidiBuffer, Error};

/// Input is borrowed for the call. The transport copies it into its own prepared
/// storage before accepting the batch.
pub struct AsyncBatch<'a> {
    pub tag: BatchTag,
    pub frames: usize,
    /// Planar audio, with `frames` contiguous samples per port.
    pub audio: &'a [f32],
    pub midi: &'a [MidiBuffer],
    pub parameters: &'a [ParameterEvent],
}

pub struct AsyncOutput<'a> {
    pub frames: usize,
    /// Planar audio, with `frames` contiguous samples per port.
    pub audio: &'a [f32],
    pub midi: &'a [MidiBuffer],
}

pub struct AsyncCompletion<'a> {
    pub tag: BatchTag,
    pub output: Result<AsyncOutput<'a>, Error>,
}

/// Prepared render-side endpoint. Methods must be bounded and free of heap
/// work, locks, waits, and OS I/O. A successful submission transfers a copy to
/// the worker; a rejected submission transfers nothing. The worker consumes in
/// submission order and resets stream-local state before processing after a
/// generation change, a sequence gap, or a processing failure.
pub trait AsyncTransport: Send {
    fn try_submit(&mut self, batch: AsyncBatch<'_>) -> Result<(), Error>;

    /// Remove one ready completion and borrow its storage until the next call
    /// on the transport. Expired results still return their original tags.
    fn try_completion(&mut self) -> Option<AsyncCompletion<'_>>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AsyncStats {
    pub submitted: u64,
    pub rejected: u64,
    pub completed: u64,
    pub missed: u64,
    pub stale: u64,
    pub invalid: u64,
    pub failed: u64,
    /// Render frames from the end of a submitted batch to observing its result.
    pub last_completion_frames: u64,
    pub peak_completion_frames: u64,
    /// Submitted batches awaiting a result and still eligible for release.
    pub pending: usize,
    pub peak_pending: usize,
}

/// Source identity of the last rendered quantum. A missing release contains
/// silence and may request MIDI recovery; startup silence has no release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Release {
    pub tag: BatchTag,
    pub offset: usize,
    pub available: bool,
}
