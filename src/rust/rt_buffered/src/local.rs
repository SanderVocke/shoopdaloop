use crate::{
    AsyncBatch, AsyncCompletion, AsyncOutput, AsyncTransport, Batch, BatchTag, BufferLimits,
    MidiLimits, ParameterEvent, PreparedTiming, MAX_STORAGE_BYTES,
};
use alloc::{boxed::Box, vec, vec::Vec};
use core::mem::size_of;
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer, MidiCapacity},
    graph::PortCounts,
    Error, Frame,
};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

struct Job {
    tag: BatchTag,
    audio_input: Box<[f32]>,
    audio_output: Box<[f32]>,
    midi_input: Box<[MidiBuffer]>,
    midi_output: Box<[MidiBuffer]>,
    parameters: Vec<ParameterEvent>,
    result: Result<(), Error>,
}

/// Render end of a prepared in-process worker transport.
pub struct LocalTransport {
    frames: usize,
    free: Vec<Job>,
    jobs: Producer<Job>,
    results: Consumer<Job>,
    borrowed: Option<Job>,
}

/// Worker end of a prepared transport. The host controls its execution and
/// wakeup policy independently of the render callback.
pub struct LocalWorker {
    frames: usize,
    jobs: Consumer<Job>,
    results: Producer<Job>,
    previous: Option<BatchTag>,
}

fn midi_capacity(limits: MidiLimits, quanta: usize) -> Result<MidiCapacity, Error> {
    MidiCapacity {
        events: limits.events_per_quantum,
        bytes: limits.bytes_per_quantum,
    }
    .checked_mul(quanta)
}

fn midi_ports(count: usize, capacity: MidiCapacity) -> Result<Box<[MidiBuffer]>, Error> {
    (0..count)
        .map(|_| MidiBuffer::new(capacity.events, capacity.bytes))
        .collect()
}

impl LocalTransport {
    /// Storage for the transfer slots and their queues. The host also budgets
    /// the buffered graph node and plugin storage independently.
    pub fn prepared_bytes(
        ports: PortCounts,
        timing: PreparedTiming,
        limits: BufferLimits,
        slots: usize,
    ) -> Result<usize, Error> {
        if slots == 0 || slots > timing.slots() {
            return Err(Error::InvalidConfig);
        }
        limits.async_storage_bytes_for(ports, timing.quantum(), timing.timing())?;
        let quanta = timing.timing().batch_quanta;
        let midi_input = midi_capacity(limits.midi_input, quanta)?.prepared_bytes()?;
        let midi_output = midi_capacity(limits.midi_output, quanta)?.prepared_bytes()?;
        let bytes = (ports.audio_inputs + ports.audio_outputs)
            .checked_mul(timing.batch_frames() * size_of::<f32>())
            .and_then(|audio| {
                let input =
                    (midi_input + size_of::<MidiBuffer>()).checked_mul(ports.midi_inputs)?;
                let output =
                    (midi_output + size_of::<MidiBuffer>()).checked_mul(ports.midi_outputs)?;
                audio.checked_add(input)?.checked_add(output)
            })
            .and_then(|data| {
                data.checked_add(
                    limits
                        .parameters_per_quantum
                        .checked_mul(quanta * size_of::<ParameterEvent>())?,
                )
            })
            .and_then(|data| data.checked_add(3 * size_of::<Job>()))
            .and_then(|slot| slot.checked_mul(slots))
            .and_then(|storage| {
                storage.checked_add(
                    size_of::<Self>() + size_of::<LocalWorker>() + 2 * size_of::<RingBuffer<Job>>(),
                )
            })
            .filter(|bytes| *bytes <= MAX_STORAGE_BYTES)
            .ok_or(Error::Capacity)?;
        Ok(bytes)
    }

