//! Prepared audio handoff for a native Carla instance owned by the worker.

use crate::{native::CARLA_MAX_BUFFER_SIZE, processor::CarlaProcessor};
use rt_buffered::{
    AsyncTransport, Backend, Batch, BufferLimits, LocalTransport, LocalWorker, PreparedTiming,
};
use rt_runtime::{graph::PortCounts, Error};
use shoop_plugin_protocol::{
    CarlaChainType, CarlaProcessorLifecycle, MidiEvent, PreparedMidiBuffer,
    MAX_MIDI_EVENTS_PER_BLOCK, MAX_NATIVE_MIDI_PACKET_BYTES,
};
use std::mem::size_of;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

const OWNER_READY: u8 = 0;
const OWNER_RESET_REQUESTED: u8 = 1;
const OWNER_RESET_FAILED: u8 = 2;

pub struct RenderEndpoint {
    transport: LocalTransport,
    ports: PortCounts,
    owner_reset: Arc<AtomicU8>,
}

pub struct Worker {
    transport: LocalWorker,
    processor: Box<dyn CarlaProcessor>,
    midi_output: PreparedMidiBuffer,
    enabled: bool,
    owner_reset: Arc<AtomicU8>,
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
        let transfer =
            LocalTransport::prepared_bytes(ports(chain_type), timing, limits, timing.slots())?;
        let output = size_of::<PreparedMidiBuffer>()
            + MAX_MIDI_EVENTS_PER_BLOCK * (size_of::<MidiEvent>() + MAX_NATIVE_MIDI_PACKET_BYTES);
        transfer.checked_add(output).ok_or(Error::Capacity)
    }

    pub fn prepare(
        processor: impl CarlaProcessor + 'static,
        timing: PreparedTiming,
        limits: BufferLimits,
    ) -> Result<(RenderEndpoint, Self), Error> {
        Self::prepare_boxed(Box::new(processor), timing, limits)
    }

    pub(super) fn prepare_boxed(
        processor: Box<dyn CarlaProcessor>,
        timing: PreparedTiming,
        limits: BufferLimits,
    ) -> Result<(RenderEndpoint, Self), Error> {
        let info = processor.info();
        Self::transport_bytes(info.chain_type, timing, limits)?;
        let ports = ports(info.chain_type);
        if (
            info.audio_inputs,
            info.audio_outputs,
            info.midi_inputs,
            info.midi_outputs,
        ) != (
            ports.audio_inputs,
            ports.audio_outputs,
            ports.midi_inputs,
            ports.midi_outputs,
        ) {
            return Err(Error::InvalidShape);
        }
        let (transport, worker) = LocalTransport::prepare(ports, timing, limits, timing.slots())?;
        let owner_reset = Arc::new(AtomicU8::new(OWNER_READY));
        Ok((
            RenderEndpoint {
                transport,
                ports,
                owner_reset: Arc::clone(&owner_reset),
            },
            Self {
                transport: worker,
                processor,
                midi_output: PreparedMidiBuffer::new(
                    MAX_MIDI_EVENTS_PER_BLOCK,
                    MAX_NATIVE_MIDI_PACKET_BYTES,
                ),
                enabled: true,
                owner_reset,
            },
        ))
    }

    /// Called by the service on the same non-render owner that pumps batches.
    pub fn processor(&mut self) -> &mut dyn CarlaProcessor {
        self.processor.as_mut()
    }

    pub(super) fn owner_reset_requested(&self) -> bool {
        self.owner_reset.load(Ordering::Acquire) == OWNER_RESET_REQUESTED
    }

    pub(super) fn discard_submitted_batches(&mut self) -> Result<(), Error> {
        self.transport.discard_pending()
    }

    pub(super) fn replace_retired_owner(&mut self, replacement: Option<Box<dyn CarlaProcessor>>) {
        let status = if let Some(processor) = replacement {
            self.processor = processor;
            self.enabled = true;
            OWNER_READY
        } else {
            self.enabled = false;
            OWNER_RESET_FAILED
        };
        self.midi_output.clear();
        self.owner_reset.store(status, Ordering::Release);
    }

    pub fn set_active(&mut self, active: bool) -> Result<(), Error> {
        self.processor.set_active(active);
        if !self.processor.is_ready() || self.processor.is_active() != active {
            return Err(Error::Disconnected);
        }
        self.enabled = active;
        Ok(())
    }

    pub fn process_next(&mut self) -> Result<bool, Error> {
        let processor = &mut self.processor;
        let midi_output = &mut self.midi_output;
        let enabled = self.enabled;
        self.transport.process_next(|discontinuous, batch| {
            if enabled {
                process(processor.as_mut(), midi_output, discontinuous, batch)
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

    fn reset_step(&mut self, step: usize) -> Result<bool, Error> {
        if step == 0 {
            self.owner_reset
                .store(OWNER_RESET_REQUESTED, Ordering::Release);
            return Ok(false);
        }
        match self.owner_reset.load(Ordering::Acquire) {
            OWNER_READY => Ok(true),
            OWNER_RESET_REQUESTED => Ok(false),
            _ => Err(Error::Disconnected),
        }
    }
}

fn process(
    processor: &mut dyn CarlaProcessor,
    midi_output: &mut PreparedMidiBuffer,
    discontinuous: bool,
    batch: &mut Batch<'_, '_>,
) -> Result<(), Error> {
    if !batch.parameters.is_empty() {
        return Err(Error::InvalidParameter);
    }
    let midi = batch.midi_input(0).ok_or(Error::InvalidShape)?;
    let reset = discontinuous && processor.is_active();
    let reset_events = if reset { 16 } else { 0 };
    if midi.len() > MAX_MIDI_EVENTS_PER_BLOCK - reset_events {
        return Err(Error::Capacity);
    }
    if midi.iter().any(|(offset, bytes)| {
        offset as usize >= batch.frames
            || bytes.is_empty()
            || bytes.len() > MAX_NATIVE_MIDI_PACKET_BYTES
    }) {
        return Err(Error::InvalidEvent);
    }
    let recovery: [[u8; 3]; 16] = std::array::from_fn(|channel| [0xb0 | channel as u8, 120, 0]);
    let mut messages = [(0, &[][..]); MAX_MIDI_EVENTS_PER_BLOCK];
    let events = recovery[..reset_events]
        .iter()
        .map(|packet| (0, packet.as_slice()))
        .chain(midi.iter());
    for (slot, packet) in messages.iter_mut().zip(events) {
        *slot = packet;
    }
    processor
        .set_midi_input_events(0, &messages[..reset_events + midi.len()])
        .map_err(|_| Error::InvalidEvent)?;
    if reset {
        processor.set_active(false);
    }
    if !processor.is_active() {
        processor.set_active(true);
    }
    if !processor.is_active() {
        return Err(Error::Disconnected);
    }
    let channels = processor.info().audio_inputs;
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
    if processor.lifecycle() == CarlaProcessorLifecycle::Degraded {
        return Err(Error::Incomplete);
    }
    for channel in 0..channels {
        let source = processor.audio_output(channel).ok_or(Error::InvalidShape)?;
        let target = batch.audio.output(channel).ok_or(Error::InvalidShape)?;
        target.copy_from_slice(&source[..batch.frames]);
    }
    let output = batch.midi_outputs.first_mut().ok_or(Error::InvalidShape)?;
    midi_output.clear();
    processor
        .fill_midi_output_events(0, midi_output)
        .map_err(|_| Error::InvalidEvent)?;
    for event in midi_output.as_slice() {
        output.push(event.frame_offset, batch.frames, &event.data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::CarlaNativeHost;
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
