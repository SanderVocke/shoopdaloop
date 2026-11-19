//! Prepared audio handoff for a native Carla instance owned by the worker.

use crate::native::{CarlaNativeHost, CARLA_MAX_BUFFER_SIZE, CARLA_MIDI_BUFFER_CAPACITY};
use rt_buffered::{
    AsyncTransport, Backend, Batch, BufferLimits, LocalTransport, LocalWorker, PreparedTiming,
};
use rt_runtime::{graph::PortCounts, Error};
use shoop_plugin_protocol::CarlaChainType;

pub struct RenderEndpoint {
    transport: LocalTransport,
    ports: PortCounts,
}

pub struct Worker {
    transport: LocalWorker,
    processor: CarlaNativeHost,
    enabled: bool,
}

fn ports(chain_type: CarlaChainType) -> PortCounts {
    let channels = usize::from(chain_type.audio_channels());
    PortCounts {
        audio_inputs: channels,
        audio_outputs: channels,
        midi_inputs: 1,
        midi_outputs: 1,
    }
}

impl Worker {
    pub fn transport_bytes(
        chain_type: CarlaChainType,
        timing: PreparedTiming,
        limits: BufferLimits,
    ) -> Result<usize, Error> {
        if timing.batch_frames() > CARLA_MAX_BUFFER_SIZE {
            return Err(Error::InvalidConfig);
        }
        LocalTransport::prepared_bytes(ports(chain_type), timing, limits, timing.slots())
    }

    pub fn prepare(
        processor: CarlaNativeHost,
        timing: PreparedTiming,
        limits: BufferLimits,
    ) -> Result<(RenderEndpoint, Self), Error> {
        Self::transport_bytes(processor.chain_type(), timing, limits)?;
        let ports = ports(processor.chain_type());
        let (transport, worker) = LocalTransport::prepare(ports, timing, limits, timing.slots())?;
        Ok((
            RenderEndpoint { transport, ports },
            Self {
                transport: worker,
                processor,
                enabled: true,
            },
        ))
    }

    /// Called by the service on the same non-render owner that pumps batches.
    pub fn processor(&mut self) -> &mut CarlaNativeHost {
        &mut self.processor
    }

    pub fn set_active(&mut self, active: bool) {
        self.enabled = active;
        self.processor.set_active(active);
    }

    pub fn process_next(&mut self) -> Result<bool, Error> {
        let processor = &mut self.processor;
        let enabled = self.enabled;
        self.transport.process_next(|discontinuous, batch| {
            if enabled {
                process(processor, discontinuous, batch)
            } else {
                Err(Error::InvalidState)
            }
        })
    }
}

impl Backend for RenderEndpoint {
    fn ports(&self) -> PortCounts {
        self.ports
    }

    fn async_transport(&mut self) -> Option<&mut dyn AsyncTransport> {
        Some(&mut self.transport)
    }

    fn process(&mut self, _: &mut Batch<'_, '_>) -> Result<(), Error> {
        Err(Error::InvalidState)
    }

