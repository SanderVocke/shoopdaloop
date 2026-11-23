//! Ordered sample work. This processor sees jobs and storage, never graph nodes.

use alloc::{collections::VecDeque, vec, vec::Vec};

use crate::{
    fixed::{Fixed, Ramp},
    storage::ViewId,
    Error,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ArithmeticMode {
    Multiply,
    LeftBalance,
    RightBalance,
    OneMinus,
}
#[derive(Clone, Copy, Default)]
struct History {
    valid: bool,
    last: i32,
    anchor: i32,
    elapsed: u32,
    duration: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BufferId(
    #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
    #[cfg_attr(feature = "schema", schemars(with = "u32"))]
    pub usize,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Job {
    MidiRestoreState {
        owner: crate::records::Identity,
        sequence: crate::midi::SequenceId,
        position: u64,
    },
    MidiCleanupState {
        owner: crate::records::Identity,
        sequence: crate::midi::SequenceId,
    },
    MidiInput {
        port: u16,
        destination: crate::midi::BufferId,
    },
    MidiEmpty {
        destination: crate::midi::BufferId,
    },
    MidiReadView {
        owner: crate::records::Identity,
        sequence: crate::midi::SequenceId,
        cursor: crate::midi::CursorId,
        position: u64,
        frames: u32,
        restart: bool,
        channels: u16,
        destination: crate::midi::BufferId,
    },
    MidiBeginCapture {
        sequence: crate::midi::SequenceId,
    },
    MidiAppend {
        sequence: crate::midi::SequenceId,
        source: crate::midi::BufferId,
        position: u64,
        frames: u32,
    },
    MidiFinishCapture {
        sequence: crate::midi::SequenceId,
    },
    MidiMerge {
        left: crate::midi::BufferId,
        right: crate::midi::BufferId,
        destination: crate::midi::BufferId,
    },
    MidiOutput {
        source: crate::midi::BufferId,
        port: u16,
    },
    Constant {
        value: i32,
        destination: BufferId,
    },
    Arithmetic {
        left: BufferId,
        right: BufferId,
        destination: BufferId,
        mode: ArithmeticMode,
    },
    Smoothing {
        source: BufferId,
        destination: BufferId,
        history: usize,
        duration: u32,
        playing: bool,
        restart: bool,
    },
    Input {
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
        destination: BufferId,
    },
    Silence {
        destination: BufferId,
    },
    ReadView {
        view: ViewId,
        #[cfg_attr(feature = "serde", serde(rename = "position_chunks"))]
        position: u64,
        #[cfg_attr(feature = "serde", serde(rename = "length_chunks"))]
        length: u64,
        destination: BufferId,
    },
    Gain {
        source: BufferId,
        destination: BufferId,
        ramp: Ramp,
    },
    Mix {
        left: BufferId,
        right: BufferId,
        destination: BufferId,
    },
    BeginCapture {
        view: ViewId,
    },
    Capture {
        source: BufferId,
        view: ViewId,
    },
    Output {
        source: BufferId,
        #[cfg_attr(feature = "serde", serde(with = "crate::serde_index"))]
        #[cfg_attr(feature = "schema", schemars(with = "u32"))]
        port: usize,
    },
}

impl Job {
    pub(crate) fn is_midi(self) -> bool {
        matches!(
            self,
            Self::MidiRestoreState { .. }
                | Self::MidiCleanupState { .. }
                | Self::MidiInput { .. }
                | Self::MidiEmpty { .. }
                | Self::MidiReadView { .. }
                | Self::MidiBeginCapture { .. }
                | Self::MidiAppend { .. }
                | Self::MidiFinishCapture { .. }
                | Self::MidiMerge { .. }
                | Self::MidiOutput { .. }
        )
    }
}

/// A finite FIFO. This functional model emits a whole quantum before draining it.
pub struct JobQueue {
    jobs: VecDeque<Job>,
    capacity: usize,
}

impl JobQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            jobs: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn pop(&mut self) -> Option<Job> {
        self.jobs.pop_front()
    }

    pub fn remaining(&self) -> usize {
        self.capacity - self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Job> {
        self.jobs.iter()
    }

    pub fn push(&mut self, job: Job) -> Result<(), Error> {
        if self.remaining() == 0 {
            return Err(Error::QueueFull);
        }
        self.jobs.push_back(job);
        Ok(())
    }
}

pub(crate) struct JobQueues {
    pub audio: JobQueue,
    pub midi: JobQueue,
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    use crate::{
        midi::{BufferLimits, InputEvent, Io, Message, MidiMemory, OutputEvent, StoreLimits},
        storage::MediaStore,
    };
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn audio_and_midi_queues_can_drain_in_either_order() {
        let message = Message::parse(&[0x90, 60, 80]).unwrap();
        let events = [InputEvent {
            input: 0,
            offset: 3,
            message,
        }];
        for midi_first in [false, true] {
            let mut queues = JobQueues::new(2);
            let mut jobs = JobProcessor::new(4, 1);
            jobs.prepare_midi(BufferLimits {
                buffers: 1,
                cursors: 1,
                scratch_events: 2,
                input_ports: 1,
                output_ports: 1,
            })
            .unwrap();
            for job in [
                Job::Constant {
                    value: 87,
                    destination: BufferId(0),
                },
                Job::MidiInput {
                    port: 0,
                    destination: crate::midi::BufferId(0),
                },
                Job::Output {
                    source: BufferId(0),
                    port: 0,
                },
                Job::MidiOutput {
                    source: crate::midi::BufferId(0),
                    port: 0,
                },
            ] {
                queues.push(job).unwrap();
            }
            let mut audio = MediaStore::new(1).unwrap();
            let mut midi = MidiMemory::prepare(
                StoreLimits {
                    block_events: 4,
                    blocks: 2,
                    max_events: 8,
                    views: 2,
                    retirements: 2,
                },
                1,
            )
            .unwrap();
            let mut captured = Vec::new();
            let mut sink = |event| {
                captured.push(event);
                Ok(())
            };
            let mut io = Io {
                memory: &mut midi,
                inputs: &events,
                sink: &mut sink,
            };
            let mut samples = [Fixed::ZERO; 4];
            if midi_first {
                jobs.run_midi_at(&mut queues.midi, Some(&mut io), 0)
                    .unwrap();
            }
            jobs.run_at(&mut queues.audio, &mut audio, &[], &mut [&mut samples], 0)
                .unwrap();
            if !midi_first {
                jobs.run_midi_at(&mut queues.midi, Some(&mut io), 0)
                    .unwrap();
            }
            assert!(queues.is_empty());
            assert_eq!(samples, [Fixed(87); 4]);
            assert_eq!(
                captured,
                [OutputEvent {
                    destination: 0,
                    offset: 3,
                    message
                }]
            );
        }
    }
}

impl JobQueues {
    pub fn new(capacity: usize) -> Self {
        Self {
            audio: JobQueue::new(capacity),
            midi: JobQueue::new(capacity),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.audio.is_empty() && self.midi.is_empty()
    }

    pub fn push(&mut self, job: Job) -> Result<(), Error> {
        if job.is_midi() {
            self.midi.push(job)
        } else {
            self.audio.push(job)
        }
    }
}

/// Positions and lengths are in chunks; sample streams retain sample units.
pub trait Memory {
    fn prepare_unused_view(&mut self, _view: ViewId) -> Result<bool, Error> {
        Ok(true)
    }
    fn begin_completion_fence(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn completion_fence_ready(&mut self) -> Result<bool, Error> {
        Ok(true)
    }
    fn supports_media(&self) -> bool {
        true
    }
    fn begin_capture(&mut self, _view: ViewId) -> Result<(), Error> {
        Err(Error::InvalidState)
    }
    fn fault(&self) -> Option<Error> {
        None
    }
    fn chunk_frames(&self) -> usize {
        1
    }
    fn read(
        &mut self,
        view: ViewId,
        position: u64,
        length: u64,
        output: &mut [Fixed],
    ) -> Result<(), Error>;
    fn write(&mut self, view: ViewId, samples: &[Fixed]) -> Result<(), Error>;
    fn prefetch(&mut self, _view: ViewId, _position: u64, _length: u64, _chunks: u64) {}
}

mod midi;

pub struct JobProcessor {
    midi: Option<crate::midi::Buffers>,
    quantum: usize,
    history: Vec<History>,
    buffers: Vec<Vec<Fixed>>,
    ready: Vec<bool>,
}

impl JobProcessor {
    pub fn initialize_history(&mut self, slot: usize) -> Result<(), Error> {
        *self.history.get_mut(slot).ok_or(Error::InvalidState)? = History::default();
        Ok(())
    }
    pub fn quantum(&self) -> usize {
        self.quantum
    }
    pub fn new(quantum: usize, buffers: usize) -> Self {
        Self::with_history_slots(quantum, buffers, buffers)
    }
    pub fn with_history_slots(quantum: usize, buffers: usize, history_slots: usize) -> Self {
        Self {
            midi: None,
            quantum,
            history: vec![History::default(); history_slots],
            buffers: vec![vec![Fixed::ZERO; quantum]; buffers],
            ready: vec![false; buffers],
        }
    }

    pub fn run(
        &mut self,
        queue: &mut JobQueue,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), Error> {
        if self.quantum == 0
            || memory.chunk_frames() == 0
            || self.quantum % memory.chunk_frames() != 0
            || inputs.iter().any(|input| input.len() != self.quantum)
            || outputs.iter().any(|output| output.len() != self.quantum)
        {
            return Err(Error::InvalidShape);
        }
        self.run_at(queue, memory, inputs, outputs, 0)
    }
    pub(crate) fn run_at(
        &mut self,
        queue: &mut JobQueue,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        base: usize,
    ) -> Result<(), Error> {
        self.ready.fill(false);
        for output in outputs.iter_mut() {
            output[base..base + self.quantum].fill(Fixed::ZERO);
        }
        while let Some(job) = queue.jobs.pop_front() {
            if let Some(Job::ReadView {
                view,
                position,
                length,
                ..
            }) = queue.jobs.front()
            {
                memory.prefetch(
                    *view,
                    *position,
                    *length,
                    (self.quantum / memory.chunk_frames()) as u64,
                );
            }
            self.execute_at(job, memory, inputs, outputs, base)?;
        }
        Ok(())
    }

    pub(crate) fn run_midi_at(
        &mut self,
        queue: &mut JobQueue,
        mut midi: Option<&mut crate::midi::Io<'_>>,
        base: usize,
    ) -> Result<(), Error> {
        if let Some(buffers) = &mut self.midi {
            buffers.begin();
        }
        while let Some(job) = queue.pop() {
            self.execute_midi(job, midi.as_deref_mut().ok_or(Error::UnsupportedJob)?, base)
                .map_err(Error::Midi)?;
        }
        Ok(())
    }

    fn require(&self, buffer: BufferId) -> Result<(), Error> {
        if self.ready.get(buffer.0).copied() != Some(true) {
            return Err(Error::InvalidBuffer);
        }
        Ok(())
    }

    fn destination(&self, buffer: BufferId) -> Result<(), Error> {
        if self.ready.get(buffer.0).copied() != Some(false) {
            return Err(Error::InvalidBuffer);
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        if let Some(buffers) = &mut self.midi {
            buffers.begin();
            buffers
                .cursors
                .fill(crate::midi::buffers::Cursor::default());
            buffers.work = crate::midi::Work::default();
        }
        self.ready.fill(false);
        self.history.fill(History::default());
    }
    pub fn begin(&mut self) {
        self.ready.fill(false);
    }

    pub fn buffer(&self, id: BufferId) -> Result<&[Fixed], Error> {
        self.require(id)?;
        Ok(&self.buffers[id.0])
    }

    pub fn execute(
        &mut self,
        job: Job,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
    ) -> Result<(), Error> {
        if self.quantum == 0
            || inputs.iter().any(|input| input.len() != self.quantum)
            || outputs.iter().any(|output| output.len() != self.quantum)
        {
            return Err(Error::InvalidShape);
        }
        self.execute_at(job, memory, inputs, outputs, 0)
    }
    fn execute_at(
        &mut self,
        job: Job,
        memory: &mut impl Memory,
        inputs: &[&[Fixed]],
        outputs: &mut [&mut [Fixed]],
        base: usize,
    ) -> Result<(), Error> {
        if matches!(
            job,
            Job::ReadView { .. } | Job::Capture { .. } | Job::BeginCapture { .. }
        ) && !memory.supports_media()
        {
            return Err(Error::UnsupportedJob);
        }
        match job {
            Job::MidiInput { .. }
            | Job::MidiEmpty { .. }
            | Job::MidiReadView { .. }
            | Job::MidiBeginCapture { .. }
            | Job::MidiAppend { .. }
            | Job::MidiFinishCapture { .. }
            | Job::MidiMerge { .. }
            | Job::MidiOutput { .. }
            | Job::MidiRestoreState { .. }
            | Job::MidiCleanupState { .. } => return Err(Error::UnsupportedJob),
            Job::Constant { value, destination } => {
                self.destination(destination)?;
                self.buffers[destination.0].fill(Fixed(value));
                self.ready[destination.0] = true;
            }
            Job::Arithmetic {
                left,
                right,
                destination,
                mode,
            } => {
                self.require(left)?;
                self.require(right)?;
                self.destination(destination)?;
                for offset in 0..self.quantum {
                    let a = self.buffers[left.0][offset];
                    let b = self.buffers[right.0][offset].0;
                    self.buffers[destination.0][offset] = match mode {
                        ArithmeticMode::OneMinus => Fixed(Fixed::ONE.0.wrapping_sub(a.0)),
                        _ => a.multiply(Fixed(match mode {
                            ArithmeticMode::LeftBalance => {
                                if b < 0 {
                                    Fixed::ONE.0
                                } else {
                                    Fixed::ONE.0.wrapping_sub(b)
                                }
                            }
                            ArithmeticMode::RightBalance => {
                                if b < 0 {
                                    Fixed::ONE.0.wrapping_add(b)
                                } else {
                                    Fixed::ONE.0
                                }
                            }
                            _ => b,
                        })),
                    };
                }
                self.ready[destination.0] = true;
            }
            Job::Smoothing {
                source,
                destination,
                history,
                duration,
                playing,
                restart,
            } => {
                self.require(source)?;
                self.destination(destination)?;
                let mut h = *self.history.get(history).ok_or(Error::InvalidBuffer)?;
                if !playing {
                    h = History {
                        valid: true,
                        duration,
                        elapsed: duration,
                        ..History::default()
                    };
                } else if restart {
                    h = History {
                        valid: true,
                        anchor: if h.valid { h.last } else { 0 },
                        duration,
                        ..History::default()
                    };
                } else if !h.valid {
                    h = History {
                        valid: true,
                        ..History::default()
                    };
                }
                for offset in 0..self.quantum {
                    let raw = self.buffers[source.0][offset].0;
                    let value = if !playing {
                        0
                    } else if h.elapsed >= h.duration {
                        raw
                    } else {
                        let weight = (u64::from(h.elapsed) << 24) / u64::from(h.duration);
                        let delta = i64::from(raw) - i64::from(h.anchor);
                        (i64::from(h.anchor) + delta * weight as i64 / i64::from(Fixed::ONE.0))
                            as i32
                    };
                    self.buffers[destination.0][offset] = Fixed(value);
                    h.last = value;
                    if playing {
                        h.elapsed = h.elapsed.saturating_add(1).min(h.duration);
                    }
                }
                self.history[history] = h;
                self.ready[destination.0] = true;
            }
            Job::BeginCapture { view } => memory.begin_capture(view)?,
            Job::Input { port, destination } => {
                self.destination(destination)?;
                self.buffers[destination.0].copy_from_slice(
                    &inputs.get(port).ok_or(Error::InvalidBuffer)?[base..base + self.quantum],
                );
                self.ready[destination.0] = true;
            }
            Job::Silence { destination } => {
                self.destination(destination)?;
                self.buffers[destination.0].fill(Fixed::ZERO);
                self.ready[destination.0] = true;
            }
            Job::ReadView {
                view,
                position,
                length,
                destination,
            } => {
                self.destination(destination)?;
                memory.read(view, position, length, &mut self.buffers[destination.0])?;
                self.ready[destination.0] = true;
            }
            Job::Gain {
                source,
                destination,
                ramp,
            } => {
                self.require(source)?;
                self.destination(destination)?;
                for offset in 0..self.quantum {
                    self.buffers[destination.0][offset] =
                        self.buffers[source.0][offset].multiply(ramp.at(offset as u32));
                }
                self.ready[destination.0] = true;
            }
            Job::Mix {
                left,
                right,
                destination,
            } => {
                self.require(left)?;
                self.require(right)?;
                self.destination(destination)?;
                for offset in 0..self.quantum {
                    self.buffers[destination.0][offset] =
                        self.buffers[left.0][offset].saturating_add(self.buffers[right.0][offset]);
                }
                self.ready[destination.0] = true;
            }
            Job::Capture { source, view } => {
                self.require(source)?;
                memory.write(view, &self.buffers[source.0])?;
            }
            Job::Output { source, port } => {
                self.require(source)?;
                outputs.get_mut(port).ok_or(Error::InvalidBuffer)?[base..base + self.quantum]
                    .copy_from_slice(&self.buffers[source.0]);
            }
        }
        Ok(())
    }
}
