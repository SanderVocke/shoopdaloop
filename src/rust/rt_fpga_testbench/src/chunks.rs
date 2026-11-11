//! Functional chunk-memory traces; transfer timing is checked by the RTL transport.
use crate::ExecutionStatus;
use rt_fpga_reference::{chunk_memory::ChunkMemory, fixed::Fixed, storage::ChunkId, Error};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkCase {
    pub name: String,
    pub chunk_frames: u32,
    pub samples: Vec<Fixed>,
    pub requests: Vec<ChunkRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkRequest {
    pub id: u32,
    pub operation: ChunkOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChunkOperation {
    Read {
        chunk: ChunkId,
        offset: u32,
        frames: u32,
    },
    Write {
        chunk: ChunkId,
        offset: u32,
        samples: Vec<Fixed>,
    },
    Prefetch {
        chunk: ChunkId,
    },
    Fence,
    Retire {
        chunk: ChunkId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkResult {
    pub id: u32,
    pub status: ExecutionStatus,
    pub samples: Vec<Fixed>,
    pub memory: Vec<Fixed>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkTrace {
    pub case: ChunkCase,
    pub results: Vec<ChunkResult>,
}

pub fn run_chunk_case(case: ChunkCase) -> Result<ChunkTrace, Error> {
    let mut memory = ChunkMemory::from_samples(case.chunk_frames as usize, case.samples.clone())?;
    let mut results = Vec::with_capacity(case.requests.len());
    for request in &case.requests {
        let mut samples = Vec::new();
        let result = match &request.operation {
            ChunkOperation::Read {
                chunk,
                offset,
                frames,
            } => (|| {
                // Validate before allocating a caller-controlled result length.
                memory.read(*chunk, *offset as usize, &mut [])?;
                if *frames as usize > memory.chunk_frames() - *offset as usize {
                    return Err(Error::InvalidShape);
                }
                samples
                    .try_reserve_exact(*frames as usize)
                    .map_err(|_| Error::MemoryFull)?;
                samples.resize(*frames as usize, Fixed::ZERO);
                memory.read(*chunk, *offset as usize, &mut samples)
            })(),
            ChunkOperation::Write {
                chunk,
                offset,
                samples,
            } => memory.write(*chunk, *offset as usize, samples),
            ChunkOperation::Prefetch { chunk } => memory.prefetch(*chunk),
            ChunkOperation::Fence => memory.fence(),
            ChunkOperation::Retire { chunk } => memory.retire(*chunk),
        };
        results.push(ChunkResult {
            id: request.id,
            status: match result {
                Ok(()) => ExecutionStatus::Completed,
                Err(error) => ExecutionStatus::Failed { error },
            },
            samples,
            memory: memory.samples().to_vec(),
        });
    }
    Ok(ChunkTrace { case, results })
}
