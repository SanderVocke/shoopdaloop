use alloc::{boxed::Box, sync::Arc, vec};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::{
    Clock, Packet, Phase, Sample, Status, CALLBACK, CAPTURE_END, CHUNK_SAMPLES, DEADLINE, FAULT,
    MAX_SAMPLES, PHASE, TRUNCATED,
};

struct Shared {
    generation: AtomicU64,
    requested: AtomicBool,
    active: AtomicBool,
    available: AtomicBool,
    resolution: AtomicU64,
    dropped: AtomicU64,
    skipped: AtomicU64,
    high_water: AtomicU64,
    storage_bytes: u64,
    trigger: AtomicU32,
    host_frame: AtomicU64,
    host_callback_frame: AtomicU64,
    host_open: AtomicBool,
    outer_budget: AtomicBool,
    host_frames: AtomicU32,
    host_start: AtomicU64,
    host_input: AtomicU64,
    host_end: AtomicU64,
    host_generation: AtomicU64,
    ready_frame: AtomicU64,
    ready_callback_frame: AtomicU64,
    ready_frames: AtomicU32,
    ready_budget: AtomicU64,
    ready_start: AtomicU64,
    ready_input: AtomicU64,
    ready_generation: AtomicU64,
}

#[derive(Clone)]
pub struct Control(Arc<Shared>);