    fn suspend(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

fn process(
    processor: &mut CarlaNativeHost,
    discontinuous: bool,
    batch: &mut Batch<'_, '_>,
) -> Result<(), Error> {
    if !batch.parameters.is_empty() {
        return Err(Error::InvalidParameter);
    }
    let midi = batch.midi_input(0).ok_or(Error::InvalidShape)?;
    let reset = discontinuous && processor.is_active();
    let reset_events = if reset { 16 } else { 0 };
    if midi.len() > CARLA_MIDI_BUFFER_CAPACITY - reset_events {
        return Err(Error::Capacity);
    }
    if midi.iter().any(|(offset, bytes)| {
        offset as usize >= batch.frames || bytes.is_empty() || bytes.len() > 4
    }) {
        return Err(Error::InvalidEvent);
    }
    let recovery: [[u8; 3]; 16] = std::array::from_fn(|channel| [0xb0 | channel as u8, 120, 0]);
    processor
        .load_midi_input(
            recovery[..reset_events]
                .iter()
                .map(|packet| (0, packet.as_slice()))
                .chain(midi.iter()),
        )
        .map_err(|_| Error::InvalidEvent)?;
    if reset {
        processor.set_active(false);
    }
    processor.set_active(true);
    let channels = usize::from(processor.chain_type().audio_channels());
    for channel in 0..channels {
        let input = batch.audio.input(channel).ok_or(Error::InvalidShape)?;
        let target = processor
            .audio_input_mut(channel)
            .ok_or(Error::InvalidShape)?;
        target[..batch.frames].copy_from_slice(input);
    }
    processor
        .process(batch.frames)
        .map_err(|_| Error::InvalidState)?;
    for channel in 0..channels {
        let source = processor.audio_output(channel).ok_or(Error::InvalidShape)?;
        let target = batch.audio.output(channel).ok_or(Error::InvalidShape)?;
        target.copy_from_slice(&source[..batch.frames]);
    }
    let output = batch.midi_outputs.first_mut().ok_or(Error::InvalidShape)?;
    for (offset, packet) in processor.midi_packets() {
        output.push(offset, batch.frames, packet)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt_buffered::{AsyncBatch, BatchTag, MidiLimits, Timing};
    use rt_runtime::{buffer::MidiBuffer, Frame, Quantum};
    use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

    #[shoop_test]
    fn native_worker_releases_real_rack_output_and_refuses_long_midi() {
        let quantum = Quantum::new(16).unwrap();
        let timing = Timing {
            batch_quanta: 2,
            slack_quanta: 1,
        }
        .prepare(quantum)
        .unwrap();
        let processor = match CarlaNativeHost::instantiate(
            CarlaChainType::Rack,
            48_000,
            timing.batch_frames() as u32,
        ) {
            Ok(processor) => processor,
            Err(error) if std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none() => {
                eprintln!("skipping buffered Native runtime test: {error:#}");
                return;
            }
            Err(error) => panic!("required Carla runtime is unavailable: {error:#}"),
        };
        let midi = MidiLimits {
            events_per_quantum: 32,
            bytes_per_quantum: 128,
        };
        let limits = BufferLimits {
            midi_input: midi,
            midi_output: midi,
            parameters_per_quantum: 0,
        };
        let (mut endpoint, mut worker) = Worker::prepare(processor, timing, limits).unwrap();
        let transport = endpoint.async_transport().unwrap();
        let frames = timing.batch_frames();
        let mut audio = [0.25; 64];
        audio[frames..].fill(-0.5);
        let mut midi_input = [MidiBuffer::new(32, 128).unwrap()];
        let note = [0x90, 60, 100];
        let sysex = [0xf0, 0x7d, 1, 2, 0xf7];
        for (sequence, packet) in [&note[..], &sysex[..], &note[..]].into_iter().enumerate() {
            midi_input[0].clear();
            midi_input[0].push(7, frames, packet).unwrap();
            let tag = BatchTag {
                generation: 1,
                sequence: sequence as u64,
                source: Frame((sequence * frames) as u64),
            };
            assert_no_heap(|| {
                transport.try_submit(AsyncBatch {
                    tag,
                    frames,
                    audio: &audio,
                    midi: &midi_input,
                    parameters: &[],
                })
            })
            .unwrap();
            assert!(worker.process_next().unwrap());
            let completed = transport.try_completion().unwrap();
            assert_eq!(completed.tag, tag);
            if sequence == 1 {
                assert_eq!(completed.output.err(), Some(Error::InvalidEvent));
                continue;
            }
            let output = completed.output.unwrap();
            assert_eq!(output.audio, &audio);
            let midi = &output.midi[0];
            let recovery = if sequence == 2 { 16 } else { 0 };
            assert_eq!(midi.len(), recovery + 1);
            for channel in 0..recovery {
                assert_eq!(
                    midi.get(channel),
                    Some((0, &[0xb0 | channel as u8, 120, 0][..]))
                );
            }
            assert_eq!(midi.get(midi.len() - 1), Some((7, &note[..])));
        }
    }
}
