//! DummyMidiPort - A MIDI port implementation for testing and development in Rust.
//!
//! Inherits from MidiPort which uses composition (MidiPortBase) for state tracking
//! and ringbuffer functionality. Implements buffer interfaces for test use.
//!
//! Ported from C++ DummyMidiPort.h/cpp

use std::sync::atomic::{AtomicU32, Ordering};

use crate::midi_port::MidiPort;
use crate::midi_storage::MidiStorageElem;
use crate::midi_traits::{MidiReadableBuffer, MidiWritableBuffer};

/// Direction for the port
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
}

/// DummyMidiPort - A MIDI port implementation for testing and development.
///
/// Maintains internal buffers for test messages, supports queue/dequeue operations.
pub struct DummyMidiPort {
    // Base MIDI port (provides state tracking, ringbuffer, etc.)
    base: MidiPort,

    // Direction (input or output)
    direction: PortDirection,

    // Queued messages as external input to the port
    queued_msgs: Vec<MidiStorageElem>,

    // Buffer data for internal use
    buffer_data: Vec<MidiStorageElem>,

    // Messages written during the requested period
    pub written_requested_msgs: Vec<MidiStorageElem>,

    // Current buffer frame count
    current_buf_frames: AtomicU32,

    // Amount of frames requested for reading externally out of the port
    n_requested_frames: AtomicU32,

    // Number of frames processed in last round
    n_processed_last_round: AtomicU32,

    // Original requested frames
    n_original_requested_frames: AtomicU32,
}

impl DummyMidiPort {
    /// Create a new DummyMidiPort
    pub fn new(direction: PortDirection) -> Self {
        DummyMidiPort {
            base: MidiPort::new(true, true, true), // track_notes, track_controls, track_programs
            direction,
            queued_msgs: Vec::new(),
            buffer_data: Vec::new(),
            written_requested_msgs: Vec::new(),
            current_buf_frames: AtomicU32::new(0),
            n_requested_frames: AtomicU32::new(0),
            n_processed_last_round: AtomicU32::new(0),
            n_original_requested_frames: AtomicU32::new(0),
        }
    }

    /// Get reference to base MidiPort
    pub fn base(&self) -> &MidiPort {
        &self.base
    }

    /// Get mutable reference to base MidiPort
    pub fn base_mut(&mut self) -> &mut MidiPort {
        &mut self.base
    }

    /// Queue a message for input
    pub fn queue_msg(&mut self, time: u32, size: u16, data: &[u8]) {
        if let Some(elem) = MidiStorageElem::new(time, size, data) {
            self.queued_msgs.push(elem);
            // Sort by time
            self.queued_msgs.sort_by(|a, b| a.time.cmp(&b.time));
        }
    }

    /// Check if queue is empty
    pub fn get_queue_empty(&self) -> bool {
        self.queued_msgs.is_empty()
    }

    /// Clear all queues
    pub fn clear_queues(&mut self) {
        self.queued_msgs.clear();
        self.written_requested_msgs.clear();
        self.n_original_requested_frames.store(0, Ordering::SeqCst);
        self.n_requested_frames.store(0, Ordering::SeqCst);
    }

    /// Request a certain number of frames to be stored
    pub fn request_data(&mut self, n_frames: u32) {
        if self.n_requested_frames.load(Ordering::SeqCst) > 0 {
            panic!("Previous request not yet completed");
        }
        self.n_requested_frames.store(n_frames, Ordering::SeqCst);
        self.n_original_requested_frames
            .store(n_frames, Ordering::SeqCst);
    }

    /// Get messages written during the requested period
    pub fn get_written_requested_msgs(&mut self) -> Vec<MidiStorageElem> {
        let result = self.written_requested_msgs.clone();
        self.written_requested_msgs.clear();
        result
    }

    /// Get count of written messages
    pub fn get_n_written_requested_msgs(&self) -> u32 {
        self.written_requested_msgs.len() as u32
    }

