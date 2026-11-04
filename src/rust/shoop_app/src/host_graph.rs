use alloc::string::String;
use alloc::sync::Arc;
use shoop_app_api::{HostGraphRequest, HostGraphState};
use shoop_app_protocol::Refusal;

pub struct HostGraphError {
    pub reason: Refusal,
    pub message: String,
}

/// Optional host control adapter. The service owns operation tracking and authority.
pub trait HostGraphController: Send {
    fn snapshot(&mut self) -> Arc<HostGraphState>;
    fn submit(&mut self, request: HostGraphRequest) -> Result<u64, HostGraphError>;
    fn take_result(&mut self) -> Option<(u64, Result<(), HostGraphError>)>;
}
