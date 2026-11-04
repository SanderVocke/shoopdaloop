use crate::{
    inline::{Completion, Job},
    InlineTestProcessor, PreparedTiming, Timing,
};
use alloc::{boxed::Box, vec, vec::Vec};
use rt_runtime::{
    buffer::{AudioPorts, Event, MidiBuffer},
    graph::PortCounts,
    midi::ResetState,
    planner::Planner,
    processor::{ProcessBlock, ProcessorOperation},
    Error, Frame, Quantum,
};

pub const MAX_STORAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiLimits {
    pub events_per_quantum: usize,
    pub bytes_per_quantum: usize,
}

impl Default for MidiLimits {
    fn default() -> Self {
        Self {
            events_per_quantum: 64,
            bytes_per_quantum: 4096,
        }
    }
}

struct Slot {
    audio: Box<[f32]>,
    input: MidiBuffer,
    output: MidiBuffer,
    completion: Option<Completion>,
    played_events: usize,
}

/// A render-owned node whose completed batches retain a fixed stream position.
pub struct BufferedProcessor<B = InlineTestProcessor> {
    backend: B,
    timing: PreparedTiming,
    limits: MidiLimits,
    slots: Box<[Slot]>,
    write: usize,
    read: usize,
    filled: usize,
    next: Frame,
    batch_start: Frame,
    generation: u64,
    sequence: u64,
}

impl BufferedProcessor<InlineTestProcessor> {
    pub fn prepare(quantum: Quantum, timing: Timing, limits: MidiLimits) -> Result<Self, Error> {
        let timing = timing.prepare(quantum)?;
        let m = timing.timing().batch_quanta;
        let events = limits
            .events_per_quantum
            .checked_mul(m)
            .ok_or(Error::Capacity)?;
        let bytes = limits
            .bytes_per_quantum
            .checked_mul(m)
            .ok_or(Error::Capacity)?;
        let reserved_events = events
            .checked_add(ResetState::EVENTS)
            .ok_or(Error::Capacity)?;
        let reserved_bytes = bytes
            .checked_add(ResetState::BYTES)
            .ok_or(Error::Capacity)?;
        let midi_storage = reserved_events
            .checked_mul(size_of::<Event>())
            .and_then(|v| v.checked_add(reserved_bytes))
            .ok_or(Error::Capacity)?;
        let samples = timing
            .batch_frames()
            .checked_mul(2)
            .ok_or(Error::Capacity)?;
        let storage = samples
            .checked_mul(size_of::<f32>())
            .and_then(|v| midi_storage.checked_mul(2).and_then(|m| v.checked_add(m)))
            .and_then(|v| v.checked_add(size_of::<Slot>()))
            .and_then(|v| v.checked_mul(timing.slots()))
            .and_then(|v| v.checked_add(limits.bytes_per_quantum))
            .ok_or(Error::Capacity)?;
        if storage > MAX_STORAGE_BYTES {
            return Err(Error::Capacity);
        }
        let mut slots = Vec::with_capacity(timing.slots());
        for _ in 0..timing.slots() {
            let mut audio = vec![0.; samples].into_boxed_slice();
            core::hint::black_box(&mut *audio).fill(0.);
            slots.push(Slot {
                audio,
                input: MidiBuffer::new(events, bytes)?,
                output: MidiBuffer::new(events, bytes)?,
                completion: None,
                played_events: 0,
            });
        }
        Ok(Self {
            backend: InlineTestProcessor::new(limits.bytes_per_quantum),
            timing,
            limits,
            slots: slots.into_boxed_slice(),
            write: 0,
            read: 0,
            filled: 0,
            next: Frame(0),
            batch_start: Frame(0),
            generation: 1,
            sequence: 0,
        })
    }

    pub fn timing(&self) -> PreparedTiming {
        self.timing
    }
    pub fn latency_frames(&self) -> usize {
        self.timing.latency_frames()
    }
    pub fn completed_batches(&self) -> u64 {
        self.sequence
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn next_frame(&self) -> Frame {
        self.next
    }
    pub fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }

    /// Reset only after callbacks stop and downstream MIDI recovery is settled.
    pub fn reset(&mut self, origin: Frame) -> Result<(), Error> {
        if !self.timing.quantum().aligned(origin) {
            return Err(Error::InvalidFrame);
        }
        origin.checked_add(self.latency_frames() + self.timing.quantum().frames())?;
        let generation = self.generation.checked_add(1).ok_or(Error::Exhausted)?;
        for slot in &mut self.slots {
            slot.audio.fill(0.);
            slot.input.clear();
            slot.output.clear();
            slot.completion = None;
            slot.played_events = 0;
        }
        self.write = 0;
        self.read = 0;
        self.filled = 0;
        self.next = origin;
        self.batch_start = origin;
        self.generation = generation;
        self.sequence = 0;
        Ok(())
    }