    /// Get time of written message at index
    pub fn get_written_msg_time(&self, idx: u32) -> u32 {
        self.written_requested_msgs
            .get(idx as usize)
            .map(|e| e.time)
            .unwrap_or(u32::MAX)
    }

    /// Get size of written message at index
    pub fn get_written_msg_size(&self, idx: u32) -> u16 {
        self.written_requested_msgs
            .get(idx as usize)
            .map(|e| e.size)
            .unwrap_or(u16::MAX)
    }

    /// Get bytes of written message at index (fills output buffer)
    pub unsafe fn get_written_msg_bytes(&self, idx: u32, out: *mut u8, max_len: usize) {
        if let Some(elem) = self.written_requested_msgs.get(idx as usize) {
            let len = std::cmp::min(elem.size as usize, max_len);
            std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
        }
    }

    /// Clear written messages
    pub fn clear_written_msgs(&mut self) {
        self.written_requested_msgs.clear();
    }

    /// Write an event directly (for FFI boundary)
    pub fn write_event_raw(&mut self, time: u32, size: u16, data: &[u8]) {
        if let Some(elem) = MidiStorageElem::new(time, size, data) {
            self.buffer_data.push(elem);
        }
    }

    /// Prepare for processing
    pub fn prepare(&mut self, nframes: u32) {
        self.buffer_data.clear();

        let progress_by = self.n_processed_last_round.load(Ordering::SeqCst);
        let mut new_progress = progress_by;
        new_progress = new_progress.saturating_sub(
            self.n_requested_frames
                .load(Ordering::SeqCst)
                .min(progress_by),
        );

        if new_progress > 0 && !self.queued_msgs.is_empty() {
            // Truncate old messages
            self.queued_msgs.retain(|msg| msg.time >= new_progress);
            // Adjust timestamps
            for msg in &mut self.queued_msgs {
                msg.time = msg.time.saturating_sub(new_progress);
            }
        }

        self.n_processed_last_round.store(0, Ordering::SeqCst);
        self.current_buf_frames.store(nframes, Ordering::SeqCst);
    }

    /// Process frames
    pub fn process(&mut self, nframes: u32) {
        if self.direction == PortDirection::Output {
            // Sort buffer data by time
            self.buffer_data.sort_by(|a, b| a.time.cmp(&b.time));

            if !self.base.get_muted() {
                let n_requested = self.n_requested_frames.load(Ordering::SeqCst);
                let n_original = self.n_original_requested_frames.load(Ordering::SeqCst);

                for msg in &self.buffer_data {
                    if msg.time < n_requested {
                        let new_time = msg.time + (n_original.saturating_sub(n_requested));
                        let mut out_msg = msg.clone();
                        out_msg.time = new_time;
                        self.written_requested_msgs.push(out_msg);
                        // Track event through base
                        self.base.increment_output_events(1);
                    }
                }
            }
        }

        self.n_processed_last_round.store(nframes, Ordering::SeqCst);
        let n_req = self.n_requested_frames.load(Ordering::SeqCst);
        self.n_requested_frames
            .store(n_req.saturating_sub(nframes.min(n_req)), Ordering::SeqCst);
    }

    /// Get the direction
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
}

// Implement MidiReadableBuffer for DummyMidiPort
impl MidiReadableBuffer for DummyMidiPort {
    fn n_events(&self) -> u32 {
        if !self.queued_msgs.is_empty() {
            let mut count = 0u32;
            for msg in &self.queued_msgs {
                if msg.time < self.current_buf_frames.load(Ordering::SeqCst) {
                    count += 1;
                } else {
                    break;
                }
            }
            return count;
        }
        self.buffer_data.len() as u32
    }

    fn get_event(&self, idx: u32) -> Option<MidiStorageElem> {
        if !self.queued_msgs.is_empty() {
            return self.queued_msgs.get(idx as usize).cloned();
        }
        self.buffer_data.get(idx as usize).cloned()
    }
}

