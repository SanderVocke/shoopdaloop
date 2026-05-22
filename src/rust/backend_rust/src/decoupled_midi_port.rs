//! DecoupledMidiPort - Lock-free SPSC queue for MIDI messages.
//!
//! Replaces boost::lockfree::spsc_queue with crossbeam_queue::ArrayQueue.
//! The queue is bounded; messages are silently dropped when full.

use crossbeam_queue::ArrayQueue;

use crate::midi_storage::MidiStorageElem;
use crate::port_direction::PortDirection;

pub struct DecoupledMidiPort {
    queue: ArrayQueue<MidiStorageElem>,
    direction: PortDirection,
}

impl DecoupledMidiPort {
    pub fn new(queue_size: usize, direction: PortDirection) -> Self {
        DecoupledMidiPort {
            queue: ArrayQueue::new(queue_size),
            direction,
        }
    }

    /// Push a message into the queue.
    /// Returns true if queued, false if the queue was full (message dropped).
    pub fn push(&self, msg: MidiStorageElem) -> bool {
        self.queue.push(msg).is_ok()
    }

    /// Pop a message from the queue.
    /// Returns Some(msg) if a message was available, None if empty.
    pub fn pop(&self) -> Option<MidiStorageElem> {
        self.queue.pop()
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get the port direction.
    pub fn direction(&self) -> PortDirection {
        self.direction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let port = DecoupledMidiPort::new(4, PortDirection::Input);
        assert!(port.is_empty());
    }

    #[test]
    fn test_push_and_pop() {
        let port = DecoupledMidiPort::new(4, PortDirection::Input);
        let msg = MidiStorageElem::new(100, 3, &[0x90, 0x3C, 0x7F]).unwrap();
        assert!(port.push(msg.clone()));
        assert!(!port.is_empty());

        let popped = port.pop();
        assert!(popped.is_some());
        assert_eq!(popped.unwrap(), msg);
        assert!(port.is_empty());
    }

    #[test]
    fn test_push_drops_when_full() {
        let port = DecoupledMidiPort::new(2, PortDirection::Output);
        let msg1 = MidiStorageElem::new(10, 1, &[0x90]).unwrap();
        let msg2 = MidiStorageElem::new(20, 1, &[0x91]).unwrap();
        let msg3 = MidiStorageElem::new(30, 1, &[0x92]).unwrap();

        assert!(port.push(msg1.clone()));
        assert!(port.push(msg2.clone()));
        // Queue is now full; next push should fail/drop
        assert!(!port.push(msg3.clone()));

        // Only the first two messages should be retrievable
        assert_eq!(port.pop(), Some(msg1));
        assert_eq!(port.pop(), Some(msg2));
        assert_eq!(port.pop(), None);
    }

    #[test]
    fn test_empty_pop_returns_none() {
        let port = DecoupledMidiPort::new(4, PortDirection::Input);
        assert_eq!(port.pop(), None);
    }

    #[test]
    fn test_direction() {
        let input_port = DecoupledMidiPort::new(4, PortDirection::Input);
        let output_port = DecoupledMidiPort::new(4, PortDirection::Output);
        assert_eq!(input_port.direction(), PortDirection::Input);
        assert_eq!(output_port.direction(), PortDirection::Output);
    }
}