    pub fn prepare(
        ports: PortCounts,
        timing: PreparedTiming,
        limits: BufferLimits,
        slots: usize,
    ) -> Result<(Self, LocalWorker), Error> {
        Self::prepared_bytes(ports, timing, limits, slots)?;
        let frames = timing.batch_frames();
        let quanta = timing.timing().batch_quanta;
        let midi_input = midi_capacity(limits.midi_input, quanta)?;
        let midi_output = midi_capacity(limits.midi_output, quanta)?;
        let mut free = Vec::with_capacity(slots);
        for _ in 0..slots {
            let mut audio_input = vec![0.; frames * ports.audio_inputs].into_boxed_slice();
            let mut audio_output = vec![0.; frames * ports.audio_outputs].into_boxed_slice();
            core::hint::black_box(&mut *audio_input).fill(0.);
            core::hint::black_box(&mut *audio_output).fill(0.);
            let mut parameters = vec![
                ParameterEvent {
                    offset: 0,
                    parameter: 0,
                    value: 0.
                };
                limits.parameters_per_quantum * quanta
            ];
            core::hint::black_box(&mut parameters);
            parameters.clear();
            free.push(Job {
                tag: BatchTag {
                    generation: 0,
                    sequence: 0,
                    source: Frame(0),
                },
                audio_input,
                audio_output,
                midi_input: midi_ports(ports.midi_inputs, midi_input)?,
                midi_output: midi_ports(ports.midi_outputs, midi_output)?,
                parameters,
                result: Ok(()),
            });
        }
        let (submit, jobs) = RingBuffer::new(slots);
        let (complete, results) = RingBuffer::new(slots);
        Ok((
            Self {
                frames,
                free,
                jobs: submit,
                results,
                borrowed: None,
            },
            LocalWorker {
                frames,
                jobs,
                results: complete,
                previous: None,
            },
        ))
    }

    fn recycle(&mut self) {
        if let Some(job) = self.borrowed.take() {
            self.free.push(job);
        }
    }
}

impl Job {
    fn load(&mut self, batch: AsyncBatch<'_>, frames: usize) -> Result<(), Error> {
        if batch.frames != frames
            || batch.audio.len() != self.audio_input.len()
            || batch.midi.len() != self.midi_input.len()
        {
            return Err(Error::InvalidShape);
        }
        if batch.parameters.len() > self.parameters.capacity() {
            return Err(Error::Capacity);
        }
        let mut previous = 0;
        for event in batch.parameters {
            if event.offset < previous
                || event.offset as usize >= frames
                || !event.value.is_finite()
            {
                return Err(Error::InvalidParameter);
            }
            previous = event.offset;
        }
        for (source, destination) in batch.midi.iter().zip(self.midi_input.iter_mut()) {
            destination.clear();
            for index in 0..source.len() {
                let (at, packet) = source.get(index).ok_or(Error::InvalidEvent)?;
                destination.push(at, frames, packet)?;
            }
        }
        self.audio_input.copy_from_slice(batch.audio);
        self.parameters.clear();
        self.parameters.extend_from_slice(batch.parameters);
        self.tag = batch.tag;
        Ok(())
    }
}

impl AsyncTransport for LocalTransport {
    fn try_submit(&mut self, batch: AsyncBatch<'_>) -> Result<(), Error> {
        self.recycle();
        if self.jobs.is_abandoned() {
            return Err(Error::Disconnected);
        }
        let mut job = self.free.pop().ok_or(Error::Full)?;
        if let Err(error) = job.load(batch, self.frames) {
            self.free.push(job);
            return Err(error);
        }
        if let Err(PushError::Full(job)) = self.jobs.push(job) {
            self.free.push(job);
            return Err(Error::Full);
        }
        Ok(())
    }

    fn try_completion(&mut self) -> Option<AsyncCompletion<'_>> {
        self.recycle();
        let job = self.borrowed.insert(self.results.pop().ok()?);
        Some(AsyncCompletion {
            tag: job.tag,
            output: job.result.map(|()| AsyncOutput {
                frames: self.frames,
                audio: &job.audio_output,
                midi: &job.midi_output,
            }),
        })
    }
}