impl Control {
    /// Configure during host preparation when complete driver callbacks supply budgets.
    pub fn use_outer_callback_budget(&self) {
        self.0.outer_budget.store(true, Ordering::Relaxed);
    }
    pub fn recording(&self) -> bool {
        self.0.active.load(Ordering::Relaxed)
    }
    pub fn host_begin(&self, frame: u64, frames: u32, at: u64) {
        self.host_begin_external(frame, frame, frames, at);
    }
    pub fn host_begin_external(&self, frame: u64, callback_frame: u64, frames: u32, at: u64) {
        if !self.recording() {
            return;
        }
        self.0
            .host_generation
            .store(self.0.generation.load(Ordering::Acquire), Ordering::Relaxed);
        self.0.host_frame.store(frame, Ordering::Relaxed);
        self.0
            .host_callback_frame
            .store(callback_frame, Ordering::Relaxed);
        self.0.host_frames.store(frames, Ordering::Relaxed);
        self.0.host_start.store(at, Ordering::Relaxed);
        self.0.host_input.store(at, Ordering::Relaxed);
        self.0.host_open.store(true, Ordering::Release);
    }
    pub fn host_input(&self, at: u64) {
        if self.recording() {
            self.0.host_input.store(at, Ordering::Relaxed);
        }
    }
    pub fn host_end(&self, at: u64, budget_ns: u64) {
        if !self.recording()
            || !self.0.host_open.swap(false, Ordering::AcqRel)
            || self.0.host_generation.load(Ordering::Relaxed)
                != self.0.generation.load(Ordering::Acquire)
        {
            return;
        }
        if at.saturating_sub(self.0.host_start.load(Ordering::Relaxed)) > budget_ns
            && budget_ns != 0
        {
            self.trigger(DEADLINE);
        }
        self.0.ready_frames.store(
            self.0.host_frames.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.0.ready_callback_frame.store(
            self.0.host_callback_frame.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.0
            .ready_frame
            .store(self.0.host_frame.load(Ordering::Relaxed), Ordering::Relaxed);
        self.0
            .ready_start
            .store(self.0.host_start.load(Ordering::Relaxed), Ordering::Relaxed);
        self.0
            .ready_input
            .store(self.0.host_input.load(Ordering::Relaxed), Ordering::Relaxed);
        self.0.ready_generation.store(
            self.0.host_generation.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.0.ready_budget.store(budget_ns, Ordering::Relaxed);
        self.0.host_end.store(at, Ordering::Release);
    }

    pub fn set_enabled(&self, enabled: bool) {
        if self.0.requested.swap(enabled, Ordering::AcqRel) != enabled && enabled {
            self.reset();
        }
    }
    pub fn reset(&self) {
        self.0.generation.fetch_add(1, Ordering::AcqRel);
    }
    pub fn trigger(&self, cause: u32) {
        if self.0.active.load(Ordering::Relaxed) {
            self.0.trigger.fetch_or(cause, Ordering::Relaxed);
        }
    }
    pub fn status(&self) -> Status {
        Status {
            generation: self.0.generation.load(Ordering::Acquire),
            requested: self.0.requested.load(Ordering::Relaxed),
            active: self.0.active.load(Ordering::Relaxed),
            available: self.0.available.load(Ordering::Relaxed),
            resolution_ns: self.0.resolution.load(Ordering::Relaxed),
            dropped: self.0.dropped.load(Ordering::Relaxed),
            skipped_captures: self.0.skipped.load(Ordering::Relaxed),
            high_water: self.0.high_water.load(Ordering::Relaxed),
            storage_bytes: self.0.storage_bytes,
        }
    }
}

struct Trace {
    header: Packet,
    samples: Box<[Sample]>,
    len: usize,
}
impl Trace {
    fn new(capacity: usize) -> Self {
        Self {
            header: Packet::default(),
            samples: vec![Sample::default(); capacity].into_boxed_slice(),
            len: 0,
        }
    }
    fn begin(&mut self, header: Packet) {
        self.header = header;
        self.len = 0;
    }
    fn push(&mut self, key: u64, at: u64) {
        if let Some(slot) = self.samples.get_mut(self.len) {
            *slot = Sample { key, at };
            self.len += 1;
        } else {
            self.header.flags |= TRUNCATED;
        }
    }
    fn copy_from(&mut self, source: &Self) {
        self.header = source.header;
        self.len = source.len;
        self.samples[..self.len].copy_from_slice(&source.samples[..self.len]);
    }
    fn packet(&self, offset: usize) -> Packet {
        let mut packet = self.header;
        let count = (self.len - offset).min(CHUNK_SAMPLES);
        packet.index = offset as u32;
        packet.total = self.len as u32;
        packet.len = count as u8;
        packet.samples[..count].copy_from_slice(&self.samples[offset..offset + count]);
        packet
    }
}

struct Bank {
    traces: Box<[Trace]>,
    next: usize,
    count: usize,
    capture: u64,
    trigger: u32,
    flags: u32,
}
impl Bank {
    fn new(capacity: usize, traces: usize) -> Self {
        Self {
            traces: (0..traces).map(|_| Trace::new(capacity)).collect(),
            next: 0,
            count: 0,
            capture: 0,
            trigger: 0,
            flags: 0,
        }
    }
    fn reset(&mut self) {
        self.next = 0;
        self.count = 0;
        self.capture = 0;
        self.trigger = 0;
        self.flags = 0;
    }
    fn completed(&mut self) {
        self.next = (self.next + 1) % self.traces.len();
        self.count = (self.count + 1).min(self.traces.len());
    }
    fn at(&self, index: usize) -> &Trace {
        &self.traces[(self.next + self.traces.len() - self.count + index) % self.traces.len()]
    }
}

struct Storage {
    control: Control,
    history: Bank,
    spare_bank: Option<Bank>,
    callback: Trace,
    stream: Producer<Trace>,
    free: Consumer<Trace>,
    spare_trace: Option<Trace>,
    captures: Producer<Bank>,
    returned: Consumer<Bank>,
    open: bool,
    post_callbacks: usize,
    trigger: u32,
    capture_sequence: u64,
    high_water: usize,
    stream_capacity: usize,
}

impl Storage {
    fn completed(&mut self) {
        self.history.completed();
        if self.post_callbacks != 0 && self.history.count == self.history.traces.len() {
            self.freeze(true);
        }
    }
    fn freeze(&mut self, incomplete: bool) {
        if let Some(mut bank) = self.spare_bank.take() {
            bank.reset();
            core::mem::swap(&mut bank, &mut self.history);
            self.capture_sequence = self.capture_sequence.wrapping_add(1);
            bank.capture = self.capture_sequence;
            bank.trigger = self.trigger;
            if incomplete {
                bank.flags |= TRUNCATED;
            }
            if let Err(PushError::Full(bank)) = self.captures.push(bank) {
                self.spare_bank = Some(bank);
                self.control.0.skipped.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.control.0.skipped.fetch_add(1, Ordering::Relaxed);
        }
        self.trigger = 0;
        self.post_callbacks = 0;
    }
}

pub struct Recorder {
    storage: Option<Storage>,
    clock: Option<Clock>,
    enabled: bool,
    layout_pending: bool,
    generation: u64,
    epoch: u64,
    callback: u64,
    sequence: u64,
    callback_frame: u64,
    callback_frames: u32,
    tick_frames: u32,
    callback_at: u64,
    budget: u64,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            storage: None,
            clock: None,
            enabled: false,
            layout_pending: false,
            generation: 0,
            epoch: 0,
            callback: 0,
            sequence: 0,
            callback_frame: 0,
            callback_frames: 0,
            tick_frames: 0,
            callback_at: 0,
            budget: 0,
        }
    }
}

pub struct Reader {
    control: Control,
    stream: Consumer<Trace>,
    free: Producer<Trace>,
    current: Option<Trace>,
    offset: usize,
    captures: Consumer<Bank>,
    returned: Producer<Bank>,
    capture: Option<Bank>,
    capture_index: usize,
    capture_offset: usize,
    alternate: bool,
}

pub fn prepare(max_nodes: usize) -> Option<(Control, Recorder, Reader)> {
    let capacity = max_nodes.checked_mul(2)?.checked_add(24)?;
    if capacity > MAX_SAMPLES {
        return None;
    }
    let pages =
        (2 * 1024 * 1024 / (capacity * core::mem::size_of::<Sample>() + 512)).clamp(2, 4096);
    let stream_capacity =
        (8 * 1024 * 1024 / (capacity * core::mem::size_of::<Sample>() + 512)).clamp(64, 1024);
    let control = Control(Arc::new(Shared {
        generation: AtomicU64::new(1),
        requested: AtomicBool::new(false),
        active: AtomicBool::new(false),
        available: AtomicBool::new(false),
        resolution: AtomicU64::new(0),
        dropped: AtomicU64::new(0),
        skipped: AtomicU64::new(0),
        high_water: AtomicU64::new(0),
        storage_bytes: ((2 * pages + stream_capacity) * capacity * core::mem::size_of::<Sample>()
            + 24 * core::mem::size_of::<Sample>()) as u64,
        trigger: AtomicU32::new(0),
        host_frame: AtomicU64::new(0),
        host_callback_frame: AtomicU64::new(0),
        host_open: AtomicBool::new(false),
        outer_budget: AtomicBool::new(false),
        host_frames: AtomicU32::new(0),
        host_start: AtomicU64::new(0),
        host_input: AtomicU64::new(0),
        host_end: AtomicU64::new(0),
        host_generation: AtomicU64::new(0),
        ready_frame: AtomicU64::new(0),
        ready_callback_frame: AtomicU64::new(0),
        ready_frames: AtomicU32::new(0),
        ready_budget: AtomicU64::new(0),
        ready_start: AtomicU64::new(0),
        ready_input: AtomicU64::new(0),
        ready_generation: AtomicU64::new(0),
    }));
    let (stream, receive) = RingBuffer::new(stream_capacity);
    let (mut free, spare) = RingBuffer::new(stream_capacity);
    for _ in 0..stream_capacity {
        let _ = free.push(Trace::new(capacity));
    }
    let (captures, receive_capture) = RingBuffer::new(1);
    let (returned, receive_returned) = RingBuffer::new(1);
    let recorder = Recorder {
        storage: Some(Storage {
            control: control.clone(),
            history: Bank::new(capacity, pages),
            spare_bank: Some(Bank::new(capacity, pages)),
            callback: Trace::new(24),
            stream,
            free: spare,
            spare_trace: None,
            captures,
            returned: receive_returned,
            open: false,
            post_callbacks: 0,
            trigger: 0,
            capture_sequence: 0,
            high_water: 0,
            stream_capacity,
        }),
        ..Recorder::default()
    };
    let reader = Reader {
        control: control.clone(),
        stream: receive,
        free,
        current: None,
        offset: 0,
        captures: receive_capture,
        returned,
        capture: None,
        capture_index: 0,
        capture_offset: 0,
        alternate: false,
    };
    Some((control, recorder, reader))
}

impl Recorder {
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = Some(clock);
        if let Some(storage) = &self.storage {
            storage
                .control
                .0
                .resolution
                .store(clock.resolution_ns, Ordering::Relaxed);
            storage.control.0.available.store(true, Ordering::Release);
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    fn now(&self) -> u64 {
        self.clock.map_or(0, |clock| (clock.now)())
    }
    fn header(&mut self, frame: u64, at: u64, flags: u32) -> Packet {
        self.sequence = self.sequence.wrapping_add(1);
        Packet {
            generation: self.generation,
            budget_ns: if flags & CALLBACK != 0 {
                self.budget
            } else {
                0
            },
            sequence: self.sequence,
            callback: self.callback,
            callback_frame: self.callback_frame,
            frame,
            epoch: self.epoch,
            start_ns: at,
            end_ns: at,
            flags,
            ..Packet::default()
        }
    }
    fn reset(&mut self, generation: u64) {
        self.generation = generation;
        self.layout_pending = true;
        if let Some(storage) = &mut self.storage {
            storage.history.reset();
            storage.open = false;
            storage.post_callbacks = 0;
            storage.trigger = 0;
            storage.control.0.dropped.store(0, Ordering::Relaxed);
            storage.control.0.skipped.store(0, Ordering::Relaxed);
            storage.control.0.high_water.store(0, Ordering::Relaxed);
            storage.high_water = 0;
            storage.control.0.trigger.store(0, Ordering::Relaxed);
        }
    }
    pub fn begin_callback(&mut self, frame: u64, epoch: u64, frames: u32, budget_ns: u64) {
        let Some(storage) = &self.storage else {
            return;
        };
        let was_enabled = self.enabled;
        let enabled = storage.control.0.requested.load(Ordering::Relaxed) && self.clock.is_some();
        if enabled != self.enabled {
            self.enabled = enabled;
            storage.control.0.active.store(enabled, Ordering::Relaxed);
        }
        let generation = storage.control.0.generation.load(Ordering::Acquire);
        if generation != self.generation {
            self.reset(generation);
        }
        if !self.enabled {
            if was_enabled {
                if let Some(storage) = &mut self.storage {
                    storage.trigger |= storage.control.0.trigger.swap(0, Ordering::Relaxed);
                    if storage.post_callbacks != 0 || storage.trigger != 0 {
                        storage.freeze(true);
                    }
                }
            }
            return;
        }
        if let Some(storage) = &mut self.storage {
            let shared = &storage.control.0;
            let end = shared.host_end.swap(0, Ordering::Acquire);
            let start = shared.ready_start.load(Ordering::Relaxed);
            let input = shared.ready_input.load(Ordering::Relaxed);
            let frame = shared.ready_frame.load(Ordering::Relaxed);
            if end >= input
                && input >= start
                && end != 0
                && shared.ready_generation.load(Ordering::Relaxed) == self.generation
            {
                self.sequence = self.sequence.wrapping_add(1);
                storage.callback.begin(Packet {
                    generation: self.generation,
                    sequence: self.sequence,
                    callback: self.callback,
                    callback_frame: shared.ready_callback_frame.load(Ordering::Relaxed),
                    frame,
                    frames: shared.ready_frames.load(Ordering::Relaxed),
                    epoch: self.epoch,
                    start_ns: start,
                    end_ns: end,
                    budget_ns: shared.ready_budget.load(Ordering::Relaxed),
                    flags: CALLBACK | crate::HOST,
                    ..Packet::default()
                });
                storage
                    .callback
                    .push(PHASE | Phase::CallbackStart as u64, start);
                storage
                    .callback
                    .push(PHASE | Phase::HostInput as u64, input);
                storage.callback.push(PHASE | Phase::HostOutput as u64, end);
                Self::stream(storage, true);
                storage.history.traces[storage.history.next].copy_from(&storage.callback);
                storage.completed();
            }
        }
        self.callback = self.callback.wrapping_add(1);
        self.callback_frame = frame;
        self.callback_frames = frames;
        self.epoch = epoch;
        self.budget = if self
            .storage
            .as_ref()
            .is_some_and(|storage| storage.control.0.outer_budget.load(Ordering::Relaxed))
        {
            0
        } else {
            budget_ns
        };
        self.callback_at = self.now();
        let mut header = self.header(frame, self.callback_at, CALLBACK);
        header.frames = frames;
        if let Some(storage) = &mut self.storage {
            storage.callback.begin(header);
            storage
                .callback
                .push(PHASE | Phase::CallbackStart as u64, self.callback_at);
            if storage.spare_bank.is_none() {
                storage.spare_bank = storage.returned.pop().ok();
            }
        }
    }
    pub fn callback_phase(&mut self, phase: Phase) {
        if !self.enabled {
            return;
        }
        let at = self.now();
        if let Some(storage) = &mut self.storage {
            storage.callback.push(PHASE | phase as u64, at);
        }
    }
    pub fn begin_tick(&mut self, frame: u64, frames: u32) {
        if !self.enabled {
            return;
        }
        let at = self.now();
        self.tick_frames = frames;
        let mut header = self.header(frame, at, 0);
        header.frames = frames;
        if let Some(storage) = &mut self.storage {
            storage.history.traces[storage.history.next].begin(header);
            storage.open = true;
        }
        self.phase(Phase::TickStart);
    }
    pub fn graph_start(&mut self, epoch: u64, frame: u64) {
        if !self.enabled {
            return;
        }
        if self.epoch != epoch {
            if let Some(storage) = &self.storage {
                storage.control.reset();
            }
            let generation = self
                .storage
                .as_ref()
                .map_or(self.generation, |s| s.control.status().generation);
            self.reset(generation);
            self.epoch = epoch;
            let at = self.now();
            let mut header = self.header(self.callback_frame, at, CALLBACK);
            header.frames = self.callback_frames;
            if let Some(storage) = &mut self.storage {
                storage.callback.begin(header);
            }
            self.begin_tick(frame, self.tick_frames);
        }
        self.phase(Phase::GraphStart);
    }
    pub fn needs_layout(&self) -> bool {
        self.enabled && self.layout_pending
    }
    pub fn layout_node(&mut self, node: u64) {
        if let Some(storage) = &mut self.storage {
            let trace = &mut storage.history.traces[storage.history.next];
            let at = trace
                .samples
                .get(trace.len.saturating_sub(1))
                .map_or(trace.header.start_ns, |s| s.at);
            trace.push(crate::LAYOUT | node, at);
        }
    }
    pub fn layout_done(&mut self) {
        self.layout_pending = false;
    }
    pub fn phase(&mut self, phase: Phase) {
        self.point(PHASE | phase as u64);
    }
    #[inline]
    pub fn node_ready(&mut self, node: u64) {
        self.point(node);
    }
    #[inline]
    fn point(&mut self, key: u64) {
        if !self.enabled {
            return;
        }
        let at = self.now();
        if let Some(storage) = &mut self.storage {
            if storage.open {
                storage.history.traces[storage.history.next].push(key, at);
            }
        }
    }
    fn stream(storage: &mut Storage, callback: bool) {
        let page = storage
            .spare_trace
            .take()
            .or_else(|| storage.free.pop().ok());
        let Some(mut page) = page else {
            storage.control.0.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        page.copy_from(if callback {
            &storage.callback
        } else {
            &storage.history.traces[storage.history.next]
        });
        if let Err(PushError::Full(page)) = storage.stream.push(page) {
            storage.spare_trace = Some(page);
            storage.control.0.dropped.fetch_add(1, Ordering::Relaxed);
        }
        let occupied = storage.stream_capacity - storage.stream.slots();
        if occupied > storage.high_water {
            storage.high_water = occupied;
            storage
                .control
                .0
                .high_water
                .store(occupied as u64, Ordering::Relaxed);
        }
    }
    pub fn end_tick(&mut self, complete: bool) {
        if !self.enabled {
            return;
        }
        let at = self.now();
        if let Some(storage) = &mut self.storage {
            if !storage.open {
                return;
            }
            let trace = &mut storage.history.traces[storage.history.next];
            trace.header.end_ns = at;
            if !complete {
                trace.header.flags |= FAULT;
            }
            Self::stream(storage, false);
            storage.completed();
            storage.open = false;
        }
    }
    pub fn end_callback(&mut self, complete: bool) {
        if !self.enabled {
            return;
        }
        self.end_tick(false);
        let at = self.now();
        if let Some(storage) = &mut self.storage {
            storage.callback.push(PHASE | Phase::CallbackEnd as u64, at);
            storage.callback.header.end_ns = at;
            if !complete {
                storage.callback.header.flags |= FAULT;
            }
            Self::stream(storage, true);
            storage.history.traces[storage.history.next].copy_from(&storage.callback);
            storage.completed();
            let mut cause = storage.control.0.trigger.swap(0, Ordering::Relaxed);
            if self.budget != 0 && at.saturating_sub(self.callback_at) > self.budget {
                cause |= DEADLINE;
            }
            if cause != 0 && storage.post_callbacks == 0 {
                storage.trigger = cause;
                storage.post_callbacks = 3;
                let pre = (storage.history.traces.len() * 3 / 4).max(1);
                if storage.history.count > pre {
                    storage.history.count = pre;
                    storage.history.flags |= TRUNCATED;
                }
            } else {
                storage.trigger |= cause;
            }
            if storage.post_callbacks != 0 {
                if !complete {
                    storage.post_callbacks = 1;
                }
                storage.post_callbacks -= 1;
                if storage.post_callbacks == 0 {
                    storage.freeze(!complete);
                }
            }
        }
    }
}

impl Reader {
    pub fn control(&self) -> &Control {
        &self.control
    }
    pub fn next_packet(&mut self) -> Option<Packet> {
        self.alternate = !self.alternate;
        if self.alternate {
            self.capture_packet().or_else(|| self.live_packet())
        } else {
            self.live_packet().or_else(|| self.capture_packet())
        }
    }
    fn live_packet(&mut self) -> Option<Packet> {
        for _ in 0..64 {
            if self.current.is_none() {
                self.current = self.stream.pop().ok();
                self.offset = 0;
            }
            let trace = self.current.as_ref()?;
            let valid = trace.header.generation == self.control.status().generation;
            let packet = valid.then(|| trace.packet(self.offset));
            if let Some(packet) = packet {
                self.offset += usize::from(packet.len);
            }
            if !valid || self.offset == trace.len {
                if let Some(trace) = self.current.take() {
                    if let Err(PushError::Full(trace)) = self.free.push(trace) {
                        self.current = Some(trace);
                    }
                }
            }
            if packet.is_some() {
                return packet;
            }
        }
        None
    }
    fn capture_packet(&mut self) -> Option<Packet> {
        if self.capture.is_none() {
            self.capture = self.captures.pop().ok();
            self.capture_index = 0;
            self.capture_offset = 0;
        }
        let bank = self.capture.as_ref()?;
        let valid =
            bank.count != 0 && bank.at(0).header.generation == self.control.status().generation;
        let packet = if valid && self.capture_index < bank.count {
            let trace = bank.at(self.capture_index);
            let mut packet = trace.packet(self.capture_offset);
            packet.capture = bank.capture;
            packet.trigger = bank.trigger;
            packet.flags |= bank.flags;
            if bank.count == bank.traces.len() {
                packet.flags |= TRUNCATED;
            }
            self.capture_offset += usize::from(packet.len);
            if self.capture_offset == trace.len {
                self.capture_offset = 0;
                self.capture_index += 1;
                if self.capture_index == bank.count {
                    packet.flags |= CAPTURE_END;
                }
            }
            Some(packet)
        } else {
            None
        };
        if !valid || self.capture_index == bank.count {
            if let Some(bank) = self.capture.take() {
                if let Err(PushError::Full(bank)) = self.returned.push(bank) {
                    self.capture = Some(bank);
                }
            }
        }
        packet
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{
        heap::{assert_no_heap, TrackingAllocator},
        shoop_test,
    };
    #[global_allocator]
    static ALLOCATOR: TrackingAllocator = TrackingAllocator;
    std::thread_local! { static TIME: core::cell::Cell<u64> = const { core::cell::Cell::new(0) }; }
    fn now() -> u64 {
        TIME.with(|time| {
            let at = time.get();
            time.set(at + 10);
            at
        })
    }
    fn render(recorder: &mut Recorder, frame: u64) {
        recorder.begin_callback(frame, 0, 1, 1_000_000);
        recorder.begin_tick(frame, 1);
        recorder.graph_start(0, frame);
        recorder.node_ready(3);
        recorder.end_tick(true);
        recorder.end_callback(true);
    }

    #[shoop_test]
    fn disabled_and_saturated_recording_do_not_touch_the_heap() {
        let (control, mut recorder, mut reader) = prepare(4).unwrap();
        recorder.set_clock(Clock {
            now,
            resolution_ns: 10,
        });
        let before = TIME.with(core::cell::Cell::get);
        assert_no_heap(|| {
            for i in 0..100 {
                render(&mut recorder, i);
            }
        });
        assert_eq!(TIME.with(core::cell::Cell::get), before);
        assert!(reader.next_packet().is_none());
        control.set_enabled(true);
        let capacity = recorder.storage.as_ref().unwrap().stream_capacity;
        let base = capacity as u64 + 10;
        assert_no_heap(|| {
            for i in 0..base {
                render(&mut recorder, i);
            }
        });
        assert!(control.status().dropped > 0);
        assert_eq!(control.status().high_water, capacity as u64);
        control.trigger(crate::HOST_XRUN);
        assert_no_heap(|| {
            for i in base..base + 4 {
                render(&mut recorder, i);
            }
        });
        let first = (0..4)
            .find_map(|_| {
                assert_no_heap(|| reader.next_packet()).filter(|packet| packet.capture != 0)
            })
            .expect("capture becomes available");
        assert_ne!(first.capture, 0);
        control.trigger(crate::HOST_XRUN);
        assert_no_heap(|| {
            for i in base + 4..base + 8 {
                render(&mut recorder, i);
            }
        });
        assert!(control.status().skipped_captures > 0);
        let mut captured = true;
        let mut ended = false;
        for _ in 0..8192 {
            if let Some(packet) = assert_no_heap(|| reader.next_packet()) {
                assert!(packet.valid());
                captured |= packet.capture != 0;
                ended |= packet.flags & CAPTURE_END != 0;
            }
        }
        assert!(captured && ended);
        control.reset();
        assert_no_heap(|| render(&mut recorder, base + 9));
        let generation = control.status().generation;
        for _ in 0..128 {
            if let Some(packet) = reader.next_packet() {
                assert_eq!(packet.generation, generation);
            }
        }
        control.set_enabled(false);
        let before = TIME.with(core::cell::Cell::get);
        assert_no_heap(|| render(&mut recorder, base + 10));
        assert_eq!(TIME.with(core::cell::Cell::get), before);
        assert!(!control.status().active);
    }

    #[shoop_test]
    fn a_complete_small_quantum_callback_fits_before_the_consumer_runs() {
        let (control, mut recorder, mut reader) = prepare(4).unwrap();
        recorder.set_clock(Clock {
            now,
            resolution_ns: 10,
        });
        control.set_enabled(true);
        assert_no_heap(|| {
            recorder.begin_callback(0, 0, 256, 1_000_000);
            for frame in 0..256 {
                recorder.begin_tick(frame, 1);
                recorder.graph_start(0, frame);
                recorder.node_ready(3);
                recorder.end_tick(true);
            }
            recorder.end_callback(true);
        });
        assert_eq!(control.status().dropped, 0);
        let mut collector = crate::Collector::default();
        collector.status(control.status());
        for _ in 0..1024 {
            if let Some(packet) = reader.next_packet() {
                collector.receive(packet);
            }
        }
        assert_eq!(collector.view().live.rows[0].samples, 256);
    }

    #[shoop_test]
    fn a_short_history_preserves_the_trigger_when_post_work_fills_capacity() {
        let (control, mut recorder, mut reader) = prepare(4).unwrap();
        let storage = recorder.storage.as_mut().unwrap();
        storage.history = Bank::new(32, 4);
        storage.spare_bank = Some(Bank::new(32, 4));
        recorder.set_clock(Clock {
            now,
            resolution_ns: 10,
        });
        control.set_enabled(true);
        for frame in 0..100 {
            render(&mut recorder, frame);
        }
        control.trigger(crate::HOST_XRUN);
        assert_no_heap(|| {
            render(&mut recorder, 100);
            render(&mut recorder, 101);
        });
        let mut trigger_retained = false;
        let mut complete = false;
        for _ in 0..256 {
            if let Some(packet) = reader.next_packet() {
                if packet.capture != 0 {
                    trigger_retained |= packet.frame == 100;
                    complete |= packet.flags & CAPTURE_END != 0;
                    assert_ne!(packet.flags & TRUNCATED, 0);
                }
            }
        }
        assert!(trigger_retained && complete);
    }

    #[shoop_test]
    fn disabling_closes_a_pending_capture_without_reading_the_clock() {
        let (control, mut recorder, mut reader) = prepare(4).unwrap();
        recorder.set_clock(Clock {
            now,
            resolution_ns: 10,
        });
        control.set_enabled(true);
        render(&mut recorder, 0);
        control.trigger(crate::HOST_XRUN);
        control.set_enabled(false);
        let before = TIME.with(core::cell::Cell::get);
        assert_no_heap(|| render(&mut recorder, 1));
        assert_eq!(TIME.with(core::cell::Cell::get), before);
        let mut complete = false;
        for _ in 0..32 {
            if let Some(packet) = reader.next_packet() {
                if packet.capture != 0 {
                    complete |= packet.flags & CAPTURE_END != 0;
                    assert_ne!(packet.flags & TRUNCATED, 0);
                }
            }
        }
        assert!(complete);
    }

    #[shoop_test]
    fn split_engine_parts_do_not_create_independent_driver_deadlines() {
        let (control, mut recorder, _reader) = prepare(4).unwrap();
        recorder.set_clock(Clock {
            now,
            resolution_ns: 10,
        });
        control.use_outer_callback_budget();
        control.set_enabled(true);
        render(&mut recorder, 0);
        control.host_begin(1, 2, now());
        for frame in [1, 2] {
            recorder.begin_callback(frame, 0, 1, 1);
            recorder.begin_tick(frame, 1);
            recorder.graph_start(0, frame);
            recorder.node_ready(3);
            recorder.end_tick(true);
            recorder.end_callback(true);
        }
        control.host_end(now(), 10_000);
        assert_eq!(control.0.trigger.load(Ordering::Relaxed), 0);
        control.host_begin(3, 1, now());
        control.host_end(now() + 100, 10);
        assert_eq!(control.0.trigger.load(Ordering::Relaxed), DEADLINE);
    }

    #[shoop_test]
    fn host_frames_remain_separate_and_an_unmatched_end_is_ignored() {
        let (control, mut recorder, mut reader) = prepare(4).unwrap();
        recorder.set_clock(Clock {
            now,
            resolution_ns: 10,
        });
        control.set_enabled(true);
        render(&mut recorder, 0);
        control.host_end(now(), 1000);
        assert_eq!(control.0.trigger.load(Ordering::Relaxed), 0);
        control.host_begin_external(1, 5001, 1, now());
        control.host_input(now());
        control.host_end(now(), 1000);
        render(&mut recorder, 2);
        let mut found = false;
        let mut collector = crate::Collector::default();
        collector.status(control.status());
        for _ in 0..32 {
            if let Some(packet) = reader.next_packet() {
                if packet.flags & crate::HOST != 0 {
                    found = true;
                    assert_eq!(packet.frame, 1);
                    assert_eq!(packet.callback_frame, 5001);
                }
                collector.receive(packet);
            }
        }
        assert!(found);
        assert_eq!(collector.view().live.interval.end_frame, 3);
    }
}
