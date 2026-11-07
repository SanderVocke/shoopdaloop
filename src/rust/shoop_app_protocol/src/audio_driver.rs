use serde::{Deserialize, Serialize};
use shoop_app_api::AudioDriverConfig;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Command {
    Discover(AudioDriverConfig),
    Request(AudioDriverConfig),
    Confirm { request_id: u64, accept: bool },
    RetryPersistence { request_id: u64 },
}

impl Command {
    pub fn bounded(&self) -> bool {
        match self {
            Self::Discover(config) | Self::Request(config) => config.bounded(),
            Self::Confirm { request_id, .. } | Self::RetryPersistence { request_id } => {
                *request_id != 0
            }
        }
    }
}
