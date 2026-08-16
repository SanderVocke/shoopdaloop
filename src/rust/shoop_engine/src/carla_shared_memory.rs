use anyhow::{anyhow, Context, Result};
use memmap2::{MmapMut, MmapOptions};
use shoop_plugin_protocol::{
    BlockSequence, MidiEvent, ProcessGeneration, MAX_AUDIO_CHANNELS, MAX_BLOCK_FRAMES,
    MAX_MIDI_BYTES_PER_BLOCK, MAX_MIDI_EVENTS_PER_BLOCK, PROTOCOL_VERSION,
};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;
use tempfile::NamedTempFile;

pub const SHARED_SLOT_COUNT: usize = 3;
const FILE_HEADER_SIZE: usize = 4096;
const SLOT_HEADER_SIZE: usize = 64;
const AUDIO_PLANE_BYTES: usize = MAX_BLOCK_FRAMES * std::mem::size_of::<f32>();
const AUDIO_REGION_BYTES: usize = MAX_AUDIO_CHANNELS * AUDIO_PLANE_BYTES;
const AUDIO_INPUT_OFFSET: usize = SLOT_HEADER_SIZE;
const AUDIO_OUTPUT_OFFSET: usize = AUDIO_INPUT_OFFSET + AUDIO_REGION_BYTES;
const MIDI_INPUT_OFFSET: usize = AUDIO_OUTPUT_OFFSET + AUDIO_REGION_BYTES;
const MIDI_OUTPUT_OFFSET: usize = MIDI_INPUT_OFFSET + MAX_MIDI_BYTES_PER_BLOCK;
const SLOT_CONTENT_BYTES: usize = MIDI_OUTPUT_OFFSET + MAX_MIDI_BYTES_PER_BLOCK;
const SLOT_SIZE: usize = SLOT_CONTENT_BYTES.div_ceil(4096) * 4096;
const FILE_SIZE: usize = FILE_HEADER_SIZE + SHARED_SLOT_COUNT * SLOT_SIZE;
const MAGIC: &[u8; 8] = b"SHOOPFX\0";

const STATE_FREE: u32 = 0;
const STATE_READY: u32 = 1;
const STATE_DONE: u32 = 2;
const STATE_ABANDONED: u32 = 3;
const STATE_PROCESSING: u32 = 4;
const STATE_PARENT_WRITING: u32 = 5;

