use alloc::{boxed::Box, vec};
use core::mem::size_of;
use rt_runtime::{
    buffer::{AudioPorts, MidiBuffer, MidiCapacity},
    resource::{DelayedCaptureTake, DelayedCaptureTick},
    Error, Frame, NodeId, Quantum, Revision, Sample,
};

#[derive(Clone, Copy)]
pub struct WorkerCaptureInputSettings {
    pub sources: usize,
    pub edit_loops: usize,
    pub channels: usize,
    pub midi: MidiCapacity,
    pub delay_frames: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerCaptureSource {
    pub strip_generation: u32,
    pub processor: NodeId,
    pub processor_generation: u32,
    pub delay_frames: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerCaptureRelease {
    pub owner: WorkerCaptureSource,
    pub source_frame: Frame,
    pub render_frame: Frame,
    pub stream_generation: u64,
}

#[derive(Clone, Copy)]
pub(super) struct DelayedTake {
    pub token: Option<DelayedCaptureTake>,
    pub cut: Option<Frame>,
    pub interval: Option<DelayedCaptureTick>,
    pub stream_generation: Option<u64>,
}

impl DelayedTake {
    pub fn release_source(self, start: Frame, frame: Frame, delay: usize) -> Option<Frame> {
        let source = Frame(frame.0.checked_sub(delay as u64)?);
        (source >= start && self.cut.is_none_or(|cut| source < cut)).then_some(source)
    }
}

#[derive(Clone, Copy)]
struct EditPosition {
    frame: Frame,
    target: crate::LoopId,
    revision: Revision,
    position: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Stamp {
    generation: u32,
    frame: Frame,
}

struct Input<S> {
    stamp: Option<Stamp>,
    release: Option<WorkerCaptureRelease>,
    audio: Box<[S]>,
    midi: MidiBuffer,
}

pub(super) struct WorkerCaptureInputs<S> {
    inputs: Box<[Input<S>]>,
    edit_positions: Box<[Option<EditPosition>]>,
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
        let slots = self.slots(quantum)?;
        let audio_and_midi = self
            .sources
            .checked_mul(slot_bytes)
            .ok_or(Error::Capacity)?;
        let positions = self
            .edit_loops
            .checked_mul(size_of::<Option<EditPosition>>())
            .ok_or(Error::Capacity)?;
        audio_and_midi
            .checked_add(positions)
            .and_then(|bytes| bytes.checked_mul(slots))
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
                    release: None,
                    audio,
                    midi: MidiBuffer::new(settings.midi.events, settings.midi.bytes)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        let mut edit_positions =
            vec![None; slots_per_source * settings.edit_loops].into_boxed_slice();
        for position in &mut edit_positions {
            *core::hint::black_box(position) = None;
        }
        Ok(Self {
            inputs,
            edit_positions,
            slots_per_source,
            channels: settings.channels,
            quantum,
        })
    }

    pub(super) fn accepts(
        &self,
        source: usize,
        owner: WorkerCaptureSource,
        channels: usize,
    ) -> bool {
        self.index(source, Frame(0)).is_some()
            && owner.delay_frames != 0
            && owner.delay_frames % self.quantum.frames() == 0
            && owner.delay_frames / self.quantum.frames() < self.slots_per_source
            && owner.strip_generation != 0
            && owner.processor_generation != 0
            && channels == self.channels
    }

    fn index(&self, source: usize, frame: Frame) -> Option<usize> {
        self.slot(source, frame, self.inputs.len())
    }

    fn slot(&self, source: usize, frame: Frame, storage_len: usize) -> Option<usize> {
        if !self.quantum.aligned(frame) || source >= storage_len / self.slots_per_source {
            return None;
        }
        let tick = frame.0 / self.quantum.frames() as u64;
        Some(source * self.slots_per_source + (tick % self.slots_per_source as u64) as usize)
    }

    pub(super) fn observe_edit_position(
        &mut self,
        target: crate::LoopId,
        revision: Revision,
        frame: Frame,
        position: u64,
    ) -> Result<(), Error> {
        let index = self
            .slot(target.slot() as usize, frame, self.edit_positions.len())
            .ok_or(Error::InvalidId)?;
        self.edit_positions[index] = Some(EditPosition {
            frame,
            target,
            revision,
            position,
        });
        Ok(())
    }

    pub(super) fn edit_position(
        &self,
        target: crate::LoopId,
        revision: Revision,
        frame: Frame,
    ) -> Option<u64> {
        let index = self.slot(target.slot() as usize, frame, self.edit_positions.len())?;
        let position = self.edit_positions[index]?;
        (position.frame == frame && position.target == target && position.revision == revision)
            .then_some(position.position)
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
        input.release = None;
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

    pub(super) fn publish_release(&mut self, source: usize, release: WorkerCaptureRelease) -> bool {
        let Some(index) = self.index(source, release.source_frame) else {
            return false;
        };
        let input = &mut self.inputs[index];
        if input.stamp
            != Some(Stamp {
                generation: release.owner.strip_generation,
                frame: release.source_frame,
            })
            || release.stream_generation == 0
            || release
                .source_frame
                .checked_add(release.owner.delay_frames)
                .ok()
                != Some(release.render_frame)
            || input.release.is_some()
        {
            return false;
        }
        input.release = Some(release);
        true
    }

    pub(super) fn completed(
        &self,
        source: usize,
        owner: WorkerCaptureSource,
        render_frame: Frame,
    ) -> Option<(&[S], &MidiBuffer, u64)> {
        let frame = Frame(render_frame.0.checked_sub(owner.delay_frames as u64)?);
        let input = &self.inputs[self.index(source, frame)?];
        let release = input.release?;
        if release.owner != owner || release.render_frame != render_frame {
            return None;
        }
        Some((&input.audio, &input.midi, release.stream_generation))
    }

    #[cfg(test)]
    fn read(&self, source: usize, generation: u32, frame: Frame) -> Option<(&[S], &MidiBuffer)> {
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
            edit_loops: 2,
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