    pub fn process_tick(
        &mut self,
        frame: Frame,
        quantum: Quantum,
        audio: &mut AudioPorts<'_>,
        input: &MidiBuffer,
        output: &mut MidiBuffer,
    ) -> Result<(), Error> {
        let q = self.timing.quantum().frames();
        let b = self.timing.batch_frames();
        if quantum != self.timing.quantum() || frame != self.next {
            return Err(Error::InvalidFrame);
        }
        if audio.input_count() != 2
            || audio.output_count() != 2
            || (0..2).any(|c| audio.input(c).is_none_or(|v| v.len() != q))
        {
            return Err(Error::InvalidShape);
        }
        for c in 0..2 {
            if audio.output(c).is_none_or(|v| v.len() != q) {
                return Err(Error::InvalidShape);
            }
        }
        if !output.is_empty() || output.is_recovery() {
            return Err(Error::InvalidState);
        }
        let next = frame.checked_add(q)?;
        next.checked_add(self.latency_frames())?;
        let complete = self.filled + q == b;
        let sequence = self
            .sequence
            .checked_add(u64::from(complete))
            .ok_or(Error::Exhausted)?;
        let collecting = &self.slots[self.write];
        if collecting.completion.is_some() {
            return Err(Error::InvalidState);
        }
        if input.len() > self.limits.events_per_quantum
            || input.byte_len() > self.limits.bytes_per_quantum
            || input.len() > collecting.input.remaining_events()
            || input.byte_len() > collecting.input.remaining_bytes()
        {
            return Err(Error::Capacity);
        }
        for i in 0..input.len() {
            if input.get(i).ok_or(Error::InvalidState)?.0 as usize >= q {
                return Err(Error::InvalidEvent);
            }
        }

        let playing = &self.slots[self.read];
        let mut playback = None;
        if let Some(done) = playing.completion {
            if done.job.generation != self.generation {
                return Err(Error::InvalidState);
            }
            let due = done.job.source.checked_add(self.latency_frames())?;
            if frame >= due {
                let offset = usize::try_from(frame.0 - due.0).map_err(|_| Error::InvalidFrame)?;
                if offset >= b {
                    return Err(Error::InvalidState);
                }
                let mut end_event = playing.played_events;
                let mut event_bytes = 0;
                while let Some((at, packet)) = playing.output.get(end_event) {
                    if at as usize >= offset + q {
                        break;
                    }
                    if (at as usize) < offset {
                        return Err(Error::InvalidState);
                    }
                    event_bytes += packet.len();
                    end_event += 1;
                }
                if end_event - playing.played_events > output.remaining_events()
                    || event_bytes > output.remaining_bytes()
                {
                    return Err(Error::Capacity);
                }
                playback = Some((offset, end_event));
            }
        }

        for c in 0..2 {
            audio.output(c).ok_or(Error::InvalidShape)?.fill(0.);
        }
        if let Some((offset, end_event)) = playback {
            let playing = &mut self.slots[self.read];
            for c in 0..2 {
                audio
                    .output(c)
                    .ok_or(Error::InvalidShape)?
                    .copy_from_slice(&playing.audio[c * b + offset..c * b + offset + q]);
            }
            for i in playing.played_events..end_event {
                let (at, packet) = playing.output.get(i).ok_or(Error::InvalidState)?;
                output
                    .push(at - offset as u32, q, packet)
                    .map_err(capacity)?;
            }
            playing.played_events = end_event;
            if offset + q == b {
                playing.input.clear();
                playing.output.clear();
                playing.completion = None;
                playing.played_events = 0;
                self.read = (self.read + 1) % self.slots.len();
            }
        }
        let collecting = &mut self.slots[self.write];
        for c in 0..2 {
            collecting.audio[c * b + self.filled..c * b + self.filled + q]
                .copy_from_slice(audio.input(c).ok_or(Error::InvalidShape)?);
        }
        for i in 0..input.len() {
            let (at, packet) = input.get(i).ok_or(Error::InvalidState)?;
            collecting
                .input
                .push(at + self.filled as u32, b, packet)
                .map_err(capacity)?;
        }
        self.filled += q;
        if complete {
            let done = self.backend.submit(
                Job {
                    generation: self.generation,
                    sequence: self.sequence,
                    source: self.batch_start,
                },
                b,
                &mut collecting.audio,
                &collecting.input,
                &mut collecting.output,
            )?;
            collecting.completion = Some(done);
            self.write = (self.write + 1) % self.slots.len();
            self.filled = 0;
            self.batch_start = next;
        }
        self.sequence = sequence;
        self.next = next;
        Ok(())
    }
}

fn capacity(error: Error) -> Error {
    if error == Error::Full {
        Error::Capacity
    } else {
        error
    }
}

impl<P: Planner, O: ProcessorOperation> rt_runtime::processor::Processor<P, O>
    for BufferedProcessor<InlineTestProcessor>
{
    fn ports(&self) -> PortCounts {
        self.ports()
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, P>) -> Result<(), Error> {
        if block.frames != block.quantum.frames()
            || block.midi_inputs.len() != 1
            || block.midi_outputs.len() != 1
        {
            return Err(Error::InvalidShape);
        }
        self.process_tick(
            block.frame,
            block.quantum,
            &mut block.audio,
            &block.midi_inputs[0],
            &mut block.midi_outputs[0],
        )
    }
}
