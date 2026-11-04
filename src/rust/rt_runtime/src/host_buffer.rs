use crate::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    Error, Frame, PreparedConfig,
};
use alloc::{boxed::Box, vec, vec::Vec};

const PORTS: usize = 1024;

/// One-tick stream delay, independent of callback partitioning.
pub(crate) struct HostBuffer {
    q: usize,
    fill: usize,
    processed: Frame,
    input: Box<[f32]>,
    output: Box<[f32]>,
    midi_input: Vec<MidiBuffer>,
    midi_output: Vec<MidiBuffer>,
    overflows: usize,
}
impl HostBuffer {
    pub fn prepare(config: PreparedConfig, quantum: crate::Quantum) -> Result<Self, Error> {
        let c = config.limits();
        if c.audio_inputs > PORTS || c.audio_outputs > PORTS {
            return Err(Error::InvalidConfig);
        }
        let q = quantum.frames();
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
            q,
            fill: 0,
            processed: Frame(0),
            input,
            output,
            midi_input: midi(c.midi_inputs)?,
            midi_output: midi(c.midi_outputs)?,
            overflows: 0,
        })
    }
    pub fn process(
        &mut self,
        audio: &mut AudioBlock<'_, '_>,
        midi: &mut MidiBlock<'_>,
        tick: impl FnMut(&mut AudioBlock<'_, '_>, &mut MidiBlock<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if audio.inputs.len().max(audio.outputs.len()) <= 64 {
            self.process_ports::<64>(audio, midi, tick)
        } else {
            self.process_ports::<PORTS>(audio, midi, tick)
        }
    }
    fn process_ports<const N: usize>(
        &mut self,
        audio: &mut AudioBlock<'_, '_>,
        midi: &mut MidiBlock<'_>,
        mut tick: impl FnMut(&mut AudioBlock<'_, '_>, &mut MidiBlock<'_>) -> Result<(), Error>,
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
                let mut inputs = [&[][..]; N];
                for (dst, src) in inputs.iter_mut().zip(self.input.chunks_exact(q)) {
                    *dst = src;
                }
                self.output.fill(0.);
                let mut outputs: [&mut [f32]; N] = core::array::from_fn(|_| &mut [][..]);
                for (dst, src) in outputs.iter_mut().zip(self.output.chunks_exact_mut(q)) {
                    *dst = src;
                }
                for m in &mut self.midi_output {
                    m.clear();
                }
                tick(
                    &mut AudioBlock {
                        frame: self.processed,
                        frames: q,
                        inputs: &inputs[..audio.inputs.len()],
                        outputs: &mut outputs[..audio.outputs.len()],
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
                self.fill = 0;
            }
        }
        Ok(())
    }
    pub fn overflows(&self) -> usize {
        self.overflows
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
