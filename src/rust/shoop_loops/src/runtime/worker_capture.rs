use alloc::{boxed::Box, vec};
use core::mem::size_of;
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer, MidiCapacity},
    Error, Frame, Quantum, Sample,
};

#[derive(Clone, Copy)]
pub struct WorkerCaptureInputSettings {
    pub sources: usize,
    pub channels: usize,
    pub midi: MidiCapacity,
    pub delay_frames: usize,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Stamp {
    generation: u32,
    frame: Frame,
}

struct Input<S> {
    stamp: Option<Stamp>,
    audio: Box<[S]>,
    midi: MidiBuffer,
}

pub(super) struct WorkerCaptureInputs<S> {
    inputs: Box<[Input<S>]>,
    slots_per_source: usize,
    channels: usize,
    quantum: Quantum,
}

impl WorkerCaptureInputSettings {
    fn slots(self, quantum: Quantum) -> Result<usize, Error> {
        if self.sources == 0 || self.delay_frames == 0 || self.delay_frames % quantum.frames() != 0
        {
            return Err(Error::InvalidConfig);
        }
        (self.delay_frames / quantum.frames())
            .checked_add(1)
            .ok_or(Error::Capacity)
    }

    pub fn storage_bytes<S: Sample>(self, quantum: Quantum) -> Result<usize, Error> {
        let midi = self.midi.prepared_bytes()?;
        let slot_bytes = self
            .channels
            .checked_mul(quantum.frames() * size_of::<S>())
            .and_then(|audio| audio.checked_add(midi))
            .and_then(|payload| payload.checked_add(size_of::<Input<S>>()))
            .ok_or(Error::Capacity)?;
        self.slots(quantum)?
            .checked_mul(self.sources)
            .and_then(|slots| slots.checked_mul(slot_bytes))
            .filter(|bytes| *bytes <= i32::MAX as usize)
            .ok_or(Error::Capacity)
    }
}

impl<S: Sample> WorkerCaptureInputs<S> {
    pub(super) fn prepare(
        settings: WorkerCaptureInputSettings,
        quantum: Quantum,
    ) -> Result<Self, Error> {
        settings.storage_bytes::<S>(quantum)?;
        let slots_per_source = settings.slots(quantum)?;
        let inputs = (0..slots_per_source * settings.sources)
            .map(|_| {
                let mut audio =
                    vec![S::ZERO; settings.channels * quantum.frames()].into_boxed_slice();
                core::hint::black_box(&mut *audio).fill(S::ZERO);
                Ok(Input {
                    stamp: None,
                    audio,
                    midi: MidiBuffer::new(settings.midi.events, settings.midi.bytes)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            inputs,
            slots_per_source,
            channels: settings.channels,
            quantum,
        })
    }

    fn index(&self, source: usize, frame: Frame) -> Option<usize> {
        if !self.quantum.aligned(frame) || source >= self.inputs.len() / self.slots_per_source {
            return None;
        }
        let tick = frame.0 / self.quantum.frames() as u64;
        Some(source * self.slots_per_source + (tick % self.slots_per_source as u64) as usize)
    }

    pub(super) fn observe(
        &mut self,
        source: usize,
        generation: u32,
        frame: Frame,
        ports: &mut AudioPorts<'_, S>,
        midi: Option<&MidiBuffer>,
    ) -> Result<(), Error> {
        let index = self.index(source, frame).ok_or(Error::InvalidFrame)?;
        let input = &mut self.inputs[index];
        input.stamp = None;
        if generation == 0 {
            return Err(Error::InvalidId);
        }
        let frames = self.quantum.frames();
        for channel in 0..self.channels {
            let samples = ports
                .output(channel)
                .filter(|samples| samples.len() == frames)
                .ok_or(Error::InvalidShape)?;
            for (frame, &sample) in samples.iter().enumerate() {
                input.audio[frame * self.channels + channel] = sample;
            }
        }
        input.midi.clear();
        if let Some(midi) = midi {
            if midi.is_recovery() {
                return Err(Error::Incomplete);
            }
            for index in 0..midi.len() {
                let (offset, payload) = midi.get(index).ok_or(Error::InvalidEvent)?;
                input.midi.push(offset, frames, payload)?;
            }
        }
        input.stamp = Some(Stamp { generation, frame });
        Ok(())
    }

    pub(super) fn read(
        &self,
        source: usize,
        generation: u32,
        frame: Frame,
    ) -> Option<(&[S], &MidiBuffer)> {
        let input = &self.inputs[self.index(source, frame)?];
        (input.stamp == Some(Stamp { generation, frame }))
            .then_some((input.audio.as_ref(), &input.midi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn worker_input_history_pairs_exact_frames_and_rejects_reused_owners_and_midi_loss() {
        let quantum = Quantum::new(16).unwrap();
        let settings = WorkerCaptureInputSettings {
            sources: 2,
            channels: 2,
            midi: MidiCapacity {
                events: 2,
                bytes: 6,
            },
            delay_frames: 48,
        };
        let mut history = WorkerCaptureInputs::prepare(settings, quantum).unwrap();
        let mut midi = MidiBuffer::new(2, 6).unwrap();
        let mut audio = [1.; 32];
        audio[16..].fill(2.);
        midi.push(15, 16, &[0x90, 60, 100]).unwrap();
        let mut ports = AudioPorts::new(&[], &mut audio, 16, 0, 16).unwrap();
        assert_no_heap(|| {
            history
                .observe(0, 2, Frame(0), &mut ports, Some(&midi))
                .unwrap();
            history.observe(1, 3, Frame(0), &mut ports, None).unwrap();
        });
        midi.clear();
        ports.output(1).unwrap().fill(7.);
        for frame in [16, 32, 48] {
            assert_no_heap(|| history.observe(0, 2, Frame(frame), &mut ports, None)).unwrap();
        }
        let (dry, midi_out) = history.read(0, 2, Frame(0)).unwrap();
        assert!(dry.chunks_exact(2).all(|sample| sample == [1., 2.]));
        assert_eq!(midi_out.get(0), Some((15, &[0x90, 60, 100][..])));
        assert!(history.read(0, 3, Frame(0)).is_none());
        assert!(history.read(1, 2, Frame(0)).is_none());
        assert!(history.read(1, 3, Frame(0)).unwrap().1.is_empty());
        midi.recover_notes();
        assert_eq!(
            assert_no_heap(|| history.observe(0, 4, Frame(64), &mut ports, Some(&midi))),
            Err(Error::Incomplete)
        );
        assert!(history.read(0, 2, Frame(0)).is_none());
        assert!(history.read(0, 4, Frame(64)).is_none());
        assert!(settings.storage_bytes::<f32>(quantum).unwrap() > 32 * 4 * 8);
    }
}