impl LocalWorker {
    /// Run at most one submitted batch. Reset stream-local DSP state before
    /// applying any input when the callback's first argument is true.
    pub fn process_next(
        &mut self,
        process: impl FnOnce(bool, &mut Batch<'_, '_>) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        let Ok(mut job) = self.jobs.pop() else {
            return Ok(false);
        };
        let tag = job.tag;
        let reset = self.previous.is_none_or(|previous| {
            previous.generation != tag.generation
                || previous.sequence.checked_add(1) != Some(tag.sequence)
                || previous.source.checked_add(self.frames).ok() != Some(tag.source)
        });
        job.audio_output.fill(0.);
        for output in &mut job.midi_output {
            output.clear();
        }
        job.result = AudioPorts::new(
            &job.audio_input,
            &mut job.audio_output,
            self.frames,
            0,
            self.frames,
        )
        .and_then(|audio| {
            process(
                reset,
                &mut Batch {
                    tag,
                    frames: self.frames,
                    audio,
                    midi_inputs: &job.midi_input,
                    midi_controls: None,
                    midi_outputs: &mut job.midi_output,
                    parameters: &job.parameters,
                    observer: None,
                },
            )
        });
        self.previous = job.result.is_ok().then_some(tag);
        self.results.push(job).map_err(|_| Error::Full)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Timing;
    use rt_runtime::Quantum;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn local_worker_preserves_ownership_and_resets_after_gaps_failures_and_restarts() {
        let q = Quantum::new(16).unwrap();
        let limits = BufferLimits {
            parameters_per_quantum: 1,
            ..BufferLimits::default()
        };
        let (mut transport, mut worker) = LocalTransport::prepare(
            PortCounts {
                audio_inputs: 1,
                audio_outputs: 1,
                midi_inputs: 1,
                midi_outputs: 1,
            },
            Timing {
                batch_quanta: 1,
                slack_quanta: 1,
            }
            .prepare(q)
            .unwrap(),
            limits,
            1,
        )
        .unwrap();
        let tag = |generation, sequence| BatchTag {
            generation,
            sequence,
            source: Frame(sequence * 16),
        };
        let mut midi = [MidiBuffer::new(1, 3).unwrap()];
        midi[0].push(15, 16, &[0x80, 61, 0]).unwrap();
        let mut audio = [2.; 16];
        let mut parameters = [ParameterEvent {
            offset: 5,
            parameter: 2,
            value: 8.,
        }];
        assert_no_heap(|| {
            transport.try_submit(AsyncBatch {
                tag: tag(1, 0),
                frames: 16,
                audio: &audio,
                midi: &midi,
                parameters: &parameters,
            })
        })
        .unwrap();
        audio.fill(100.);
        midi[0].clear();
        parameters[0].value = 100.;
        let submit = |transport: &mut LocalTransport, generation, seq| {
            transport.try_submit(AsyncBatch {
                tag: tag(generation, seq),
                frames: 16,
                audio: &audio,
                midi: &midi,
                parameters: &parameters,
            })
        };
        assert_eq!(
            assert_no_heap(|| submit(&mut transport, 1, 1)),
            Err(Error::Full)
        );
        assert_no_heap(|| {
            assert!(worker
                .process_next(|reset, batch| {
                    assert!(reset);
                    assert_eq!(batch.audio.input(0), Some(&[2.; 16][..]));
                    assert_eq!(batch.parameters[0].offset, 5);
                    assert_eq!(batch.parameters[0].value, 8.);
                    let (at, packet) = batch.midi_inputs[0].get(0).unwrap();
                    batch.midi_outputs[0].push(at, batch.frames, packet)?;
                    batch.audio.output(0).unwrap()[3] = 9.;
                    Ok(())
                })
                .unwrap());
            assert_eq!(submit(&mut transport, 1, 1), Err(Error::Full));
            let completion = transport.try_completion().unwrap();
            assert_eq!(completion.tag, tag(1, 0));
            let output = completion.output.unwrap();
            assert_eq!(output.audio[3], 9.);
            assert_eq!(output.midi[0].get(0), Some((15, &[0x80, 61, 0][..])));
        });
        for (generation, sequence, reset, result) in [
            (1, 2, true, Err(Error::InvalidState)),
            (1, 3, true, Ok(())),
            (1, 4, false, Ok(())),
            (2, 0, true, Ok(())),
        ] {
            assert_no_heap(|| {
                submit(&mut transport, generation, sequence).unwrap();
                assert!(worker
                    .process_next(|must_reset, batch| {
                        assert_eq!(must_reset, reset);
                        assert!(batch.midi_outputs[0].is_empty());
                        assert_eq!(batch.parameters[0].value, 100.);
                        assert_eq!(batch.audio.output(0), Some(&mut [0.; 16][..]));
                        result
                    })
                    .unwrap());
                let completion = transport.try_completion().unwrap();
                assert_eq!(completion.tag, tag(generation, sequence));
                assert_eq!(completion.output.map(|_| ()), result);
            });
        }
        assert!(!worker
            .process_next(|_, _| panic!("empty queue ran DSP"))
            .unwrap());
        drop(worker);
        assert_eq!(
            assert_no_heap(|| submit(&mut transport, 2, 1)),
            Err(Error::Disconnected)
        );
    }
}