const STATE_OFFSET: usize = 0;
const SEQUENCE_OFFSET: usize = 8;
const GENERATION_OFFSET: usize = 16;
const FRAMES_OFFSET: usize = 24;
const AUDIO_INPUTS_OFFSET: usize = 28;
const AUDIO_OUTPUTS_OFFSET: usize = 32;
const MIDI_INPUT_BYTES_OFFSET: usize = 36;
const MIDI_OUTPUT_BYTES_OFFSET: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedBlockToken {
    pub slot: usize,
    pub sequence: BlockSequence,
    pub generation: ProcessGeneration,
    pub frames: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedBlockError {
    NoFreeSlot,
    DeadlineMiss,
    InvalidCapacity,
    MidiOverflow,
    StaleCompletion,
}

impl std::fmt::Display for SharedBlockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SharedBlockError {}

pub struct SharedBlockTransport {
    mapping: MmapMut,
    _file: Option<File>,
    _temporary_file: Option<NamedTempFile>,
    path: PathBuf,
    generation: ProcessGeneration,
    next_slot: usize,
}

unsafe impl Send for SharedBlockTransport {}

impl std::fmt::Debug for SharedBlockTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedBlockTransport")
            .field("path", &self.path)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl SharedBlockTransport {
    pub fn create(generation: ProcessGeneration, nonce: &[u8; 32]) -> Result<SharedBlockTransport> {
        Self::create_in(std::env::temp_dir(), generation, nonce)
    }

    pub fn create_in(
        directory: impl AsRef<Path>,
        generation: ProcessGeneration,
        nonce: &[u8; 32],
    ) -> Result<SharedBlockTransport> {
        let file = tempfile::Builder::new()
            .prefix("shoop-carla-")
            .suffix(".ipc")
            .tempfile_in(directory)
            .context("could not create Carla shared-memory file")?;
        file.as_file().set_len(FILE_SIZE as u64)?;
        let mut mapping = unsafe { MmapOptions::new().len(FILE_SIZE).map_mut(file.as_file())? };
        mapping.fill(0);
        mapping[..8].copy_from_slice(MAGIC);
        write_u32(&mut mapping, 8, PROTOCOL_VERSION);
        write_u32(&mut mapping, 12, SHARED_SLOT_COUNT as u32);
        write_u64(&mut mapping, 16, generation.0);
        mapping[24..56].copy_from_slice(nonce);
        for slot in 0..SHARED_SLOT_COUNT {
            unsafe {
                std::ptr::write(
                    mapping.as_mut_ptr().add(slot_offset(slot) + STATE_OFFSET) as *mut AtomicU32,
                    AtomicU32::new(STATE_FREE),
                );
                std::ptr::write(
                    mapping
                        .as_mut_ptr()
                        .add(slot_offset(slot) + SEQUENCE_OFFSET)
                        as *mut AtomicU64,
                    AtomicU64::new(0),
                );
                std::ptr::write(
                    mapping
                        .as_mut_ptr()
                        .add(slot_offset(slot) + GENERATION_OFFSET)
                        as *mut AtomicU64,
                    AtomicU64::new(generation.0),
                );
            }
        }
        mapping.flush()?;
        Ok(Self {
            path: file.path().to_path_buf(),
            mapping,
            _file: None,
            _temporary_file: Some(file),
            generation,
            next_slot: 0,
        })
    }

    pub fn open(
        path: &Path,
        expected_generation: ProcessGeneration,
        expected_nonce: &[u8; 32],
    ) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let metadata = file.metadata()?;
        if metadata.len() != FILE_SIZE as u64 {
            return Err(anyhow!("Carla shared-memory layout size does not match"));
        }
        let mapping = unsafe { MmapOptions::new().len(FILE_SIZE).map_mut(&file)? };
        if &mapping[..8] != MAGIC {
            return Err(anyhow!("Carla shared-memory magic does not match"));
        }
        if read_u32(&mapping, 8) != PROTOCOL_VERSION {
            return Err(anyhow!(
                "Carla shared-memory protocol version does not match"
            ));
        }
        if read_u32(&mapping, 12) != SHARED_SLOT_COUNT as u32 {
            return Err(anyhow!("Carla shared-memory slot count does not match"));
        }
        if read_u64(&mapping, 16) != expected_generation.0 {
            return Err(anyhow!("Carla shared-memory generation does not match"));
        }
        if &mapping[24..56] != expected_nonce {
            return Err(anyhow!("Carla shared-memory nonce does not match"));
        }
        Ok(Self {
            mapping,
            _file: Some(file),
            _temporary_file: None,
            path: path.to_path_buf(),
            generation: expected_generation,
            next_slot: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn generation(&self) -> ProcessGeneration {
        self.generation
    }

    pub fn occupied_slots(&self) -> usize {
        (0..SHARED_SLOT_COUNT)
            .filter(|slot| self.state(*slot).load(Ordering::Relaxed) != STATE_FREE)
            .count()
    }

    pub fn submit(
        &mut self,
        sequence: BlockSequence,
        frames: usize,
        audio_inputs: &[Vec<f32>],
        audio_output_channels: usize,
        midi_inputs: &[Vec<(u32, Vec<u8>)>],
        midi_input_counts: &[usize],
    ) -> std::result::Result<SharedBlockToken, SharedBlockError> {
        if sequence.0 == 0
            || frames == 0
            || frames > MAX_BLOCK_FRAMES
            || audio_inputs.len() > MAX_AUDIO_CHANNELS
            || audio_output_channels > MAX_AUDIO_CHANNELS
            || audio_inputs.iter().any(|input| input.len() < frames)
        {
            return Err(SharedBlockError::InvalidCapacity);
        }
        let slot = (0..SHARED_SLOT_COUNT)
            .map(|offset| (self.next_slot + offset) % SHARED_SLOT_COUNT)
            .find(|slot| {
                self.state(*slot)
                    .compare_exchange(
                        STATE_FREE,
                        STATE_PARENT_WRITING,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
            })
            .ok_or(SharedBlockError::NoFreeSlot)?;
        self.next_slot = (slot + 1) % SHARED_SLOT_COUNT;
        let base = slot_offset(slot);
        self.sequence(slot).store(sequence.0, Ordering::Relaxed);
        self.slot_generation(slot)
            .store(self.generation.0, Ordering::Relaxed);
        write_u32(&mut self.mapping, base + FRAMES_OFFSET, frames as u32);
        write_u32(
            &mut self.mapping,
            base + AUDIO_INPUTS_OFFSET,
            audio_inputs.len() as u32,
        );
        write_u32(
            &mut self.mapping,
            base + AUDIO_OUTPUTS_OFFSET,
            audio_output_channels as u32,
        );
        for (channel, input) in audio_inputs.iter().enumerate() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    input.as_ptr().cast::<u8>(),
                    self.mapping
                        .as_mut_ptr()
                        .add(base + AUDIO_INPUT_OFFSET + channel * AUDIO_PLANE_BYTES),
                    frames * std::mem::size_of::<f32>(),
                );
            }
        }
        let midi_bytes = write_midi(
            &mut self.mapping[base + MIDI_INPUT_OFFSET..base + MIDI_OUTPUT_OFFSET],
            midi_inputs
                .iter()
                .enumerate()
                .flat_map(|(channel, events)| {
                    events[..midi_input_counts
                        .get(channel)
                        .copied()
                        .unwrap_or(events.len())
                        .min(events.len())]
                        .iter()
                })
                .map(|(offset, data)| (*offset, data.as_slice())),
            frames,
        )?;
        write_u32(
            &mut self.mapping,
            base + MIDI_INPUT_BYTES_OFFSET,
            midi_bytes as u32,
        );
        write_u32(&mut self.mapping, base + MIDI_OUTPUT_BYTES_OFFSET, 0);
        self.state(slot).store(STATE_READY, Ordering::Release);
        Ok(SharedBlockToken {
            slot,
            sequence,
            generation: self.generation,
            frames,
        })
    }

    pub fn wait_and_copy(
        &mut self,
        token: SharedBlockToken,
        deadline: Instant,
        audio_outputs: &mut [Vec<f32>],
        midi_outputs: &mut Vec<MidiEvent>,
    ) -> std::result::Result<(), SharedBlockError> {
        loop {
            let state = self.state(token.slot).load(Ordering::Acquire);
            if state == STATE_DONE {
                if self.sequence(token.slot).load(Ordering::Relaxed) != token.sequence.0
                    || self.slot_generation(token.slot).load(Ordering::Relaxed)
                        != token.generation.0
                {
                    self.state(token.slot)
                        .store(STATE_ABANDONED, Ordering::Release);
                    return Err(SharedBlockError::StaleCompletion);
                }
                let base = slot_offset(token.slot);
                let channels = read_u32(&self.mapping, base + AUDIO_OUTPUTS_OFFSET) as usize;
                if channels != audio_outputs.len()
                    || audio_outputs
                        .iter()
                        .any(|output| output.len() < token.frames)
                {
                    self.state(token.slot).store(STATE_FREE, Ordering::Release);
                    return Err(SharedBlockError::InvalidCapacity);
                }
                for (channel, output) in audio_outputs.iter_mut().enumerate() {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            self.mapping
                                .as_ptr()
                                .add(base + AUDIO_OUTPUT_OFFSET + channel * AUDIO_PLANE_BYTES),
                            output.as_mut_ptr().cast::<u8>(),
                            token.frames * std::mem::size_of::<f32>(),
                        );
                    }
                }
                midi_outputs.clear();
                let midi_bytes = read_u32(&self.mapping, base + MIDI_OUTPUT_BYTES_OFFSET) as usize;
                let midi_result = read_midi(
                    &self.mapping
                        [base + MIDI_OUTPUT_OFFSET..base + MIDI_OUTPUT_OFFSET + midi_bytes],
                    token.frames,
                    midi_outputs,
                );
                self.state(token.slot).store(STATE_FREE, Ordering::Release);
                midi_result?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                if self
                    .state(token.slot)
                    .compare_exchange(state, STATE_ABANDONED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return Err(SharedBlockError::DeadlineMiss);
                }
                continue;
            }
            std::hint::spin_loop();
        }
    }

    /// Allocation-free completion copy into a preinitialized MIDI event pool.
    /// `midi_output_count` identifies the valid prefix; event payloads larger
    /// than their fixed preallocated capacity are reported as overflow.
    pub fn wait_and_copy_reusing_midi(
        &mut self,
        token: SharedBlockToken,
        deadline: Instant,
        audio_outputs: &mut [Vec<f32>],
        midi_outputs: &mut [MidiEvent],
        midi_output_count: &mut usize,
    ) -> std::result::Result<(), SharedBlockError> {
        loop {
            let state = self.state(token.slot).load(Ordering::Acquire);
            if state == STATE_DONE {
                if self.sequence(token.slot).load(Ordering::Relaxed) != token.sequence.0
                    || self.slot_generation(token.slot).load(Ordering::Relaxed)
                        != token.generation.0
                {
                    self.state(token.slot)
                        .store(STATE_ABANDONED, Ordering::Release);
                    return Err(SharedBlockError::StaleCompletion);
                }
                let base = slot_offset(token.slot);
                let channels = read_u32(&self.mapping, base + AUDIO_OUTPUTS_OFFSET) as usize;
                if channels != audio_outputs.len()
                    || audio_outputs
                        .iter()
                        .any(|output| output.len() < token.frames)
                {
                    self.state(token.slot).store(STATE_FREE, Ordering::Release);
                    return Err(SharedBlockError::InvalidCapacity);
                }
                for (channel, output) in audio_outputs.iter_mut().enumerate() {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            self.mapping
                                .as_ptr()
                                .add(base + AUDIO_OUTPUT_OFFSET + channel * AUDIO_PLANE_BYTES),
                            output.as_mut_ptr().cast::<u8>(),
                            token.frames * std::mem::size_of::<f32>(),
                        );
                    }
                }
                *midi_output_count = 0;
                let midi_bytes = read_u32(&self.mapping, base + MIDI_OUTPUT_BYTES_OFFSET) as usize;
                let midi_result = read_midi_reusing(
                    &self.mapping
                        [base + MIDI_OUTPUT_OFFSET..base + MIDI_OUTPUT_OFFSET + midi_bytes],
                    token.frames,
                    midi_outputs,
                    midi_output_count,
                );
                self.state(token.slot).store(STATE_FREE, Ordering::Release);
                midi_result?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                if self
                    .state(token.slot)
                    .compare_exchange(state, STATE_ABANDONED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return Err(SharedBlockError::DeadlineMiss);
                }
                continue;
            }
            std::hint::spin_loop();
        }
    }

    pub fn worker_take(&mut self) -> Option<SharedBlockToken> {
        for slot in 0..SHARED_SLOT_COUNT {
            let state = self.state(slot).load(Ordering::Acquire);
            if state == STATE_ABANDONED {
                let _ = self.state(slot).compare_exchange(
                    STATE_ABANDONED,
                    STATE_FREE,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                );
                continue;
            }
            if self
                .state(slot)
                .compare_exchange(
                    STATE_READY,
                    STATE_PROCESSING,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                continue;
            }
            let base = slot_offset(slot);
            let generation = ProcessGeneration(self.slot_generation(slot).load(Ordering::Relaxed));
            let sequence = BlockSequence(self.sequence(slot).load(Ordering::Relaxed));
            let frames = read_u32(&self.mapping, base + FRAMES_OFFSET) as usize;
            if generation != self.generation || frames == 0 || frames > MAX_BLOCK_FRAMES {
                self.state(slot).store(STATE_ABANDONED, Ordering::Release);
                continue;
            }
            return Some(SharedBlockToken {
                slot,
                sequence,
                generation,
                frames,
            });
        }
        None
    }

    pub fn worker_audio_input_channels(&self, token: SharedBlockToken) -> usize {
        read_u32(&self.mapping, slot_offset(token.slot) + AUDIO_INPUTS_OFFSET) as usize
    }

    pub fn worker_audio_output_channels(&self, token: SharedBlockToken) -> usize {
        read_u32(
            &self.mapping,
            slot_offset(token.slot) + AUDIO_OUTPUTS_OFFSET,
        ) as usize
    }

    pub fn worker_copy_audio_input(
        &self,
        token: SharedBlockToken,
        channel: usize,
        destination: &mut [f32],
    ) -> std::result::Result<(), SharedBlockError> {
        if channel >= self.worker_audio_input_channels(token) || destination.len() < token.frames {
            return Err(SharedBlockError::InvalidCapacity);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.mapping.as_ptr().add(
                    slot_offset(token.slot) + AUDIO_INPUT_OFFSET + channel * AUDIO_PLANE_BYTES,
                ),
                destination.as_mut_ptr().cast::<u8>(),
                token.frames * std::mem::size_of::<f32>(),
            );
        }
        Ok(())
    }

    pub fn worker_read_midi(
        &self,
        token: SharedBlockToken,
        destination: &mut Vec<MidiEvent>,
    ) -> std::result::Result<(), SharedBlockError> {
        let base = slot_offset(token.slot);
        let bytes = read_u32(&self.mapping, base + MIDI_INPUT_BYTES_OFFSET) as usize;
        destination.clear();
        read_midi(
            &self.mapping[base + MIDI_INPUT_OFFSET..base + MIDI_INPUT_OFFSET + bytes],
            token.frames,
            destination,
        )
    }

    pub fn worker_read_midi_reusing(
        &self,
        token: SharedBlockToken,
        destination: &mut [MidiEvent],
        count: &mut usize,
    ) -> std::result::Result<(), SharedBlockError> {
        let base = slot_offset(token.slot);
        let bytes = read_u32(&self.mapping, base + MIDI_INPUT_BYTES_OFFSET) as usize;
        *count = 0;
        read_midi_reusing(
            &self.mapping[base + MIDI_INPUT_OFFSET..base + MIDI_INPUT_OFFSET + bytes],
            token.frames,
            destination,
            count,
        )
    }

    pub fn worker_complete(
        &mut self,
        token: SharedBlockToken,
        audio_outputs: &[&[f32]],
        midi_outputs: &[MidiEvent],
    ) -> std::result::Result<(), SharedBlockError> {
        if audio_outputs.len() != self.worker_audio_output_channels(token)
            || audio_outputs
                .iter()
                .any(|output| output.len() < token.frames)
        {
            self.state(token.slot)
                .store(STATE_ABANDONED, Ordering::Release);
            return Err(SharedBlockError::InvalidCapacity);
        }
        let base = slot_offset(token.slot);
        for (channel, output) in audio_outputs.iter().enumerate() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    output.as_ptr().cast::<u8>(),
                    self.mapping
                        .as_mut_ptr()
                        .add(base + AUDIO_OUTPUT_OFFSET + channel * AUDIO_PLANE_BYTES),
                    token.frames * std::mem::size_of::<f32>(),
                );
            }
        }
        let midi_bytes = write_midi(
            &mut self.mapping[base + MIDI_OUTPUT_OFFSET..base + SLOT_CONTENT_BYTES],
            midi_outputs
                .iter()
                .map(|event| (event.frame_offset, event.data.as_slice())),
            token.frames,
        )?;
        write_u32(
            &mut self.mapping,
            base + MIDI_OUTPUT_BYTES_OFFSET,
            midi_bytes as u32,
        );
        match self.state(token.slot).compare_exchange(
            STATE_PROCESSING,
            STATE_DONE,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(STATE_ABANDONED) => {
                self.state(token.slot).store(STATE_FREE, Ordering::Release);
                Err(SharedBlockError::DeadlineMiss)
            }
            Err(_) => Err(SharedBlockError::StaleCompletion),
        }
    }

    fn state(&self, slot: usize) -> &AtomicU32 {
        unsafe {
            &*(self.mapping.as_ptr().add(slot_offset(slot) + STATE_OFFSET) as *const AtomicU32)
        }
    }

    fn sequence(&self, slot: usize) -> &AtomicU64 {
        unsafe {
            &*(self
                .mapping
                .as_ptr()
                .add(slot_offset(slot) + SEQUENCE_OFFSET) as *const AtomicU64)
        }
    }

    fn slot_generation(&self, slot: usize) -> &AtomicU64 {
        unsafe {
            &*(self
                .mapping
                .as_ptr()
                .add(slot_offset(slot) + GENERATION_OFFSET) as *const AtomicU64)
        }
    }
}

