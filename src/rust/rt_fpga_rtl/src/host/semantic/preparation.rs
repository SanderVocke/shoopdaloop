use super::BoundTarget;
use crate::{
    host::{Upload, UploadStep, control::descriptor},
    target::ProgramImage,
};
use alloc::vec::Vec;
use rt_fpga_compiler::{CompileError, Identity};
use shoop_rt_protocol::immediate::SwitchProgram;

pub enum PreparationStep<'a> {
    Upload(UploadStep<'a>),
    Observation { slot: u16, word: u8, value: u64 },
}

/// Immutable lowering recipe, not an allocation lease or permission to issue writes.
/// Its owner must correlate allocator state, incarnation, upload completion and fences.
pub struct Preparation {
    upload: Upload,
    observations: Vec<(u16, u32)>,
    cursor: usize,
}
impl Preparation {
    pub fn new(image: ProgramImage) -> Result<Self, CompileError> {
        let program = &image.compiled;
        let invalid = CompileError::InvalidProgram;
        if !image.whole_sync_prefix
            || program.quantum != 32
            || program.chunk_frames != 32
            || image.layout.region > 1
            || image.layout.revision <= image.layout.expected
            || program.instructions.len() > 1024
            || program.state.len() > 512
            || program.sync_count != image.layout.loops.len()
            || program.bindings.len() != image.layout.loops.len()
            || program.resources.buffers > 512
            || program.resources.history_slots > 512
            || program.resources.input_ports > 1
            || program.resources.output_ports > 2
            || program.resources.queue_capacity > 512
            || program.jobs_per_quantum > 512
            || image.layout.allocations().state > 512
            || image.layout.allocations().loops > 128
            || image.layout.allocations().history > 512
        {
            return Err(invalid);
        }
        let mut cursor = 0;
        for _ in 0..program.sync_count {
            let instruction = program.instructions.get(cursor).ok_or(invalid)?;
            cursor += match instruction.get(0, 8) {
                12 => 1,
                22 => 1 + instruction.get(72, 16) as usize,
                _ => return Err(invalid),
            };
            if cursor > program.instructions.len() {
                return Err(invalid);
            }
        }
        if program.instructions[cursor..]
            .iter()
            .any(|word| matches!(word.get(0, 8), 12 | 14..=22))
        {
            return Err(invalid);
        }
        for slot in &image.layout.loops {
            BoundTarget::new(slot.physical.slot, slot.physical.generation, None, true)
                .map_err(|_| invalid)?;
            let binding = program
                .bindings
                .iter()
                .find(|b| b.id == slot.physical)
                .ok_or(invalid)?;
            if binding
                .gain
                .is_some_and(|address| usize::from(address) >= program.state.len())
            {
                return Err(invalid);
            }
        }
        let fresh: Vec<_> = image
            .layout
            .new_values()
            .map(|(_, address)| address)
            .collect();
        if image.initializers.len() != fresh.len()
            || image
                .initializers
                .iter()
                .enumerate()
                .any(|(i, (address, value))| {
                    !fresh.contains(&u32::from(*address))
                        || image.initializers[..i].iter().any(|(a, _)| a == address)
                        || program.state.get(usize::from(*address)) != Some(value)
                })
            || image.new_loops.len() != image.layout.new_loops().count()
            || image.new_loops.iter().enumerate().any(|(i, row)| {
                !image.layout.new_loops().any(|id| id == row.id)
                    || image.new_loops[..i].iter().any(|r| r.id == row.id)
                    || !program
                        .loops
                        .iter()
                        .any(|r| r.id == row.id && r.words == row.words)
            })
        {
            return Err(invalid);
        }
        let observations = image
            .layout
            .new_loops()
            .map(|id| {
                let bits = program
                    .bindings
                    .iter()
                    .find(|b| b.id == id)
                    .and_then(|binding| binding.initial_gain_bits)
                    .ok_or(invalid)?;
                if !f32::from_bits(bits).is_finite() {
                    return Err(invalid);
                }
                Ok((id.slot as u16, bits))
            })
            .collect::<Result<_, CompileError>>()?;
        Ok(Self {
            upload: Upload::new(image, 1024),
            observations,
            cursor: 0,
        })
    }
    pub fn image(&self) -> &ProgramImage {
        &self.upload.image
    }
    pub fn descriptor(&self) -> SwitchProgram {
        descriptor(self.image())
    }
    /// State-independent physical operands; not selected-revision or lease authority.
    pub fn target(&self, logical: Identity) -> Option<BoundTarget> {
        let image = self.image();
        let slot = image
            .layout
            .loops
            .iter()
            .find(|slot| slot.logical == logical)?;
        let binding = image
            .compiled
            .bindings
            .iter()
            .find(|binding| binding.id == slot.physical)?;
        BoundTarget::new(
            slot.physical.slot,
            slot.physical.generation,
            binding.gain.map(u32::from),
            slot.sync_source.is_none_or(|source| source == logical),
        )
        .ok()
    }
    /// Iterates a prepared recipe only; advancing is not successful device delivery.
    pub fn next_step(&mut self) -> Option<PreparationStep<'_>> {
        if let Some(step) = self.upload.next_step() {
            return Some(PreparationStep::Upload(step));
        }
        let &(slot, bits) = self.observations.get(self.cursor / 3)?;
        let word = (self.cursor % 3) as u8;
        self.cursor += 1;
        Some(PreparationStep::Observation {
            slot,
            word,
            value: if word == 0 { u64::from(bits) } else { 0 },
        })
    }
}
