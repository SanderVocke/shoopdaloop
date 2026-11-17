//! MIDI between device threads and the render callback.
//!
//! An input device's callback stamps each message with the host clock and pushes it
//! into a bounded lock-free queue; the render callback places the messages of the last
//! period into its block, one period later, keeping their spacing. Render output goes
//! through another bounded queue to a pump thread that sends it to the devices. Full
//! queues drop messages and count them.

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rt_runtime::buffer::MidiBuffer;
use rt_runtime::Error;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::status::Shared;

/// Longest message a bridge carries; longer ones are refused.
pub const MAX_MESSAGE_BYTES: usize = 64;
/// Messages each bridge holds.
pub const BRIDGE_MESSAGES: usize = 1024;

const PUMP_IDLE: Duration = Duration::from_micros(500);

#[derive(Clone, Copy)]
struct Message {
    /// Nanoseconds since the host's start.
    time: u64,
    len: u8,
    bytes: [u8; MAX_MESSAGE_BYTES],
}

impl Message {
    fn new(time: u64, payload: &[u8]) -> Option<Self> {
        if payload.is_empty() || payload.len() > MAX_MESSAGE_BYTES {
            return None;
        }
        let mut bytes = [0; MAX_MESSAGE_BYTES];
        bytes[..payload.len()].copy_from_slice(payload);
        Some(Self {
            time,
            len: payload.len() as u8,
            bytes,
        })
    }
    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// Sends a device's messages towards the render callback. It does not allocate,
/// lock or wait, so device callbacks may call it.
pub struct MidiInputSender {
    producer: Producer<Message>,
    shared: Arc<Shared>,
}

impl MidiInputSender {
    /// Queue a message that arrived now. Returns false if it was refused or dropped.
    pub fn push(&mut self, message: &[u8]) -> bool {
        self.push_at(Instant::now(), message)
    }

    /// Queue a message that arrived at `at`. Messages should arrive in time order.
    pub fn push_at(&mut self, at: Instant, message: &[u8]) -> bool {
        let counters = &self.shared.counters;
        let Some(message) = Message::new(self.shared.nanos(at), message) else {
            Shared::count(&counters.midi_in_refused, 1);
            return false;
        };
        if self.producer.push(message).is_err() {
            Shared::count(&counters.midi_in_dropped, 1);
            return false;
        }
        true
    }
}

pub(crate) struct MidiInputReceiver {
    consumer: Consumer<Message>,
}

pub(crate) fn input_bridge(shared: &Arc<Shared>) -> (MidiInputSender, MidiInputReceiver) {
    let (producer, consumer) = RingBuffer::new(BRIDGE_MESSAGES);
    (
        MidiInputSender {
            producer,
            shared: shared.clone(),
        },
        MidiInputReceiver { consumer },
    )
}

/// Where a message that arrived at `at` falls in a block of `frames` that starts at
/// `now`: one block later than its arrival, at most the block's start.
pub(crate) fn arrival_offset(at: u64, now: u64, frames: usize, sample_rate: u32) -> u32 {
    let age = now.saturating_sub(at);
    let age_frames = u128::from(age) * u128::from(sample_rate) / 1_000_000_000;
    let last = frames.saturating_sub(1) as u128;
    last.saturating_sub(age_frames) as u32
}

impl MidiInputReceiver {
    /// Move pending messages into `buffer` for a block of `frames` starting at `now`.
    /// Messages that do not fit stay queued for the next block.
    pub fn drain(&mut self, buffer: &mut MidiBuffer, frames: usize, now: u64, shared: &Shared) {
        let mut floor = 0;
        while let Ok(message) = self.consumer.peek() {
            let offset = arrival_offset(message.time, now, frames, shared.sample_rate()).max(floor);
            match buffer.push(offset, frames, message.bytes()) {
                Ok(()) => floor = offset,
                Err(Error::Full) => break,
                Err(_) => Shared::count(&shared.counters.midi_in_refused, 1),
            }
            let _ = self.consumer.pop();
        }
    }
}

pub(crate) struct MidiOutputSender {
    producer: Producer<Message>,
}

pub(crate) struct MidiOutputReceiver {
    consumer: Consumer<Message>,
}

pub(crate) fn output_bridge() -> (MidiOutputSender, MidiOutputReceiver) {
    let (producer, consumer) = RingBuffer::new(BRIDGE_MESSAGES);
    (
        MidiOutputSender { producer },
        MidiOutputReceiver { consumer },
    )
}

impl MidiOutputSender {
    /// Queue a callback's messages for the device in order.
    pub fn send(&mut self, buffer: &MidiBuffer, shared: &Shared) {
        for index in 0..buffer.len() {
            let Some((offset, payload)) = buffer.get(index) else {
                break;
            };
            let queued = Message::new(u64::from(offset), payload)
                .is_some_and(|message| self.producer.push(message).is_ok());
            if !queued {
                Shared::count(&shared.counters.midi_out_dropped, 1);
            }
        }
    }
}

/// A device port that receives messages, used by the output pump thread.
pub trait MidiSink: Send {
    /// Send one message now. Returns false if the device refused it.
    fn send(&mut self, message: &[u8]) -> bool;
}

/// A device port that produces messages.
pub trait MidiSource: Send {
    /// Start delivering messages into `sender`, typically from a device thread. The
    /// returned value keeps the connection open until it is dropped.
    fn connect(self: Box<Self>, sender: MidiInputSender) -> Result<Box<dyn Any + Send>, String>;
}

pub struct MidiInputDevice {
    /// The host port name.
    pub name: String,
    pub source: Box<dyn MidiSource>,
}

pub struct MidiOutputDevice {
    /// The host port name.
    pub name: String,
    pub sink: Box<dyn MidiSink>,
}

/// The MIDI ports a device driver opens next to its audio streams.
#[derive(Default)]
pub struct MidiDevices {
    pub inputs: Vec<MidiInputDevice>,
    pub outputs: Vec<MidiOutputDevice>,
}

/// Sends render output to the devices off the audio thread. Dropping it sends
/// everything queued and ends the thread; do that after callbacks have stopped.
pub(crate) struct Pump {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Pump {
    pub fn spawn(
        mut ports: Vec<(MidiOutputReceiver, Box<dyn MidiSink>)>,
        shared: Arc<Shared>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::Builder::new()
            .name("shoop-rt-midi-out".into())
            .spawn(move || loop {
                let last = stopping.load(Ordering::Acquire);
                let mut sent = false;
                for (receiver, sink) in ports.iter_mut() {
                    while let Ok(message) = receiver.consumer.pop() {
                        sent = true;
                        if !sink.send(message.bytes()) {
                            Shared::count(&shared.counters.midi_out_failed, 1);
                        }
                    }
                }
                if last {
                    break;
                }
                if !sent {
                    std::thread::sleep(PUMP_IDLE);
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Pump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::arrival_offset;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test(no_wasm = "the native host does not build for wasm")]
    fn messages_land_one_block_after_their_arrival() {
        let ms = 1_000_000;
        // 1 kHz, 10-frame blocks starting at 100 ms.
        assert_eq!(arrival_offset(100 * ms, 100 * ms, 10, 1000), 9);
        assert_eq!(arrival_offset(97 * ms, 100 * ms, 10, 1000), 6);
        assert_eq!(arrival_offset(91 * ms, 100 * ms, 10, 1000), 0);
        assert_eq!(arrival_offset(50 * ms, 100 * ms, 10, 1000), 0);
        // A message stamped after the block started also lands at its end.
        assert_eq!(arrival_offset(101 * ms, 100 * ms, 10, 1000), 9);
    }
}
