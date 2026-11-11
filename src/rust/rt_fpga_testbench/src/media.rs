//! Serial media commands for comparing a storage controller independently of jobs.
use crate::ExecutionStatus;
use rt_fpga_reference::{
    fixed::Fixed,
    storage::{MediaSnapshot, MediaStore, ViewId},
    Error,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaCase {
    pub name: String,
    pub media: MediaSnapshot,
    pub steps: Vec<MediaCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaCommand {
    Create,
    Slice {
        source: ViewId,
        start: u64,
        frames: u64,
    },
    Clear {
        view: ViewId,
    },
    Remove {
        view: ViewId,
    },
    Reclaim {
        view: ViewId,
    },
    Append {
        view: ViewId,
        samples: Vec<Fixed>,
    },
    Read {
        view: ViewId,
        position: u64,
        length: u64,
        frames: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MediaValue {
    None,
    View(ViewId),
    Empty(bool),
    Samples(Vec<Fixed>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaResult {
    pub status: ExecutionStatus,
    pub value: MediaValue,
    pub media: MediaSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaTrace {
    pub case: MediaCase,
    pub results: Vec<MediaResult>,
}

pub fn run_media_case(case: MediaCase) -> Result<MediaTrace, Error> {
    let mut media = MediaStore::from_snapshot(case.media.clone())?;
    let mut results = Vec::with_capacity(case.steps.len());
    for command in &case.steps {
        let result = match command {
            MediaCommand::Create => media.create_recording().map(MediaValue::View),
            MediaCommand::Slice {
                source,
                start,
                frames,
            } => media.slice(*source, *start, *frames).map(MediaValue::View),
            MediaCommand::Clear { view } => media.clear(*view).map(|()| MediaValue::None),
            MediaCommand::Remove { view } => media.remove(*view).map(|()| MediaValue::None),
            MediaCommand::Reclaim { view } => media.reclaim_chunk(*view).map(MediaValue::Empty),
            MediaCommand::Append { view, samples } => {
                media.append(*view, samples).map(|()| MediaValue::None)
            }
            MediaCommand::Read {
                view,
                position,
                length,
                frames,
            } => (|| {
                media.read_loop(*view, *position, *length, &mut [])?;
                let mut samples = Vec::new();
                samples
                    .try_reserve_exact(*frames as usize)
                    .map_err(|_| Error::MemoryFull)?;
                samples.resize(*frames as usize, Fixed::ZERO);
                media
                    .read_loop(*view, *position, *length, &mut samples)
                    .map(|()| MediaValue::Samples(samples))
            })(),
        };
        let (status, value) = match result {
            Ok(value) => (ExecutionStatus::Completed, value),
            Err(error) => (ExecutionStatus::Failed { error }, MediaValue::None),
        };
        results.push(MediaResult {
            status,
            value,
            media: media.snapshot(),
        });
    }
    Ok(MediaTrace { case, results })
}
