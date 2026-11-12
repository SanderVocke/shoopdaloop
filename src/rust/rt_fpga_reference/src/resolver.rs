//! Advisory logical reads, resolved independently of ordered sample execution.
use crate::{
    jobs::Memory,
    storage::{ChunkId, MediaStore, ViewId},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReadIntent {
    pub view: ViewId,
    #[cfg_attr(feature = "serde", serde(rename = "position_chunks"))]
    pub position: u64,
    #[cfg_attr(feature = "serde", serde(rename = "length_chunks"))]
    pub length: u64,
    pub chunks: u64,
}

impl ReadIntent {
    pub fn prefetch(self, memory: &mut impl Memory) {
        memory.prefetch(self.view, self.position, self.length, self.chunks);
    }
}

pub fn resolve(intent: ReadIntent, media: &MediaStore, mut sink: impl FnMut(ChunkId)) {
    let Ok(frames) = media.frames(intent.view) else {
        return;
    };
    let cf = media.chunk_memory().chunk_frames();
    let Some(length) = intent.length.checked_mul(cf as u64) else {
        return;
    };
    let Some(mut cursor) = intent.position.checked_mul(cf as u64) else {
        return;
    };
    let Some(mut remaining) = intent.chunks.checked_mul(cf as u64) else {
        return;
    };
    if length == 0 || length > frames as u64 || cursor >= length {
        return;
    }
    let Ok(first_offset) = media.range_offset(intent.view) else {
        return;
    };
    while remaining != 0 {
        let Some(at) = (first_offset as u64).checked_add(cursor) else {
            return;
        };
        let Ok(ordinal) = usize::try_from(at / cf as u64) else {
            return;
        };
        let Ok(chunk) = media.chunk_at(intent.view, ordinal) else {
            return;
        };
        sink(chunk);
        let span = remaining
            .min(cf as u64 - at % cf as u64)
            .min(length - cursor);
        remaining -= span;
        cursor = if cursor + span == length {
            0
        } else {
            cursor + span
        };
    }
}
