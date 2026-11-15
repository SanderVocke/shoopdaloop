use crate::resource::{AudioChunkRef, ReadIndex, ResourceStore};
use crate::Sample;
use crate::{Error, Frame, NodeId, Quantum};
use alloc::{boxed::Box, vec, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainEnvelope<S: Sample = f32> {
    pub start: S,
    pub end: S,
}

impl<S: Sample> Default for GainEnvelope<S> {
    fn default() -> Self {
        Self {
            start: S::ONE,
            end: S::ONE,
        }
    }
}

impl<S: Sample> GainEnvelope<S> {
    pub fn constant(gain: S) -> Self {
        Self {
            start: gain,
            end: gain,
        }
    }
    pub fn compose(self, other: Self) -> Self {
        Self {
            start: self.start * other.start,
            end: self.end * other.end,
        }
    }
    pub fn at(self, offset: usize, frames: usize) -> S {
        if frames <= 1 || offset == 0 {
            self.start
        } else if offset + 1 == frames {
            self.end
        } else {
            S::interpolate(self.start, self.end, (frames - 1) as u32, offset as u32)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChunkRef {
    Silence,
    Transient { offset: usize, epoch: u64 },
    Persistent(AudioChunkRef),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChunkSpan {
    pub chunk: ChunkRef,
    pub frames: usize,
}

impl Default for ChunkSpan {
    fn default() -> Self {
        Self {
            chunk: ChunkRef::Silence,
            frames: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioSequence<S: Sample = f32> {
    pub spans: [ChunkSpan; 2],
    pub len: usize,
    pub gain: GainEnvelope<S>,
}

impl<S: Sample> AudioSequence<S> {
    fn transient(offset: usize, frames: usize, epoch: u64) -> Self {
        Self {
            spans: [
                ChunkSpan {
                    chunk: ChunkRef::Transient { offset, epoch },
                    frames,
                },
                ChunkSpan::default(),
            ],
            len: 1,
            gain: GainEnvelope::default(),
        }
    }
    pub fn scaled(mut self, gain: GainEnvelope<S>) -> Self {
        self.gain = self.gain.compose(gain);
        self
    }
    pub fn silent(self) -> bool {
        self.len == 0
    }
    fn source_frames(self) -> u64 {
        self.spans[..self.len]
            .iter()
            .filter(|span| span.chunk != ChunkRef::Silence)
            .map(|span| span.frames as u64)
            .sum()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioGraphWork {
    pub forwarded: u64,
    pub materialized: u64,
    pub sample_reads: u64,
    pub sample_writes: u64,
    pub arena_peak_samples: u64,
}

pub struct RenderArena<S: Sample = f32> {
    epoch: u64,
    data: Box<[S]>,
    used: usize,
    pins: Vec<Option<AudioChunkRef>>,
    pin_limit: usize,
    pub work: AudioGraphWork,
}

impl<S: Sample> RenderArena<S> {
    pub fn prepare(samples: usize, pins: usize) -> Self {
        let mut data = vec![S::ZERO; samples].into_boxed_slice();
        for sample in &mut data {
            *core::hint::black_box(sample) = S::ZERO;
        }
        let pin_limit = pins;
        // The extra tag touches the page containing the final usable slot's payload.
        let mut pins = vec![None; pin_limit + 1];
        for pin in &mut pins {
            *core::hint::black_box(pin) = None;
        }
        pins.clear();
        Self {
            epoch: 0,
            data,
            used: 0,
            pins,
            pin_limit,
            work: AudioGraphWork::default(),
        }
    }
    pub fn reset(&mut self) -> Result<(), Error> {
        if !self.pins.is_empty() {
            return Err(Error::InvalidState);
        }
        self.epoch = self.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        self.used = 0;
        self.work = AudioGraphWork::default();
        Ok(())
    }
    pub fn transient(&self, offset: usize, frames: usize) -> AudioSequence<S> {
        AudioSequence::transient(offset, frames, self.epoch)
    }
    pub fn allocate(&mut self, samples: usize) -> Result<usize, Error> {
        let start = self.used;
        if samples == 0 {
            return Ok(start);
        }
        self.used = start
            .checked_add(samples)
            .filter(|&end| end <= self.data.len())
            .ok_or(Error::Capacity)?;
        self.data[start..self.used].fill(S::ZERO);
        self.work.arena_peak_samples = self.used as u64;
        self.work.materialized += 1;
        self.work.sample_writes += samples as u64;
        Ok(start)
    }
    pub fn output(&mut self, offset: usize, samples: usize) -> &mut [S] {
        &mut self.data[offset..offset + samples]
    }
    pub fn pin_sequence(
        &mut self,
        sequence: &mut AudioSequence<S>,
        resources: &mut ResourceStore<S>,
    ) -> Result<(), Error> {
        if self.pin_limit - self.pins.len() < sequence.len {
            return Err(Error::Capacity);
        }
        for span in &mut sequence.spans[..sequence.len] {
            if let ChunkRef::Persistent(reference) = span.chunk {
                let owned = resources.retain_audio_chunk(reference)?;
                self.pins.push(Some(owned));
                span.chunk = ChunkRef::Persistent(owned);
            }
        }
        Ok(())
    }
    pub fn unpin(&mut self, resources: &mut ResourceStore<S>) -> Result<(), Error> {
        while let Some(reference) = self.pins.pop() {
            resources.release_audio_chunk(reference.ok_or(Error::InvalidState)?)?;
        }
        Ok(())
    }
    pub fn read_into(
        &mut self,
        sequence: AudioSequence<S>,
        output: &mut [S],
        resources: &ResourceStore<S>,
        add: bool,
    ) -> Result<(), Error> {
        let frames = output.len();
        if !add {
            output.fill(S::ZERO);
        }
        visit(
            sequence,
            frames,
            self.epoch,
            &self.data[..self.used],
            None,
            &[],
            resources,
            |offset, sample| {
                output[offset] += sample;
            },
        )?;
        self.work.sample_reads += sequence.source_frames();
        self.work.sample_writes += sequence.source_frames() + if add { 0 } else { frames as u64 };
        Ok(())
    }
    pub fn accumulate(
        &mut self,
        sequence: AudioSequence<S>,
        offset: usize,
        frames: usize,
        resources: &ResourceStore<S>,
    ) -> Result<(), Error> {
        let (before, target) = self.data[..self.used].split_at_mut(offset);
        let (target, after) = target.split_at_mut(frames);
        visit(
            sequence,
            frames,
            self.epoch,
            before,
            Some((offset, frames)),
            after,
            resources,
            |index, sample| target[index] += sample,
        )?;
        self.work.sample_reads += sequence.source_frames();
        self.work.sample_writes += sequence.source_frames();
        Ok(())
    }
    pub fn peak(
        &mut self,
        sequence: AudioSequence<S>,
        frames: usize,
        resources: &ResourceStore<S>,
    ) -> Result<S, Error> {
        let mut peak = 0u32;
        visit(
            sequence,
            frames,
            self.epoch,
            &self.data[..self.used],
            None,
            &[],
            resources,
            |_, sample| peak = peak.max(sample.peak_key()),
        )?;
        self.work.sample_reads += sequence.source_frames();
        Ok(S::from_bits(peak))
    }
    pub fn last(
        &self,
        sequence: AudioSequence<S>,
        resources: &ResourceStore<S>,
    ) -> Result<S, Error> {
        if sequence.silent() {
            return Ok(S::ZERO);
        }
        let span = sequence.spans[sequence.len - 1];
        let sample = match span.chunk {
            ChunkRef::Silence => S::ZERO,
            ChunkRef::Transient { offset, epoch } => {
                if epoch != self.epoch {
                    return Err(Error::InvalidState);
                }
                *self.data[..self.used]
                    .get(offset + span.frames - 1)
                    .ok_or(Error::InvalidOffset)?
            }
            ChunkRef::Persistent(reference) => {
                let mut value = S::ZERO;
                resources.visit_audio_chunk(reference, span.frames, |samples, first, stride| {
                    value = samples[first + (span.frames - 1) * stride]
                })?;
                value
            }
        };
        let frames: usize = sequence.spans[..sequence.len]
            .iter()
            .map(|span| span.frames)
            .sum();
        Ok(sample * sequence.gain.at(frames - 1, frames))
    }
}

fn visit<S: Sample>(
    mut sequence: AudioSequence<S>,
    frames: usize,
    epoch: u64,
    before: &[S],
    excluded: Option<(usize, usize)>,
    after: &[S],
    resources: &ResourceStore<S>,
    mut consume: impl FnMut(usize, S),
) -> Result<(), Error> {
    if sequence.silent() {
        return Ok(());
    }
    let mut at = 0;
    for span in &mut sequence.spans[..sequence.len] {
        let count = span.frames;
        if count > frames - at {
            return Err(Error::InvalidShape);
        }
        match span.chunk {
            ChunkRef::Silence => {}
            ChunkRef::Transient {
                offset,
                epoch: origin,
            } => {
                if origin != epoch {
                    return Err(Error::InvalidState);
                }
                let samples = match excluded {
                    Some((first, size)) if offset >= first + size => {
                        after.get(offset - first - size..offset - first - size + count)
                    }
                    Some((first, _)) if offset + count > first => None,
                    _ => before.get(offset..offset + count),
                }
                .ok_or(Error::InvalidState)?;
                for (i, &sample) in samples.iter().enumerate() {
                    let gain = sequence.gain.at(at + i, frames);
                    consume(at + i, sample * gain);
                }
            }
            ChunkRef::Persistent(reference) => {
                resources.visit_audio_chunk(reference, count, |samples, first, stride| {
                    for i in 0..count {
                        let gain = sequence.gain.at(at + i, frames);
                        consume(at + i, samples[first + i * stride] * gain);
                    }
                })?;
            }
        }
        at += count;
    }
    if at != frames {
        return Err(Error::InvalidShape);
    }
    Ok(())
}

pub struct ReferenceBlock<'a, L, S: Sample = f32> {
    pub node: NodeId,
    pub frame: Frame,
    pub graph_epoch: crate::Epoch,
    pub quantum: Quantum,
    pub inputs: &'a [AudioSequence<S>],
    pub outputs: &'a mut [AudioSequence<S>],
    pub arena: &'a mut RenderArena<S>,
    pub resources: &'a mut ResourceStore<S>,
    pub domain: &'a mut L,
}

impl<L, S: Sample> ReferenceBlock<'_, L, S> {
    pub fn forward(
        &mut self,
        input: usize,
        output: usize,
        gain: GainEnvelope<S>,
    ) -> Result<(), Error> {
        *self.outputs.get_mut(output).ok_or(Error::InvalidShape)? = self
            .inputs
            .get(input)
            .copied()
            .ok_or(Error::InvalidShape)?
            .scaled(gain);
        self.arena.work.forwarded += 1;
        Ok(())
    }
    pub fn peak(&mut self, sequence: AudioSequence<S>) -> Result<S, Error> {
        self.arena
            .peak(sequence, self.quantum.frames(), self.resources)
    }
    pub fn playback(
        &mut self,
        index: ReadIndex,
        position: u64,
        channel: usize,
    ) -> Result<Option<AudioSequence<S>>, Error> {
        let mut sequence =
            self.resources
                .audio_chunk_sequence(index, position, self.quantum.frames(), channel)?;
        if let Some(sequence) = &mut sequence {
            self.arena.pin_sequence(sequence, self.resources)?;
        }
        Ok(sequence)
    }
    pub fn materialize(&mut self, output: usize, source: AudioSequence<S>) -> Result<(), Error> {
        let count = self.quantum.frames();
        let offset = self.arena.allocate(count)?;
        self.arena
            .accumulate(source, offset, count, self.resources)?;
        *self.outputs.get_mut(output).ok_or(Error::InvalidShape)? =
            self.arena.transient(offset, count);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn composed_endpoints_survive_split_spans_and_independent_source_mixing() {
        let resources = ResourceStore::new(
            crate::Config::default().prepare().unwrap(),
            Quantum::new(32).unwrap(),
        )
        .unwrap();
        let mut arena = RenderArena::prepare(128, 0);
        assert_no_heap(|| {
            let first = arena.allocate(32).unwrap();
            arena.output(first, 32).fill(2.);
            let second = arena.allocate(32).unwrap();
            arena.output(second, 32).fill(3.);
            let envelopes = [
                GainEnvelope {
                    start: 0.1,
                    end: 0.8,
                },
                GainEnvelope {
                    start: 0.7,
                    end: 0.2,
                },
                GainEnvelope::constant(0.6),
            ];
            let gain = envelopes
                .into_iter()
                .fold(GainEnvelope::default(), GainEnvelope::compose);
            let sequence = AudioSequence {
                spans: [
                    ChunkSpan {
                        chunk: ChunkRef::Transient {
                            offset: first,
                            epoch: arena.epoch,
                        },
                        frames: 11,
                    },
                    ChunkSpan {
                        chunk: ChunkRef::Transient {
                            offset: first + 11,
                            epoch: arena.epoch,
                        },
                        frames: 21,
                    },
                ],
                len: 2,
                gain,
            };
            let target = arena.allocate(32).unwrap();
            arena.accumulate(sequence, target, 32, &resources).unwrap();
            arena
                .accumulate(arena.transient(second, 32), target, 32, &resources)
                .unwrap();
            let samples = arena.output(target, 32);
            for (i, &sample) in samples.iter().enumerate() {
                assert!((sample - (3. + 2. * gain.at(i, 32))).abs() < 1e-6);
            }
            for i in [0, 31] {
                let ideal = envelopes
                    .into_iter()
                    .map(|envelope| envelope.at(i, 32))
                    .product::<f32>();
                assert!((samples[i] - (3. + 2. * ideal)).abs() < 1e-6);
            }
            let single = gain.at(0, 1);
            assert_eq!(single, gain.start);
            assert_eq!(arena.allocate(33), Err(Error::Capacity));
            assert_eq!(arena.allocate(32), Ok(96));
            arena.reset().unwrap();
            let first = arena.allocate(32).unwrap();
            let mut output = [0.; 32];
            assert_eq!(
                arena.read_into(sequence, &mut output, &resources, false),
                Err(Error::InvalidState)
            );
            assert_eq!(first, 0);
            assert_eq!(arena.output(first, 32), [0.; 32]);
        });
    }
}
