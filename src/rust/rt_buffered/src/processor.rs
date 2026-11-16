use crate::{
    Backend, Batch, BatchTag, InlineTestProcessor, ParameterEvent, PreparedTiming, SourceObserver,
    Timing,
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
const MAX_PORTS: usize = 64;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferLimits {
    pub midi_input: MidiLimits,
    pub midi_output: MidiLimits,
    pub parameters_per_quantum: usize,
}
impl BufferLimits {
    pub fn storage_bytes_for(
        self,
        ports: PortCounts,
        quantum: Quantum,
        timing: Timing,
    ) -> Result<usize, Error> {
        if ports.total()? > MAX_PORTS {
            return Err(Error::Capacity);
        }
        buffer_storage(ports, timing.prepare(quantum)?, self)
    }
}
impl Default for BufferLimits {
    fn default() -> Self {
        Self {
            midi_input: MidiLimits::default(),
            midi_output: MidiLimits::default(),
            parameters_per_quantum: 64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    Running,
    Draining,
    Suspended,
    Faulted,
}

struct Slot {
    generation: u64,
    audio_input: Box<[f32]>,
    audio_output: Box<[f32]>,
    input: Box<[MidiBuffer]>,
    output: Box<[MidiBuffer]>,
    parameters: Vec<ParameterEvent>,
    tag: Option<BatchTag>,
    played_events: Box<[usize]>,
}
impl Slot {
    fn clear(&mut self) {
        for buffer in self.input.iter_mut().chain(self.output.iter_mut()) {
            buffer.clear();
        }
        self.parameters.clear();
        self.tag = None;
        self.played_events.fill(0);
    }
}

/// A prepared render-owned node with synchronous computation and fixed release time.
pub struct BufferedProcessor<B = InlineTestProcessor> {
    backend: B,
    ports: PortCounts,
    timing: PreparedTiming,
    limits: BufferLimits,
    slots: Box<[Slot]>,
    write: usize,
    read: usize,
    filled: usize,
    next: Frame,
    batch_start: Frame,
    generation: u64,
    sequence: u64,
    state: Lifecycle,
    pending_until: Frame,
    storage_bytes: usize,
    backend_latency: usize,
}

fn product(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_mul(b).ok_or(Error::Capacity)
}
fn sum(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b).ok_or(Error::Capacity)
}
fn midi_storage(limits: MidiLimits, m: usize) -> Result<usize, Error> {
    let events = sum(product(limits.events_per_quantum, m)?, ResetState::EVENTS)?;
    let bytes = sum(product(limits.bytes_per_quantum, m)?, ResetState::BYTES)?;
    sum(
        sum(product(events, size_of::<Event>())?, bytes)?,
        size_of::<MidiBuffer>(),
    )
}
fn midi_ports(count: usize, limits: MidiLimits, m: usize) -> Result<Box<[MidiBuffer]>, Error> {
    (0..count)
        .map(|_| {
            MidiBuffer::new(
                product(limits.events_per_quantum, m)?,
                product(limits.bytes_per_quantum, m)?,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Vec::into_boxed_slice)
}

fn buffer_storage(
    ports: PortCounts,
    timing: PreparedTiming,
    limits: BufferLimits,
) -> Result<usize, Error> {
    rt_runtime::buffer::MidiCapacity {
        events: limits.midi_input.events_per_quantum,
        bytes: limits.midi_input.bytes_per_quantum,
    }
    .prepared_bytes()?;
    let m = timing.timing().batch_quanta;
    let b = timing.batch_frames();
    let controls = product(limits.parameters_per_quantum, m)?;
    let samples = product(b, sum(ports.audio_inputs, ports.audio_outputs)?)?;
    let per_slot = sum(
        sum(
            product(samples, size_of::<f32>())?,
            product(controls, size_of::<ParameterEvent>())?,
        )?,
        sum(
            sum(
                product(
                    midi_storage(limits.midi_input, if m == 1 { 0 } else { m })?,
                    ports.midi_inputs,
                )?,
                product(midi_storage(limits.midi_output, m)?, ports.midi_outputs)?,
            )?,
            sum(
                size_of::<Slot>(),
                product(ports.midi_outputs, size_of::<usize>())?,
            )?,
        )?,
    )?;
    let storage_bytes = product(per_slot, timing.slots())?;
    if storage_bytes > MAX_STORAGE_BYTES {
        return Err(Error::Capacity);
    }
    Ok(storage_bytes)
}

impl BufferedProcessor<InlineTestProcessor> {
    pub fn prepare(quantum: Quantum, timing: Timing, limits: MidiLimits) -> Result<Self, Error> {
        let buffer_limits = BufferLimits {
            midi_input: limits,
            midi_output: limits,
            parameters_per_quantum: 0,
        };
        buffer_storage(
            PortCounts {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_inputs: 1,
                midi_outputs: 1,
            },
            timing.prepare(quantum)?,
            buffer_limits,
        )?;
        Self::with_backend(
            InlineTestProcessor::new(limits.bytes_per_quantum),
            quantum,
            timing,
            BufferLimits {
                midi_input: limits,
                midi_output: limits,
                parameters_per_quantum: 0,
            },
        )
    }
}

impl<B: Backend> BufferedProcessor<B> {
    pub fn with_backend(
        backend: B,
        quantum: Quantum,
        timing: Timing,
        limits: BufferLimits,
    ) -> Result<Self, Error> {
        let timing = timing.prepare(quantum)?;
        let ports = backend.ports();
        if ports.total()? > MAX_PORTS {
            return Err(Error::Capacity);
        }
        let backend_latency = backend.latency_frames();
        if sum(timing.latency_frames(), backend_latency)? > crate::MAX_LATENCY_FRAMES {
            return Err(Error::Capacity);
        }
        let storage_bytes = buffer_storage(ports, timing, limits)?;
        let m = timing.timing().batch_quanta;
        let b = timing.batch_frames();
        let controls = product(limits.parameters_per_quantum, m)?;
        let mut slots = Vec::with_capacity(timing.slots());
        for _ in 0..timing.slots() {
            let mut input = vec![0.; product(b, ports.audio_inputs)?].into_boxed_slice();
            let mut output = vec![0.; product(b, ports.audio_outputs)?].into_boxed_slice();
            core::hint::black_box(&mut *input).fill(0.);
            core::hint::black_box(&mut *output).fill(0.);
            let mut parameters = Vec::with_capacity(controls);
            parameters.resize(
                controls,
                ParameterEvent {
                    offset: 0,
                    parameter: 0,
                    value: 0.,
                },
            );
            core::hint::black_box(&mut parameters);
            parameters.clear();
            slots.push(Slot {
                generation: 1,
                audio_input: input,
                audio_output: output,
                input: midi_ports(
                    ports.midi_inputs,
                    limits.midi_input,
                    if m == 1 { 0 } else { m },
                )?,
                output: midi_ports(ports.midi_outputs, limits.midi_output, m)?,
                parameters,
                tag: None,
                played_events: vec![0; ports.midi_outputs].into_boxed_slice(),
            });
        }
        Ok(Self {
            backend,
            ports,
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
            state: Lifecycle::Running,
            pending_until: Frame(0),
            storage_bytes,
            backend_latency,
        })
    }

    pub fn timing(&self) -> PreparedTiming {
        self.timing
    }
    pub fn latency_frames(&self) -> usize {
        self.timing.latency_frames() + self.backend_latency
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
        self.ports
    }
    pub fn lifecycle(&self) -> Lifecycle {
        self.state
    }
    /// Adapter storage only; the host must also budget backend-owned storage.
    pub fn storage_bytes(&self) -> usize {
        self.storage_bytes
    }
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Inactive access for bounded cleanup and controls. Active backends are
    /// exclusively accessed through source-timed batches.
    pub fn inactive_backend_mut(&mut self) -> Result<&mut B, Error> {
        if self.state != Lifecycle::Suspended {
            return Err(Error::InvalidState);
        }
        Ok(&mut self.backend)
    }

    /// Borrow computed output before its scheduled release. This does not advance
    /// playback; storage remains owned by this adapter until release or reset.
    pub fn completed_audio(&self, source: Frame, port: usize) -> Option<(BatchTag, &[f32])> {
        if self.state == Lifecycle::Faulted || port >= self.ports.audio_outputs {
            return None;
        }
        let frames = self.timing.batch_frames();
        self.slots.iter().find_map(|slot| {
            let tag = slot
                .tag
                .filter(|tag| tag.generation == self.generation && tag.source == source)?;
            Some((tag, &slot.audio_output[port * frames..(port + 1) * frames]))
        })
    }

    /// Admit only source events in the next quantum, before collecting its audio.
    pub fn schedule_parameter(
        &mut self,
        frame: Frame,
        parameter: u32,
        value: f32,
    ) -> Result<(), Error> {
        if self.state != Lifecycle::Running {
            return Err(Error::InvalidState);
        }
        let end = self.next.checked_add(self.timing.quantum().frames())?;
        if frame < self.next || frame >= end {
            return Err(Error::InvalidFrame);
        }
        self.backend.validate_parameter(parameter, value)?;
        let offset = self.filled
            + usize::try_from(frame.0 - self.next.0).map_err(|_| Error::InvalidFrame)?;
        let slot = &mut self.slots[self.write];
        if slot.generation != self.generation {
            slot.clear();
            slot.generation = self.generation;
        }
        if slot
            .parameters
            .last()
            .is_some_and(|event| event.offset as usize > offset)
        {
            return Err(Error::InvalidFrame);
        }
        let count = slot
            .parameters
            .iter()
            .rev()
            .take_while(|event| event.offset as usize >= self.filled)
            .count();
        if count >= self.limits.parameters_per_quantum {
            return Err(Error::Capacity);
        }
        slot.parameters.push(ParameterEvent {
            offset: offset as u32,
            parameter,
            value,
        });
        Ok(())
    }

    pub fn begin_drain(&mut self) -> Result<(), Error> {
        if self.state != Lifecycle::Running {
            return Err(Error::InvalidState);
        }
        self.state = Lifecycle::Draining;
        Ok(())
    }

    /// Call after downstream MIDI cleanup. Invalidates storage without scanning audio.
    pub fn abort(&mut self, origin: Frame) -> Result<(), Error> {
        self.rewind(origin)?;
        self.state = Lifecycle::Suspended;
        Ok(())
    }

    /// Begin a stream whose inactive backend has already been reset/prepared.
    pub fn activate_prepared(&mut self, origin: Frame) -> Result<(), Error> {
        if self.state != Lifecycle::Suspended {
            return Err(Error::InvalidState);
        }
        self.rewind(origin)?;
        self.state = Lifecycle::Running;
        Ok(())
    }

    pub fn activate(&mut self, origin: Frame) -> Result<(), Error> {
        if self.state != Lifecycle::Suspended {
            return Err(Error::InvalidState);
        }
        self.reset(origin)
    }

    /// Quiescent reset, or activation of an inactive node after MIDI cleanup.
    pub fn reset(&mut self, origin: Frame) -> Result<(), Error> {
        self.check_origin(origin)?;
        if let Err(error) = self.backend.reset() {
            self.state = Lifecycle::Faulted;
            return Err(error);
        }
        self.rewind(origin)?;
        self.state = Lifecycle::Running;
        Ok(())
    }
    fn check_origin(&self, origin: Frame) -> Result<(), Error> {
        if !self.timing.quantum().aligned(origin) {
            return Err(Error::InvalidFrame);
        }
        origin.checked_add(self.latency_frames() + self.timing.quantum().frames())?;
        self.generation.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(())
    }
    fn rewind(&mut self, origin: Frame) -> Result<(), Error> {
        self.check_origin(origin)?;
        self.generation += 1;
        self.write = 0;
        self.read = 0;
        self.filled = 0;
        self.next = origin;
        self.batch_start = origin;
        self.pending_until = origin;
        self.sequence = 0;
        self.slots[0].clear();
        self.slots[0].generation = self.generation;
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
        self.process_ports_tick(
            frame,
            quantum,
            audio,
            core::slice::from_ref(input),
            core::slice::from_mut(output),
        )
    }

    pub fn process_ports_tick(
        &mut self,
        frame: Frame,
        quantum: Quantum,
        audio: &mut AudioPorts<'_>,
        inputs: &[MidiBuffer],
        outputs: &mut [MidiBuffer],
    ) -> Result<(), Error> {
        self.process_ports_tick_inner(frame, quantum, audio, inputs, outputs, None, None)
    }

    /// Observe only when a complete batch executes. The caller retains requests
    /// across collection ticks when using batches larger than one quantum.
    pub fn process_ports_tick_observed(
        &mut self,
        frame: Frame,
        quantum: Quantum,
        audio: &mut AudioPorts<'_>,
        inputs: &[MidiBuffer],
        outputs: &mut [MidiBuffer],
        observer: &mut dyn SourceObserver,
    ) -> Result<(), Error> {
        if !self.backend.supports_observation() {
            return Err(Error::InvalidState);
        }
        self.process_ports_tick_inner(frame, quantum, audio, inputs, outputs, None, Some(observer))
    }

    /// Merge borrowed controls while executing one complete source quantum.
    pub fn process_ports_tick_controls(
        &mut self,
        frame: Frame,
        quantum: Quantum,
        audio: &mut AudioPorts<'_>,
        inputs: &[MidiBuffer],
        outputs: &mut [MidiBuffer],
        controls: Option<crate::MidiControls<'_>>,
        observer: Option<&mut dyn SourceObserver>,
    ) -> Result<(), Error> {
        if self.timing.timing().batch_quanta != 1
            || inputs.len() != 1 && controls.is_some()
            || observer.is_some() && !self.backend.supports_observation()
        {
            return Err(Error::InvalidShape);
        }
        self.process_ports_tick_inner(frame, quantum, audio, inputs, outputs, controls, observer)
    }

    fn process_ports_tick_inner(
        &mut self,
        frame: Frame,
        quantum: Quantum,
        audio: &mut AudioPorts<'_>,
        inputs: &[MidiBuffer],
        outputs: &mut [MidiBuffer],
        controls: Option<crate::MidiControls<'_>>,
        observer: Option<&mut dyn SourceObserver>,
    ) -> Result<(), Error> {
        if self.state == Lifecycle::Faulted {
            return Err(Error::InvalidState);
        }
        let q = self.timing.quantum().frames();
        let b = self.timing.batch_frames();
        if quantum != self.timing.quantum() || frame != self.next {
            return Err(Error::InvalidFrame);
        }
        if audio.input_count() != self.ports.audio_inputs
            || audio.output_count() != self.ports.audio_outputs
            || inputs.len() != self.ports.midi_inputs
            || outputs.len() != self.ports.midi_outputs
            || (0..self.ports.audio_inputs).any(|c| audio.input(c).is_none_or(|v| v.len() != q))
            || (0..self.ports.audio_outputs).any(|c| audio.output(c).is_none_or(|v| v.len() != q))
        {
            return Err(Error::InvalidShape);
        }
        if outputs
            .iter()
            .any(|output| !output.is_empty() || output.is_recovery())
        {
            return Err(Error::InvalidState);
        }
        let next = frame.checked_add(q)?;
        next.checked_add(self.latency_frames())?;
        let collecting = self.state == Lifecycle::Running
            || (self.state == Lifecycle::Draining && self.filled > 0);
        let complete = collecting && self.filled + q == b;
        let sequence = self
            .sequence
            .checked_add(u64::from(complete))
            .ok_or(Error::Exhausted)?;
        if collecting {
            let slot = &self.slots[self.write];
            if slot
                .tag
                .is_some_and(|tag| tag.generation == self.generation)
            {
                return Err(Error::InvalidState);
            }
            if self.state == Lifecycle::Running {
                for (port, (ordinary, dest)) in inputs.iter().zip(slot.input.iter()).enumerate() {
                    let input = crate::MidiInput::new(ordinary, controls.filter(|_| port == 0));
                    if ordinary.input_lost() {
                        continue;
                    }
                    if input.len() > self.limits.midi_input.events_per_quantum
                        || input.byte_len() > self.limits.midi_input.bytes_per_quantum
                        || (self.filled > 0
                            && (input.len() > dest.remaining_events()
                                || input.byte_len() > dest.remaining_bytes()))
                    {
                        return Err(Error::Capacity);
                    }
                    for (offset, _) in input.iter() {
                        if offset as usize >= q {
                            return Err(Error::InvalidEvent);
                        }
                    }
                }
            }
        }
        let playing = &self.slots[self.read];
        let playback = if let Some(tag) =
            playing.tag.filter(|tag| tag.generation == self.generation)
        {
            let due = tag.source.checked_add(self.timing.latency_frames())?;
            if frame >= due {
                let offset = usize::try_from(frame.0 - due.0).map_err(|_| Error::InvalidFrame)?;
                if offset >= b {
                    return Err(Error::InvalidState);
                }
                for (port, output) in outputs.iter().enumerate() {
                    let buffer = &playing.output[port];
                    let mut count = 0;
                    let mut bytes = 0;
                    for i in playing.played_events[port]..buffer.len() {
                        let (at, packet) = buffer.get(i).ok_or(Error::InvalidState)?;
                        if at as usize >= offset + q {
                            break;
                        }
                        if (at as usize) < offset {
                            return Err(Error::InvalidState);
                        }
                        count += 1;
                        bytes += packet.len();
                    }
                    if count > output.remaining_events() || bytes > output.remaining_bytes() {
                        return Err(Error::Capacity);
                    }
                }
                Some(offset)
            } else {
                None
            }
        } else {
            None
        };
        for c in 0..self.ports.audio_outputs {
            audio.output(c).ok_or(Error::InvalidShape)?.fill(0.);
        }
        if let Some(offset) = playback {
            let slot = &mut self.slots[self.read];
            for c in 0..self.ports.audio_outputs {
                audio
                    .output(c)
                    .ok_or(Error::InvalidShape)?
                    .copy_from_slice(&slot.audio_output[c * b + offset..c * b + offset + q]);
            }
            for (port, output) in outputs.iter_mut().enumerate() {
                let buffer = &slot.output[port];
                while let Some((at, packet)) = buffer.get(slot.played_events[port]) {
                    if at as usize >= offset + q {
                        break;
                    }
                    output
                        .push(at - offset as u32, q, packet)
                        .map_err(capacity)?;
                    slot.played_events[port] += 1;
                }
            }
            if offset + q == b {
                slot.clear();
                self.read = (self.read + 1) % self.slots.len();
            }
        }
        if collecting {
            let slot = &mut self.slots[self.write];
            if slot.generation != self.generation {
                slot.clear();
                slot.generation = self.generation;
            }
            for c in 0..self.ports.audio_inputs {
                let dest = &mut slot.audio_input[c * b + self.filled..c * b + self.filled + q];
                if self.state == Lifecycle::Running {
                    dest.copy_from_slice(audio.input(c).ok_or(Error::InvalidShape)?);
                } else {
                    dest.fill(0.);
                }
            }
            if self.state == Lifecycle::Running && b != q {
                for (input, dest) in inputs.iter().zip(slot.input.iter_mut()) {
                    if input.input_lost() {
                        if input.is_recovery() || dest.is_recovery() {
                            dest.mark_input_loss();
                        } else {
                            dest.mark_input_loss_silent();
                        }
                        continue;
                    }
                    if dest.input_lost() {
                        continue;
                    }
                    for i in 0..input.len() {
                        let (at, packet) = input.get(i).ok_or(Error::InvalidState)?;
                        dest.push(at + self.filled as u32, b, packet)
                            .map_err(capacity)?;
                    }
                }
            }
            self.filled += q;
            if complete {
                let tag = BatchTag {
                    generation: self.generation,
                    sequence: self.sequence,
                    source: self.batch_start,
                };
                let mut batch = Batch {
                    tag,
                    frames: b,
                    audio: AudioPorts::new(&slot.audio_input, &mut slot.audio_output, b, 0, b)?,
                    midi_inputs: if b == q && self.state == Lifecycle::Running {
                        inputs
                    } else {
                        &slot.input
                    },
                    midi_controls: controls,
                    midi_outputs: &mut slot.output,
                    parameters: &slot.parameters,
                    observer,
                };
                if let Err(error) = self.backend.process(&mut batch) {
                    self.state = Lifecycle::Faulted;
                    return Err(capacity(error));
                }
                for buffer in &slot.output {
                    let mut slice = 0;
                    let mut count = 0;
                    let mut bytes = 0;
                    for i in 0..buffer.len() {
                        let (at, packet) = buffer.get(i).ok_or(Error::InvalidState)?;
                        if at as usize >= b {
                            self.state = Lifecycle::Faulted;
                            return Err(Error::InvalidEvent);
                        }
                        let current = at as usize / q;
                        if current != slice {
                            slice = current;
                            count = 0;
                            bytes = 0;
                        }
                        count += 1;
                        bytes += packet.len();
                        if count > self.limits.midi_output.events_per_quantum
                            || bytes > self.limits.midi_output.bytes_per_quantum
                        {
                            self.state = Lifecycle::Faulted;
                            return Err(Error::Capacity);
                        }
                    }
                }
                slot.tag = Some(tag);
                self.pending_until = next.checked_add(self.timing.latency_frames())?;
                self.write = (self.write + 1) % self.slots.len();
                self.filled = 0;
                self.batch_start = next;
            }
        }
        self.sequence = sequence;
        self.next = next;
        if self.state == Lifecycle::Draining && self.filled == 0 && next >= self.pending_until {
            self.state = Lifecycle::Suspended;
        }
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
impl<P: Planner, O: ProcessorOperation, B: Backend> rt_runtime::processor::Processor<P, O>
    for BufferedProcessor<B>
{
    fn ports(&self) -> PortCounts {
        self.ports
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, P>) -> Result<(), Error> {
        if block.frames != block.quantum.frames() {
            return Err(Error::InvalidShape);
        }
        self.process_ports_tick(
            block.frame,
            block.quantum,
            &mut block.audio,
            block.midi_inputs,
            block.midi_outputs,
        )
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        self.schedule_parameter(self.next, parameter, value)
    }
}
