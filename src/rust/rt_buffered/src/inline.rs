use alloc::{boxed::Box, vec};
use rt_runtime::{buffer::MidiBuffer, Error, Frame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Job {
    pub generation: u64,
    pub sequence: u64,
    pub source: Frame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Completion {
    pub job: Job,
}

/// Inline development backend: stereo 2x gain and next-channel MIDI rotation.
pub struct InlineTestProcessor {
    scratch: Box<[u8]>,
}

impl crate::Backend for InlineTestProcessor {
    fn ports(&self) -> rt_runtime::graph::PortCounts {
        rt_runtime::graph::PortCounts {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }

    fn process(&mut self, batch: &mut crate::Batch<'_, '_>) -> Result<(), Error> {
        if batch.audio.input_count() != 2 || batch.audio.output_count() != 2 {
            return Err(Error::InvalidShape);
        }
        for channel in 0..2 {
            for frame in 0..batch.frames {
                let value = batch.audio.input(channel).ok_or(Error::InvalidShape)?[frame];
                batch.audio.output(channel).ok_or(Error::InvalidShape)?[frame] = value * 2.;
            }
        }
        // The helper also checks and transforms MIDI; its audio slice may be empty.
        self.submit(
            Job {
                generation: batch.tag.generation,
                sequence: batch.tag.sequence,
                source: batch.tag.source,
            },
            batch.frames,
            &mut [],
            batch.midi_inputs.first().ok_or(Error::InvalidShape)?,
            batch.midi_outputs.first_mut().ok_or(Error::InvalidShape)?,
        )?;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

impl InlineTestProcessor {
    pub(crate) fn new(max_packet_bytes: usize) -> Self {
        let mut scratch = vec![0; max_packet_bytes].into_boxed_slice();
        core::hint::black_box(&mut *scratch).fill(0);
        Self { scratch }
    }

    pub(crate) fn submit(
        &mut self,
        job: Job,
        frames: usize,
        audio: &mut [f32],
        input: &MidiBuffer,
        output: &mut MidiBuffer,
    ) -> Result<Completion, Error> {
        if !audio.is_empty() && frames.checked_mul(2) != Some(audio.len()) {
            return Err(Error::InvalidShape);
        }
        if input.len() > output.remaining_events() || input.byte_len() > output.remaining_bytes() {
            return Err(Error::Capacity);
        }
        for i in 0..input.len() {
            if input.get(i).ok_or(Error::InvalidState)?.1.len() > self.scratch.len() {
                return Err(Error::Capacity);
            }
        }
        for sample in audio {
            *sample *= 2.;
        }
        for i in 0..input.len() {
            let (offset, packet) = input.get(i).ok_or(Error::InvalidState)?;
            let bytes = &mut self.scratch[..packet.len()];
            bytes.copy_from_slice(packet);
            if bytes[0] < 0xf0 {
                bytes[0] = (bytes[0] & 0xf0) | ((bytes[0] + 1) & 0x0f);
            }
            output.push(offset, frames, bytes).map_err(|e| {
                if e == Error::Full {
                    Error::Capacity
                } else {
                    e
                }
            })?;
        }
        Ok(Completion { job })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    fn job() -> Job {
        Job {
            generation: 3,
            sequence: 7,
            source: Frame(64),
        }
    }

    #[shoop_test]
    fn buffered_inline_audio_doubles_signed_stereo_without_clipping() {
        let mut backend = InlineTestProcessor::new(0);
        let input = MidiBuffer::new(0, 0).unwrap();
        let mut output = MidiBuffer::new(0, 0).unwrap();
        let mut audio = [0., 0.25, -0.5, 2., -2., 1., 0.5, -0.25];
        assert_eq!(
            backend.submit(job(), 4, &mut audio, &input, &mut output),
            Ok(Completion { job: job() })
        );
        assert_eq!(audio, [0., 0.5, -1., 4., -4., 2., 1., -0.5]);
        assert!(output.is_empty());
    }

    #[shoop_test]
    fn buffered_inline_midi_rotates_voice_packets_and_preserves_system_packets() {
        let packets: &[(&[u8], &[u8])] = &[
            (&[0x90, 60, 100], &[0x91, 60, 100]),
            (&[0x80, 60, 40], &[0x81, 60, 40]),
            (&[0x90, 61, 0], &[0x91, 61, 0]),
            (&[0x9f, 127, 1], &[0x90, 127, 1]),
            (&[0xa4, 60, 5], &[0xa5, 60, 5]),
            (&[0xbf, 7, 100], &[0xb0, 7, 100]),
            (&[0xc2, 3], &[0xc3, 3]),
            (&[0xd8, 4], &[0xd9, 4]),
            (&[0xef, 0, 64], &[0xe0, 0, 64]),
            (&[0xf8], &[0xf8]),
            (&[0xf2, 1, 2], &[0xf2, 1, 2]),
            (&[0xf0, 1, 2, 3, 4, 0xf7], &[0xf0, 1, 2, 3, 4, 0xf7]),
        ];
        let mut backend = InlineTestProcessor::new(6);
        let mut input = MidiBuffer::new(packets.len(), 64).unwrap();
        let mut output = MidiBuffer::new(packets.len(), 64).unwrap();
        for &(packet, _) in packets {
            input.push(3, 16, packet).unwrap();
        }
        let mut audio = [1.; 32];
        backend
            .submit(job(), 16, &mut audio, &input, &mut output)
            .unwrap();
        assert_eq!(output.len(), packets.len());
        assert_eq!(output.byte_len(), input.byte_len());
        for (i, &(_, expected)) in packets.iter().enumerate() {
            assert_eq!(output.get(i), Some((3, expected)));
        }
        let mut full = MidiBuffer::new(0, 0).unwrap();
        assert_eq!(
            backend.submit(job(), 16, &mut audio, &input, &mut full),
            Err(Error::Capacity)
        );
        assert_eq!(audio, [2.; 32]);
        assert!(full.is_empty());
    }
}