// Implement MidiWritableBuffer for DummyMidiPort
impl MidiWritableBuffer for DummyMidiPort {
    fn write_event(&mut self, event: MidiStorageElem) -> bool {
        self.buffer_data.push(event);
        true
    }
}

// Delegation methods for CXX bridge compatibility
impl DummyMidiPort {
    /// Get mute state
    pub fn get_muted(&self) -> bool {
        self.base.get_muted()
    }

    /// Set mute state
    pub fn set_muted(&mut self, muted: bool) {
        self.base.set_muted(muted);
    }

    /// Get number of active notes
    pub fn n_notes_active(&self) -> u32 {
        self.base.n_notes_active()
    }

    /// Get number of events (from MidiReadableBuffer trait)
    pub fn n_events(&self) -> u32 {
        MidiReadableBuffer::n_events(self)
    }

    /// Get event time at index (from MidiReadableBuffer trait)
    pub fn get_event_time(&self, idx: u32) -> u32 {
        MidiReadableBuffer::get_event(self, idx)
            .map(|e| e.time)
            .unwrap_or(u32::MAX)
    }

    /// Get event size at index (from MidiReadableBuffer trait)
    pub fn get_event_size(&self, idx: u32) -> u16 {
        MidiReadableBuffer::get_event(self, idx)
            .map(|e| e.size)
            .unwrap_or(u16::MAX)
    }

    /// Get event bytes at index (from MidiReadableBuffer trait)
    pub unsafe fn get_event_bytes(&self, idx: u32, out: *mut u8, max_len: usize) {
        if let Some(elem) = MidiReadableBuffer::get_event(self, idx) {
            let len = std::cmp::min(elem.size as usize, max_len);
            std::ptr::copy_nonoverlapping(elem.data().as_ptr(), out, len);
        }
    }

    /// Write event (for CXX bridge - delegates to MidiWritableBuffer trait)
    pub fn write_event_cxx(&mut self, time: u32, size: u16, data: &[u8]) {
        self.write_event_raw(time, size, data);
    }

    /// Dummy write event for CXX bridge (same as write_event_cxx)
    pub fn dummy_write_event(&mut self, time: u32, size: u16, data: &[u8]) {
        self.write_event_cxx(time, size, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dummy_midi_port_creation() {
        let port = DummyMidiPort::new(PortDirection::Input);
        assert!(!port.base.get_muted());
    }

    #[test]
    fn test_queue_message() {
        let mut port = DummyMidiPort::new(PortDirection::Input);
        port.queue_msg(100, 3, &[0x90, 0x3C, 0x7F]);
        assert!(!port.get_queue_empty());
    }

    #[test]
    fn test_clear_queues() {
        let mut port = DummyMidiPort::new(PortDirection::Input);
        port.queue_msg(100, 3, &[0x90, 0x3C, 0x7F]);
        assert!(!port.get_queue_empty());
        port.clear_queues();
        assert!(port.get_queue_empty());
    }

    #[test]
    fn test_readable_buffer() {
        let mut port = DummyMidiPort::new(PortDirection::Input);
        port.queue_msg(50, 3, &[0x90, 0x3C, 0x7F]);
        port.queue_msg(100, 3, &[0x90, 0x40, 0x7F]);

        port.prepare(200);
        assert_eq!(port.n_events(), 2);
    }

    #[test]
    fn test_writable_buffer() {
        let mut port = DummyMidiPort::new(PortDirection::Output);
        let event = MidiStorageElem::new(50, 3, &[0x90, 0x3C, 0x7F]).unwrap();
        assert!(port.write_event(event));
        assert_eq!(port.n_events(), 1);
    }

    #[test]
    fn test_request_data() {
        let mut port = DummyMidiPort::new(PortDirection::Input);
        port.request_data(1024);
        assert_eq!(port.n_requested_frames.load(Ordering::SeqCst), 1024);
    }
}