fn slot_offset(slot: usize) -> usize {
    FILE_HEADER_SIZE + slot * SLOT_SIZE
}

fn write_u32(mapping: &mut [u8], offset: usize, value: u32) {
    mapping[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(mapping: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(mapping[offset..offset + 4].try_into().expect("u32 field"))
}

fn write_u64(mapping: &mut [u8], offset: usize, value: u64) {
    mapping[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u64(mapping: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(mapping[offset..offset + 8].try_into().expect("u64 field"))
}

fn write_midi<'a>(
    destination: &mut [u8],
    events: impl IntoIterator<Item = (u32, &'a [u8])>,
    frames: usize,
) -> std::result::Result<usize, SharedBlockError> {
    let mut offset = 0usize;
    let mut count = 0usize;
    for (frame_offset, data) in events {
        count += 1;
        if count > MAX_MIDI_EVENTS_PER_BLOCK
            || frame_offset as usize >= frames
            || offset + 8 + data.len() > destination.len()
        {
            return Err(SharedBlockError::MidiOverflow);
        }
        destination[offset..offset + 4].copy_from_slice(&frame_offset.to_le_bytes());
        destination[offset + 4..offset + 8].copy_from_slice(&(data.len() as u32).to_le_bytes());
        offset += 8;
        destination[offset..offset + data.len()].copy_from_slice(data);
        offset += data.len();
    }
    Ok(offset)
}

fn read_midi_reusing(
    source: &[u8],
    frames: usize,
    destination: &mut [MidiEvent],
    count: &mut usize,
) -> std::result::Result<(), SharedBlockError> {
    let mut offset = 0usize;
    while offset < source.len() {
        if *count == destination.len() || offset + 8 > source.len() {
            return Err(SharedBlockError::MidiOverflow);
        }
        let frame_offset = u32::from_le_bytes(
            source[offset..offset + 4]
                .try_into()
                .map_err(|_| SharedBlockError::MidiOverflow)?,
        );
        let bytes = u32::from_le_bytes(
            source[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| SharedBlockError::MidiOverflow)?,
        ) as usize;
        offset += 8;
        let event = &mut destination[*count];
        if frame_offset as usize >= frames
            || offset + bytes > source.len()
            || bytes > event.data.capacity()
        {
            return Err(SharedBlockError::MidiOverflow);
        }
        event.frame_offset = frame_offset;
        event.data.clear();
        event
            .data
            .extend_from_slice(&source[offset..offset + bytes]);
        *count += 1;
        offset += bytes;
    }
    Ok(())
}

fn read_midi(
    source: &[u8],
    frames: usize,
    destination: &mut Vec<MidiEvent>,
) -> std::result::Result<(), SharedBlockError> {
    let mut offset = 0usize;
    while offset < source.len() {
        if destination.len() == MAX_MIDI_EVENTS_PER_BLOCK || offset + 8 > source.len() {
            return Err(SharedBlockError::MidiOverflow);
        }
        let frame_offset = u32::from_le_bytes(
            source[offset..offset + 4]
                .try_into()
                .map_err(|_| SharedBlockError::MidiOverflow)?,
        );
        let bytes = u32::from_le_bytes(
            source[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| SharedBlockError::MidiOverflow)?,
        ) as usize;
        offset += 8;
        if frame_offset as usize >= frames || offset + bytes > source.len() {
            return Err(SharedBlockError::MidiOverflow);
        }
        destination.push(MidiEvent {
            frame_offset,
            data: source[offset..offset + bytes].to_vec(),
        });
        offset += bytes;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[shoop_wasm_test_support::shoop_test]
    fn shared_slots_round_trip_audio_midi_and_identity() {
        let nonce = [9; 32];
        let generation = ProcessGeneration(4);
        let mut parent = SharedBlockTransport::create(generation, &nonce).unwrap();
        let mut worker = SharedBlockTransport::open(parent.path(), generation, &nonce).unwrap();
        let inputs = vec![vec![0.25; 64], vec![-0.5; 64]];
        let midi = vec![vec![(7, vec![0x90, 64, 100])]];
        let token = parent
            .submit(BlockSequence(3), 64, &inputs, 2, &midi, &[])
            .unwrap();
        let worker_token = worker.worker_take().expect("ready block");
        assert_eq!(worker_token, token);
        let mut first = vec![0.0; 64];
        worker
            .worker_copy_audio_input(worker_token, 0, &mut first)
            .unwrap();
        assert_eq!(first, inputs[0]);
        let mut worker_midi = Vec::new();
        worker
            .worker_read_midi(worker_token, &mut worker_midi)
            .unwrap();
        assert_eq!(worker_midi[0].frame_offset, 7);
        assert_eq!(worker_midi[0].data, vec![0x90, 64, 100]);
        worker
            .worker_complete(worker_token, &[&inputs[0], &inputs[1]], &worker_midi)
            .unwrap();
        let mut outputs = vec![vec![0.0; 64], vec![0.0; 64]];
        let mut output_midi = Vec::new();
        parent
            .wait_and_copy(
                token,
                Instant::now() + Duration::from_secs(1),
                &mut outputs,
                &mut output_midi,
            )
            .unwrap();
        assert_eq!(outputs, inputs);
        assert_eq!(output_midi, worker_midi);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn timeout_abandons_without_unsafe_parent_reuse() {
        let nonce = [3; 32];
        let generation = ProcessGeneration(2);
        let mut parent = SharedBlockTransport::create(generation, &nonce).unwrap();
        let inputs = vec![vec![0.0; 8]];
        let token = parent
            .submit(BlockSequence(1), 8, &inputs, 1, &[], &[])
            .unwrap();
        assert_eq!(
            parent.wait_and_copy(token, Instant::now(), &mut [vec![0.0; 8]], &mut Vec::new()),
            Err(SharedBlockError::DeadlineMiss)
        );
        let worker = SharedBlockTransport::open(parent.path(), generation, &nonce).unwrap();
        assert_eq!(
            parent.state(token.slot).load(Ordering::Acquire),
            STATE_ABANDONED
        );
        assert_eq!(
            worker.state(token.slot).load(Ordering::Acquire),
            STATE_ABANDONED
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn slots_support_out_of_order_completion_reject_duplicates_and_recover_after_timeouts() {
        let nonce = [6; 32];
        let generation = ProcessGeneration(9);
        let mut parent = SharedBlockTransport::create(generation, &nonce).unwrap();
        let mut worker = SharedBlockTransport::open(parent.path(), generation, &nonce).unwrap();
        let inputs = vec![vec![0.5; 8]];
        let mut parent_tokens = Vec::new();
        for sequence in 1..=SHARED_SLOT_COUNT as u64 {
            parent_tokens.push(
                parent
                    .submit(BlockSequence(sequence), 8, &inputs, 1, &[], &[])
                    .unwrap(),
            );
        }
        assert_eq!(
            parent.submit(BlockSequence(10), 8, &inputs, 1, &[], &[]),
            Err(SharedBlockError::NoFreeSlot)
        );
        let mut worker_tokens = Vec::new();
        while let Some(token) = worker.worker_take() {
            worker_tokens.push(token);
        }
        for token in worker_tokens.iter().rev().copied() {
            worker.worker_complete(token, &[&[0.25; 8]], &[]).unwrap();
        }
        for token in parent_tokens.iter().rev().copied() {
            parent
                .wait_and_copy(
                    token,
                    Instant::now() + Duration::from_millis(10),
                    &mut [vec![0.0; 8]],
                    &mut Vec::new(),
                )
                .unwrap();
        }
        assert_eq!(
            worker.worker_complete(worker_tokens[0], &[&[0.25; 8]], &[]),
            Err(SharedBlockError::StaleCompletion)
        );

        for sequence in 20..20 + SHARED_SLOT_COUNT as u64 {
            let token = parent
                .submit(BlockSequence(sequence), 8, &inputs, 1, &[], &[])
                .unwrap();
            assert_eq!(
                parent.wait_and_copy(token, Instant::now(), &mut [vec![0.0; 8]], &mut Vec::new()),
                Err(SharedBlockError::DeadlineMiss)
            );
        }
        assert!(worker.worker_take().is_none());
        let recovered = parent
            .submit(BlockSequence(30), 8, &inputs, 1, &[], &[])
            .unwrap();
        let worker_token = worker.worker_take().unwrap();
        assert_eq!(worker_token, recovered);
        worker
            .worker_complete(worker_token, &[&[0.75; 8]], &[])
            .unwrap();
        parent
            .wait_and_copy(
                recovered,
                Instant::now() + Duration::from_millis(10),
                &mut [vec![0.0; 8]],
                &mut Vec::new(),
            )
            .unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn paths_with_spaces_and_non_ascii_are_supported() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("Shoop IPC ü space");
        std::fs::create_dir(&directory).unwrap();
        let nonce = [4; 32];
        let generation = ProcessGeneration(7);
        let parent = SharedBlockTransport::create_in(&directory, generation, &nonce).unwrap();
        assert!(parent.path().starts_with(&directory));
        let worker = SharedBlockTransport::open(parent.path(), generation, &nonce).unwrap();
        assert_eq!(worker.generation, generation);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn wrong_nonce_or_generation_is_rejected() {
        let nonce = [4; 32];
        let parent = SharedBlockTransport::create(ProcessGeneration(1), &nonce).unwrap();
        assert!(SharedBlockTransport::open(parent.path(), ProcessGeneration(2), &nonce).is_err());
        assert!(SharedBlockTransport::open(parent.path(), ProcessGeneration(1), &[5; 32]).is_err());
    }
}
