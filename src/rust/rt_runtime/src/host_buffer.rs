use crate::Sample;
use crate::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Error, Frame, PreparedConfig,
};
use alloc::{boxed::Box, vec, vec::Vec};

pub trait AudioIo<S: Sample = f32> {
    fn frame(&self) -> Frame;
    fn frames(&self) -> usize;
    fn input(&self, port: usize) -> &[S];
    fn output(&mut self, port: usize) -> &mut [S];
    fn pair(&mut self, input: usize, output: usize) -> (&[S], &mut [S]);
}

impl<S: Sample> AudioIo<S> for AudioBlock<'_, '_, S> {
    fn frame(&self) -> Frame {
        self.frame
    }
    fn frames(&self) -> usize {
        self.frames
    }
    fn input(&self, port: usize) -> &[S] {
        self.inputs[port]
    }
    fn output(&mut self, port: usize) -> &mut [S] {
        self.outputs[port]
    }
    fn pair(&mut self, input: usize, output: usize) -> (&[S], &mut [S]) {
        (self.inputs[input], self.outputs[output])
    }
}

pub struct BufferedAudio<'a, S: Sample = f32> {
    frame: Frame,
    frames: usize,
    inputs: &'a [S],
    outputs: &'a mut [S],
}

impl<S: Sample> AudioIo<S> for BufferedAudio<'_, S> {
    fn frame(&self) -> Frame {
        self.frame
    }
    fn frames(&self) -> usize {
        self.frames
    }
    fn input(&self, port: usize) -> &[S] {
        &self.inputs[port * self.frames..(port + 1) * self.frames]
    }
    fn output(&mut self, port: usize) -> &mut [S] {
        &mut self.outputs[port * self.frames..(port + 1) * self.frames]
    }
    fn pair(&mut self, input: usize, output: usize) -> (&[S], &mut [S]) {
        (
            &self.inputs[input * self.frames..(input + 1) * self.frames],
            &mut self.outputs[output * self.frames..(output + 1) * self.frames],
        )
    }
}

/// One-tick stream delay, independent of callback partitioning.
pub struct HostBuffer<S: Sample = f32> {
    q: usize,
    fill: usize,
    processed: Frame,
    input: Box<[S]>,
    output: Box<[S]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
    midi_recovery_wait: bool,
    overflows: usize,
}
impl<S: Sample> HostBuffer<S> {
    pub fn prepare(config: PreparedConfig, quantum: crate::Quantum) -> Result<Self, Error> {
        let c = config.limits();
        Self::prepare_with_output(
            config,
            quantum,
            crate::buffer::MidiCapacity {
                events: c.midi_events_per_port,
                bytes: c.midi_bytes_per_port,
            },
        )
    }
    pub fn prepare_with_output(
        config: PreparedConfig,
        quantum: crate::Quantum,
        output_capacity: crate::buffer::MidiCapacity,
    ) -> Result<Self, Error> {
        let c = config.limits();
        let q = quantum.frames();
        let midi = |n, capacity: crate::buffer::MidiCapacity| {
            (0..n)
                .map(|_| MidiBuffer::new(capacity.events, capacity.bytes))
                .collect::<Result<Vec<_>, _>>()
        };
        let mut input = vec![S::ZERO; c.audio_inputs * q].into_boxed_slice();
        let mut output = vec![S::ZERO; c.audio_outputs * q].into_boxed_slice();
        for sample in input.iter_mut().chain(&mut output) {
            *core::hint::black_box(sample) = S::ZERO;
        }
        Ok(Self {
            q,
            fill: 0,
            processed: Frame(0),
            input,
            output,
            midi_input: midi(
                c.midi_inputs,
                crate::buffer::MidiCapacity {
                    events: c.midi_events_per_port,
                    bytes: c.midi_bytes_per_port,
                },
            )?,
            midi_output: midi(c.midi_outputs, output_capacity)?,
            midi_recovery_wait: false,
            overflows: 0,
        })
    }
    pub fn process(
        &mut self,
        audio: &mut AudioBlock<'_, '_, S>,
        midi: &mut MidiBlock<'_>,
        mut tick: impl FnMut(&mut BufferedAudio<'_, S>, &mut MidiBlock<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let q = self.q;
        self.overflows = 0;
        let mut at = 0;
        while at < audio.frames {
            let count = (q - self.fill).min(audio.frames - at);
            for (ch, input) in audio.inputs.iter().enumerate() {
                self.input[ch * q + self.fill..ch * q + self.fill + count]
                    .copy_from_slice(&input[at..at + count]);
            }
            for (ch, output) in audio.outputs.iter_mut().enumerate() {
                output[at..at + count]
                    .copy_from_slice(&self.output[ch * q + self.fill..ch * q + self.fill + count]);
            }
            for (source, target) in midi.inputs.iter().zip(&mut self.midi_input) {
                merge(
                    target.merge_window(source, at, count, self.fill, q),
                    &mut self.overflows,
                )?;
            }
            for (source, target) in self.midi_output.iter().zip(midi.outputs.iter_mut()) {
                merge(
                    target.merge_window(source, self.fill, count, at, audio.frames),
                    &mut self.overflows,
                )?;
            }
            at += count;
            self.fill += count;
            if self.fill == q {
                self.output.fill(S::ZERO);
                for m in &mut self.midi_output {
                    m.clear();
                }
                tick(
                    &mut BufferedAudio {
                        frame: self.processed,
                        frames: q,
                        inputs: &self.input,
                        outputs: &mut self.output,
                    },
                    &mut MidiBlock {
                        inputs: &self.midi_input,
                        outputs: &mut self.midi_output,
                    },
                )?;
                self.processed = self.processed.checked_add(q)?;
                for m in &mut self.midi_input {
                    m.clear();
                }
                self.midi_recovery_wait = false;
                self.fill = 0;
            }
        }
        Ok(())
    }
    pub fn overflows(&self) -> usize {
        self.overflows
    }

    pub fn clear_midi_output(&mut self, port: usize) {
        self.midi_output[port].clear();
    }

    pub fn defer_midi_recovery(&mut self) {
        self.midi_recovery_wait |= self.fill != 0;
    }

    pub fn midi_recovery_ready(&self) -> bool {
        !self.midi_recovery_wait
    }
}

fn merge(result: Result<(), Error>, overflows: &mut usize) -> Result<(), Error> {
    if result == Err(Error::Full) {
        *overflows = overflows.saturating_add(1);
        Ok(())
    } else {
        result
    }
}
