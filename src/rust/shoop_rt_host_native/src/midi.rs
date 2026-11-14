//! MIDI between device threads and the render callback.
//!
//! An input device's callback stamps each message with the host clock and pushes it
//! into a bounded lock-free queue; the render callback places the messages of the last
//! period into its block, one period later, keeping their spacing. Render output goes
//! through another bounded queue to a pump thread that sends it to the devices.
//! Input loss fences the source until reconnection. Output overload fences the
//! port and uses an acknowledged, reserved reset sequence.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rt_runtime::buffer::MidiBuffer;
use rt_runtime::Error;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::status::Shared;

/// Longest message a bridge carries; longer ones are refused.
pub const MAX_MESSAGE_BYTES: usize = rt_runtime::midi::MAX_MESSAGE_BYTES;
/// Ingress queue capacity; egress is sized from the prepared output profile.
pub const BRIDGE_MESSAGES: usize = 1024;

const PUMP_IDLE: Duration = Duration::from_micros(500);

#[derive(Clone, Copy, Debug)]
struct Message {
    /// Nanoseconds since the host's start.
    time: u64,
    len: u8,
    bytes: [u8; MAX_MESSAGE_BYTES],
}

impl Message {
    fn new(time: u64, payload: &[u8]) -> Option<Self> {
        if rt_runtime::midi::ShortMessage::managed(payload).is_err() {
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
    lost: Arc<AtomicBool>,
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
        if self.lost.load(Ordering::Acquire) || self.producer.push(message).is_err() {
            self.lost.store(true, Ordering::Release);
            Shared::count(&counters.midi_in_dropped, 1);
            return false;
        }
        true
    }
}

pub(crate) struct MidiInputReceiver {
    consumer: Consumer<Message>,
    lost: Arc<AtomicBool>,
    announced: bool,
}

pub(crate) fn input_bridge(shared: &Arc<Shared>) -> (MidiInputSender, MidiInputReceiver) {
    let (producer, consumer) = RingBuffer::new(BRIDGE_MESSAGES);
    let lost = Arc::new(AtomicBool::new(false));
    (
        MidiInputSender {
            producer,
            shared: shared.clone(),
            lost: lost.clone(),
        },
        MidiInputReceiver {
            consumer,
            lost,
            announced: false,
        },
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
        if self.lost.load(Ordering::Acquire) {
            for _ in 0..BRIDGE_MESSAGES {
                if self.consumer.pop().is_err() {
                    break;
                }
            }
            if self.announced {
                buffer.mark_input_loss_silent();
            } else {
                buffer.mark_input_loss();
                self.announced = true;
            }
            return;
        }
        let mut floor = 0;
        while let Ok(message) = self.consumer.peek() {
            let offset = arrival_offset(message.time, now, frames, shared.sample_rate()).max(floor);
            match buffer.push(offset, frames, message.bytes()) {
                Ok(()) => floor = offset,
                Err(Error::Full) => {
                    self.lost.store(true, Ordering::Release);
                    Shared::count(&shared.counters.midi_in_dropped, 1);
                    buffer.mark_input_loss();
                    self.announced = true;
                    break;
                }
                Err(_) => Shared::count(&shared.counters.midi_in_refused, 1),
            }
            let _ = self.consumer.pop();
        }
    }
}

pub(crate) struct MidiOutputSender {
    producer: Producer<Message>,
    fault: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}

pub(crate) struct MidiOutputReceiver {
    consumer: Consumer<Message>,
    fault: Arc<AtomicBool>,
    reset: rt_runtime::midi::ResetState,
    epoch: Arc<AtomicU64>,
    capacity: usize,
}

#[cfg(test)]
pub(crate) fn output_bridge() -> (MidiOutputSender, MidiOutputReceiver) {
    output_bridge_sized(BRIDGE_MESSAGES).expect("test bridge")
}
pub(crate) fn output_bridge_sized(
    capacity: usize,
) -> Result<(MidiOutputSender, MidiOutputReceiver), Error> {
    if capacity == 0
        || capacity
            .checked_mul(size_of::<Message>())
            .is_none_or(|bytes| bytes > i32::MAX as usize)
    {
        return Err(Error::Capacity);
    }
    let (producer, consumer) = RingBuffer::new(capacity);
    let fault = Arc::new(AtomicBool::new(false));
    let epoch = Arc::new(AtomicU64::new(1));
    Ok((
        MidiOutputSender {
            producer,
            fault: fault.clone(),
            epoch: epoch.clone(),
        },
        MidiOutputReceiver {
            consumer,
            fault,
            reset: Default::default(),
            epoch,
            capacity,
        },
    ))
}

impl MidiOutputSender {
    pub fn request_recovery(&self) {
        self.fault.store(true, Ordering::Release);
    }
    /// Queue a callback's messages for the device in order.
    pub fn send(&mut self, buffer: &MidiBuffer, shared: &Shared) {
        if buffer.is_empty() {
            return;
        }
        let epoch = self.epoch.load(Ordering::Acquire);
        if buffer.is_recovery()
            || self.fault.load(Ordering::Acquire)
            || self.producer.slots() < buffer.len()
        {
            Shared::count(&shared.counters.midi_out_dropped, buffer.len() as u64);
            self.fault.store(true, Ordering::Release);
            return;
        }
        for index in 0..buffer.len() {
            let Some((offset, payload)) = buffer.get(index) else {
                break;
            };
            let _ = offset;
            let queued = Message::new(epoch, payload)
                .is_some_and(|message| self.producer.push(message).is_ok());
            if !queued {
                Shared::count(&shared.counters.midi_out_dropped, 1);
            }
        }
    }
}

impl MidiOutputReceiver {
    fn pump(&mut self, sink: &mut dyn MidiSink, shared: &Shared) -> bool {
        let mut sent = false;
        if self.fault.load(Ordering::Acquire) {
            self.reset.request();
            for _ in 0..self.capacity {
                if self.consumer.pop().is_err() {
                    break;
                }
            }
        }
        if self.reset.pending() {
            while let Some(message) = self.reset.peek() {
                if !sink.send(&message) {
                    Shared::count(&shared.counters.midi_out_failed, 1);
                    return sent;
                }
                sent = true;
                self.reset.acknowledge().expect("pending reset");
            }
            if self
                .epoch
                .try_update(Ordering::AcqRel, Ordering::Acquire, |epoch| {
                    epoch.checked_add(1)
                })
                .is_ok()
            {
                self.fault.store(false, Ordering::Release);
            }
            return sent;
        }
        // The producer cannot exceed the fixed bridge quota during one service turn.
        for _ in 0..self.capacity {
            if self.fault.load(Ordering::Acquire) {
                break;
            }
            let Ok(message) = self.consumer.pop() else {
                break;
            };
            if message.time != self.epoch.load(Ordering::Acquire) {
                Shared::count(&shared.counters.midi_out_dropped, 1);
                continue;
            }
            sent = true;
            if !sink.send(message.bytes()) {
                Shared::count(&shared.counters.midi_out_failed, 1);
                self.fault.store(true, Ordering::Release);
                break;
            }
        }
        sent
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
                    sent |= receiver.pump(sink.as_mut(), &shared);
                }
                if last {
                    shared.reset_flushed.store(
                        ports.iter().all(|(receiver, _)| {
                            !receiver.reset.pending()
                                && !receiver.fault.load(Ordering::Acquire)
                                && receiver.consumer.is_empty()
                        }),
                        Ordering::Release,
                    );
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
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn shared() -> Arc<Shared> {
        let description = shoop_rt_engine::EngineDescription {
            quantum: 1,
            max_frames: 8,
            resource_pages: 64,
            host: shoop_rt_engine::HostPorts::numbered(1, 1, 1),
            strip_channels: 1,
            master_channels: 1,
            master_connections: Vec::new(),
            midi_events: 64,
            midi_bytes: 512,
            smoothing_frames: 0,
            smoothing_limit_frames: 8,
            ramp_frames: 4,
            segments: vec![shoop_rt_engine::Segment {
                strips: 1,
                loops: 1,
            }],
            history: Default::default(),
            ..Default::default()
        };
        let driver = shoop_rt_protocol::DriverStatus {
            kind: shoop_rt_protocol::DriverKind::Dummy,
            state: shoop_rt_protocol::DriverRunState::Running,
            sample_rate: 48000,
            buffer_frames: 8,
        };
        let engine = shoop_rt_engine::prepare(&description, driver).unwrap();
        Arc::new(Shared::new(engine.host, driver))
    }

    struct Sink {
        refuses: usize,
        packets: [[u8; 3]; 40],
        count: usize,
    }
    impl MidiSink for Sink {
        fn send(&mut self, bytes: &[u8]) -> bool {
            if self.refuses != 0 {
                self.refuses -= 1;
                return false;
            }
            self.packets[self.count].copy_from_slice(bytes);
            self.count += 1;
            true
        }
    }

    #[shoop_test(no_wasm = "native device bridges")]
    fn prepared_output_density_can_exceed_the_ingress_queue_capacity() {
        let shared = shared();
        let count = BRIDGE_MESSAGES + 276;
        let (mut sender, mut receiver) = output_bridge_sized(count + 32).unwrap();
        let mut batch = MidiBuffer::new(count, count * 3).unwrap();
        for index in 0..count {
            batch.push(0, 1, &[0xb0, 11, (index % 128) as u8]).unwrap();
        }
        struct OrderedSink(usize);
        impl MidiSink for OrderedSink {
            fn send(&mut self, message: &[u8]) -> bool {
                assert_eq!(message, &[0xb0, 11, (self.0 % 128) as u8]);
                self.0 += 1;
                true
            }
        }
        let mut sink = OrderedSink(0);
        shoop_wasm_test_support::heap::assert_no_heap(|| {
            sender.send(&batch, &shared);
            receiver.pump(&mut sink, &shared);
        });
        assert_eq!(sink.0, count);
        assert!(!receiver.fault.load(Ordering::Acquire));
        assert_eq!(shared.counters.midi_out_dropped.load(Ordering::Relaxed), 0);
    }

    #[shoop_test(no_wasm = "native device bridges")]
    fn output_refuses_whole_batches_and_retries_the_same_reset_head_before_new_traffic() {
        let shared = shared();
        let (mut sender, mut receiver) = output_bridge();
        let mut queued = MidiBuffer::new(BRIDGE_MESSAGES, BRIDGE_MESSAGES * 3).unwrap();
        for _ in 0..BRIDGE_MESSAGES - 1 {
            queued.push(0, 1, &[0x90, 60, 100]).unwrap();
        }
        sender.send(&queued, &shared);
        let mut next = MidiBuffer::new(2, 6).unwrap();
        next.push(0, 1, &[0x90, 61, 100]).unwrap();
        next.push(0, 1, &[0x90, 62, 100]).unwrap();
        sender.send(&next, &shared);
        assert_eq!(sender.producer.slots(), 1);
        let mut sink = Sink {
            refuses: 3,
            packets: [[0; 3]; 40],
            count: 0,
        };
        for _ in 0..3 {
            shoop_wasm_test_support::heap::assert_no_heap(|| {
                receiver.pump(&mut sink, &shared);
            });
            assert_eq!(receiver.reset.peek(), Some([0xb0, 120, 0]));
            assert_eq!(sink.count, 0);
        }
        shoop_wasm_test_support::heap::assert_no_heap(|| {
            receiver.pump(&mut sink, &shared);
        });
        assert_eq!(sink.count, 32);
        for (index, packet) in sink.packets[..32].iter().enumerate() {
            assert_eq!(
                *packet,
                [
                    0xb0 | (index / 2) as u8,
                    if index % 2 == 0 { 120 } else { 123 },
                    0
                ]
            );
        }
        // A producer interrupted before the fence may finish enqueueing old work.
        sender
            .producer
            .push(Message::new(1, &[0x90, 63, 100]).unwrap())
            .unwrap();
        next.clear();
        next.push(0, 1, &[0x90, 64, 100]).unwrap();
        sender.send(&next, &shared);
        receiver.pump(&mut sink, &shared);
        assert_eq!(sink.count, 33);
        assert_eq!(sink.packets[32], [0x90, 64, 100]);
    }

    #[shoop_test(no_wasm = "native device bridges")]
    fn input_transport_loss_fences_the_baseline_until_a_new_bridge_is_bound() {
        let shared = shared();
        let (mut sender, mut receiver) = input_bridge(&shared);
        for _ in 0..BRIDGE_MESSAGES {
            assert!(sender.push(&[0xb0, 7, 90]));
        }
        assert!(!sender.push(&[0xb0, 7, 20]));
        let mut input = MidiBuffer::new(2, 6).unwrap();
        for turn in 0..2 {
            shoop_wasm_test_support::heap::assert_no_heap(|| {
                receiver.drain(&mut input, 8, 0, &shared)
            });
            assert!(input.input_lost());
            assert_eq!(input.is_recovery(), turn == 0);
            assert_eq!(input.len(), if turn == 0 { 32 } else { 0 });
            input.clear();
            assert!(!sender.push(&[0x90, 60, 100]));
        }
    }

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
