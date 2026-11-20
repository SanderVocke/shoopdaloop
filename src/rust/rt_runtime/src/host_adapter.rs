//! Causal, fixed-latency adaptation from device blocks to complete engine quanta.
use crate::{
    buffer::{AudioBlock, MidiBlock},
    host::{Phase, Renderer, Report},
    Error, Frame,
};
use alloc::{boxed::Box, vec, vec::Vec};

const CHANNELS: usize = 32;
pub struct QuantumAdapter<R> {
    inner: R,
    input: Vec<Box<[f32]>>,
    output: Vec<Box<[f32]>>,
    quantum: usize,
    offset: usize,
    frame: Frame,
    phase: Phase,
}
impl<R: Renderer> QuantumAdapter<R> {
    pub fn new(inner: R, quantum: usize, inputs: usize, outputs: usize) -> Result<Self, Error> {
        if quantum == 0 || inputs > CHANNELS || outputs > CHANNELS {
            return Err(Error::InvalidConfig);
        }
        let frame = inner.next_frame();
        Ok(Self {
            inner,
            input: (0..inputs)
                .map(|_| vec![0.; quantum].into_boxed_slice())
                .collect(),
            output: (0..outputs)
                .map(|_| vec![0.; quantum].into_boxed_slice())
                .collect(),
            quantum,
            offset: 0,
            frame,
            phase: Phase::Running,
        })
    }
    pub fn latency_frames(&self) -> usize {
        self.quantum
    }
    pub fn buffered_frames(&self) -> usize {
        self.offset
    }
    pub fn completed_frame(&self) -> Frame {
        self.inner.next_frame()
    }
    pub fn inner(&self) -> &R {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.inner
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
    fn quantum(&mut self) -> Result<(), Error> {
        let mut inputs: [&[f32]; CHANNELS] = [&[]; CHANNELS];
        for (target, source) in inputs.iter_mut().zip(&self.input) {
            *target = source;
        }
        let mut outputs: [&mut [f32]; CHANNELS] = core::array::from_fn(|_| &mut [] as &mut [f32]);
        let count = self.output.len();
        for (target, source) in outputs.iter_mut().zip(&mut self.output) {
            *target = source;
        }
        let report = self.inner.process(
            AudioBlock {
                frame: self.inner.next_frame(),
                frames: self.quantum,
                inputs: &inputs[..self.input.len()],
                outputs: &mut outputs[..count],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )?;
        self.phase = report.phase;
        Ok(())
    }
}
impl<R: Renderer> Renderer for QuantumAdapter<R> {
    fn next_frame(&self) -> Frame {
        self.frame
    }
    fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<Report, Error> {
        if audio.frame != self.frame
            || audio.inputs.iter().any(|v| v.len() < audio.frames)
            || audio.outputs.iter().any(|v| v.len() < audio.frames)
        {
            return Err(Error::InvalidShape);
        }
        for events in midi.outputs {
            events.clear();
        }
        for output in audio.outputs.iter_mut() {
            output[..audio.frames].fill(0.);
        }
        if let Phase::Faulted(error) = self.phase {
            return Err(error);
        }
        for index in 0..audio.frames {
            for (port, input) in self.input.iter_mut().enumerate() {
                input[self.offset] = audio.inputs.get(port).map_or(0., |v| v[index]);
            }
            for (port, output) in audio.outputs.iter_mut().enumerate() {
                output[index] = self.output.get(port).map_or(0., |v| v[self.offset]);
            }
            self.offset += 1;
            if self.offset == self.quantum {
                self.offset = 0;
                if let Err(error) = self.quantum() {
                    self.phase = Phase::Faulted(error);
                    for output in audio.outputs.iter_mut() {
                        output[..audio.frames].fill(0.);
                    }
                    return Err(error);
                }
            }
        }
        self.frame = Frame(
            self.frame
                .0
                .checked_add(audio.frames as u64)
                .ok_or(Error::InvalidState)?,
        );
        Ok(Report {
            next_frame: self.frame,
            phase: self.phase,
            recovery_packets: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    struct Echo(Frame);
    impl Renderer for Echo {
        fn next_frame(&self) -> Frame {
            self.0
        }
        fn process(&mut self, a: AudioBlock<'_, '_>, _: MidiBlock<'_>) -> Result<Report, Error> {
            assert_eq!(a.frames, 4);
            a.outputs[0].copy_from_slice(a.inputs[0]);
            self.0 .0 += 4;
            Ok(Report {
                next_frame: self.0,
                phase: Phase::Running,
                recovery_packets: 0,
            })
        }
    }
    #[shoop_test]
    fn irregular_device_requests_preserve_input_with_exact_latency_and_partial_quanta() {
        let mut adapter = QuantumAdapter::new(Echo(Frame(0)), 4, 1, 1).unwrap();
        let signal: Vec<f32> = (1..=24).map(|v| v as f32).collect();
        let mut received = Vec::new();
        let mut base = 0;
        for count in [1, 2, 5, 3, 1, 8, 4] {
            let mut output = vec![0.; count];
            let mut outputs: [&mut [f32]; 1] = [&mut output];
            shoop_wasm_test_support::heap::assert_no_heap(|| {
                adapter
                    .process(
                        AudioBlock {
                            frame: Frame(base as u64),
                            frames: count,
                            inputs: &[&signal[base..base + count]],
                            outputs: &mut outputs,
                        },
                        MidiBlock {
                            inputs: &[],
                            outputs: &mut [],
                        },
                    )
                    .unwrap()
            });
            received.extend(output);
            base += count;
            assert_eq!(adapter.completed_frame().0, (base / 4 * 4) as u64);
            assert_eq!(adapter.buffered_frames(), base % 4);
        }
        assert_eq!(&received[..4], &[0.; 4]);
        assert_eq!(&received[4..], &signal[..20]);
        assert_eq!(adapter.next_frame(), Frame(24));
        assert_eq!(adapter.latency_frames(), 4);
    }
}
