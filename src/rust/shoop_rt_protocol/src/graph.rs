//! Complete graph uploads terminate at the ARM-side compiler, never at FPGA logic.
use crate::immediate::{Batch, Command, COMMANDS};
use alloc::vec::Vec;
use shoop_graph::{Graph, GraphView, MAX_TRANSFER_BYTES};
#[derive(Debug)]
pub enum UploadError {
    Encode,
    Capacity,
}
impl core::fmt::Display for UploadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "graph upload: {self:?}")
    }
}
impl core::error::Error for UploadError {}
pub fn upload(
    view: &impl GraphView,
    quantum: u32,
    revision: u64,
) -> Result<Vec<Batch>, UploadError> {
    let bytes = Graph::capture(view)
        .encode()
        .map_err(|_| UploadError::Encode)?;
    if bytes.is_empty() || bytes.len() > MAX_TRANSFER_BYTES {
        return Err(UploadError::Capacity);
    }
    let mut commands = alloc::vec![Command::BeginGraphImage {
        quantum,
        bytes: bytes.len() as u32
    }];
    for chunk in bytes.chunks(20) {
        let mut bytes = [0; 20];
        bytes[..chunk.len()].copy_from_slice(chunk);
        commands.push(Command::GraphImageData {
            len: chunk.len() as u8,
            bytes,
        });
    }
    commands.push(Command::ActivateGraph);
    Ok(commands
        .chunks(COMMANDS)
        .map(|c| Batch::new(revision, revision, c).unwrap())
        .collect())
}
