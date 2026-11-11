use rt_fpga_reference::{
    resolver::{resolve, ReadIntent},
    storage::{ChunkId, MediaSnapshot, MediaStore},
    Error,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolverCase {
    pub name: String,
    pub media: MediaSnapshot,
    pub requests: Vec<ReadIntent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolverTrace {
    pub case: ResolverCase,
    pub results: Vec<Vec<ChunkId>>,
}

pub fn run_resolver_case(case: ResolverCase) -> Result<ResolverTrace, Error> {
    let media = MediaStore::from_snapshot(case.media.clone())?;
    let results = case
        .requests
        .iter()
        .map(|&intent| {
            let mut ids = Vec::new();
            resolve(intent, &media, |chunk| ids.push(chunk));
            ids
        })
        .collect();
    Ok(ResolverTrace { case, results })
}
