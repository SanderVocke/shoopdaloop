use super::graph::MAX_HOST_PORTS;
use super::{Processor, Runtime, State};
use crate::processor::ProcessorOperation;
use crate::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Error, Frame,
};
use alloc::{boxed::Box, vec, vec::Vec};

/// Arbitrary host partitions with a constant one-quantum stream delay.
/// The selected buffering policy never changes with individual callback sizes.
pub struct BufferedHost<P, S, O: ProcessorOperation = ()> {
    runtime: Runtime<P, S, O>,
    frame: Frame,
    fill: usize,
    input: Box<[f32]>,
    output: Box<[f32]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
}
impl<P: Processor<S, O>, S: State, O: ProcessorOperation> BufferedHost<P, S, O> {
    pub fn prepare(runtime: Runtime<P, S, O>) -> Result<Self, Error> {
        if runtime.next_frame() != Frame(0) {
            return Err(Error::InvalidState);
        }
        let c = runtime.config.resources.limits();
        let q = runtime.config.quantum.frames();
        let midi = |n| {
            (0..n)
                .map(|_| MidiBuffer::new(c.midi_events_per_port, c.midi_bytes_per_port))
                .collect::<Result<Vec<_>, _>>()
        };
        let mut input = vec![0.; c.audio_inputs * q].into_boxed_slice();
        let mut output = vec![0.; c.audio_outputs * q].into_boxed_slice();
        for sample in input.iter_mut().chain(&mut output) {
            *core::hint::black_box(sample) = 0.;
        }
        Ok(Self {
            runtime,
            frame: Frame(0),
            fill: 0,
            input,
            output,
            midi_input: midi(c.midi_inputs)?,
            midi_output: midi(c.midi_outputs)?,
        })
    }
    pub fn latency_frames(&self) -> usize {
        self.runtime.config.quantum.frames()
    }
    pub fn next_frame(&self) -> Frame {
        self.frame
    }
    pub fn runtime(&self) -> &Runtime<P, S, O> {
        &self.runtime
    }
    pub fn runtime_mut(&mut self) -> &mut Runtime<P, S, O> {
        &mut self.runtime
    }
    pub fn into_runtime(self) -> Runtime<P, S, O> {
        self.runtime
    }
    pub fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<(), Error> {
        if audio.frames == 0 {
            return Ok(());
        }
        self.runtime.validate(&audio, &midi, self.frame)?;
        let q = self.latency_frames();
        for out in audio.outputs.iter_mut() {
            out[..audio.frames].fill(0.);
        }
        for out in midi.outputs.iter_mut() {
            out.clear();
        }
        let result = (|| {
            self.runtime.begin_callback()?;
            let mut at = 0;
            while at < audio.frames {
                let count = (q - self.fill).min(audio.frames - at);
                for (ch, input) in audio.inputs.iter().enumerate() {
                    self.input[ch * q + self.fill..ch * q + self.fill + count]
                        .copy_from_slice(&input[at..at + count]);
                }
                for (ch, output) in audio.outputs.iter_mut().enumerate() {
                    output[at..at + count].copy_from_slice(
                        &self.output[ch * q + self.fill..ch * q + self.fill + count],
                    );
                }
                for (source, target) in midi.inputs.iter().zip(&mut self.midi_input) {
                    target.merge_window(source, at, count, self.fill, q)?;
                }
                for (source, target) in self.midi_output.iter().zip(midi.outputs.iter_mut()) {
                    target.merge_window(source, self.fill, count, at, audio.frames)?;
                }
                self.fill += count;
                at += count;
                if self.fill == q {
                    let mut inputs = [&[][..]; MAX_HOST_PORTS];
                    for (dst, src) in inputs.iter_mut().zip(self.input.chunks_exact(q)) {
                        *dst = src;
                    }
                    self.output.fill(0.);
                    let mut outputs: [&mut [f32]; MAX_HOST_PORTS] =
                        core::array::from_fn(|_| &mut [][..]);
                    for (dst, src) in outputs.iter_mut().zip(self.output.chunks_exact_mut(q)) {
                        *dst = src;
                    }
                    for m in &mut self.midi_output {
                        m.clear();
                    }
                    let ninputs = self.midi_input.len();
                    let noutputs = self.midi_output.len();
                    self.runtime.render_tick(
                        &mut AudioBlock {
                            frame: self.runtime.frame,
                            frames: q,
                            inputs: &inputs[..audio.inputs.len()],
                            outputs: &mut outputs[..audio.outputs.len()],
                        },
                        &mut MidiBlock {
                            inputs: &self.midi_input[..ninputs],
                            outputs: &mut self.midi_output[..noutputs],
                        },
                        0,
                    )?;
                    for m in &mut self.midi_input {
                        m.clear();
                    }
                    self.fill = 0;
                }
            }
            Ok(())
        })();
        let result = self.runtime.end_callback(result);
        if result.is_err() {
            for out in audio.outputs.iter_mut() {
                out[..audio.frames].fill(0.);
            }
            for out in midi.outputs.iter_mut() {
                out.clear();
            }
        } else {
            self.frame = self.frame.checked_add(audio.frames)?;
        }
        result
    }
}
