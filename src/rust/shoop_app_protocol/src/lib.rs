//! Session control contracts, independent of engine, UI, Lua and concrete carriers.
#![forbid(unsafe_code)]

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

pub mod codec;
pub mod local;
pub mod persistence;
pub mod persistence_image;
pub mod persistence_transfer;
pub mod stream;

use serde::{Deserialize, Serialize};
use shoop_app_api::*;
use std::sync::Arc;

pub const VERSION: u16 = 8;
pub const CHUNK_BYTES: usize = 1024;
pub const MAX_MEDIA_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PENDING_OPERATIONS: usize = 128;
pub const MAX_RETAINED_RESULTS: usize = 256;
pub const QUEUE_CAPACITY: usize = 256;
pub const MAX_MESSAGES_PER_TURN: usize = 64;
pub const MAX_COMMANDS_PER_TURN: usize = 16;

macro_rules! identity {
    ($name:ident, $value:ty) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        pub struct $name(pub $value);
    };
}
identity!(HostId, u128);
identity!(SessionId, u128);
identity!(ClientId, u128);
identity!(ConnectionGeneration, u64);
identity!(Revision, u64);
identity!(TransferId, u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Authority {
    pub session: SessionId,
    pub client: ClientId,
    pub generation: ConnectionGeneration,
}

impl Authority {
    pub fn is_valid(self) -> bool {
        self.session.0 != 0 && self.client.0 != 0 && self.generation.0 != 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OperationId {
    pub client: ClientId,
    pub sequence: u64,
}

/// Shared operations only. Platform dialogs, script VMs and UI drafts are local.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Command {
    AddTrack(TrackSpec),
    AddLoop {
        track: TrackId,
    },
    Track {
        track: TrackId,
        action: TrackAction,
    },
    Loop {
        track: TrackId,
        loop_id: LoopId,
        action: LoopAction,
    },
    Bus {
        bus: BusId,
        action: BusAction,
    },
    AddBus(BusSpec),
    Global(GlobalControlAction),
    Piano(PianoAction),
    SetPortConnected {
        port: PortId,
        host_port: HostPortId,
        connected: bool,
    },
    SetMixerRoute {
        source: PortId,
        destination: BusChannelId,
        connected: bool,
    },
    SetMasterAutoConnect(bool),
    SetLoopSmoothingMs(u32),
    SelectLoops {
        loops: Vec<LoopId>,
    },
    SetTarget(Option<LoopId>),
    PlaySelected,
    RecordWithTarget {
        loops: Vec<LoopId>,
    },
    ResetXruns,
    Control(ControlOperation),
    NewSession,
    CommitPersistence(persistence::Key),
}

impl Command {
    pub fn bounded(&self) -> bool {
        let name = |text: &str| text.len() <= 128;
        let detail = |text: &str| text.len() <= 512;
        match self {
            Self::CommitPersistence(key) => key.valid(),
            Self::Control(operation) => operation.bounded(),
            Self::AddTrack(spec) => {
                name(&spec.name)
                    && spec.initial_output_bus_names.len() <= 64
                    && spec.initial_output_bus_names.iter().all(|text| name(text))
                    && spec
                        .initial_output_bus_names
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
                        <= 512
                    && match &spec.topology {
                        TrackSpecTopology::Direct { .. } => true,
                        TrackSpecTopology::DryWet { processor_type, .. } => {
                            name(processor_type.as_str())
                        }
                    }
            }
            Self::AddBus(spec) => {
                name(&spec.name)
                    && spec
                        .fx
                        .as_ref()
                        .is_none_or(|fx| name(fx.processor_type.as_str()))
            }
            Self::Track { action, .. } => match action {
                TrackAction::NameChanged(text) => name(text),
                TrackAction::FxRestoreState(text) => detail(text),
                TrackAction::OxiSynth(OxiSynthControl::SelectPreset(text)) => name(text),
                _ => true,
            },
            Self::Loop {
                action: LoopAction::NameChanged(text),
                ..
            } => name(text),
            Self::Bus { action, .. } => match action {
                BusAction::FxRestoreState(text) => detail(text),
                BusAction::FxProcessorChanged(Some(id)) => name(id.as_str()),
                _ => true,
            },
            Self::SetPortConnected { host_port, .. } => name(host_port.as_str()),
            Self::SelectLoops { loops } | Self::RecordWithTarget { loops } => loops.len() <= 128,
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Refusal {
    Version,
    Identity,
    Busy,
    StaleAuthority,
    NotReady,
    Overloaded,
    RevisionConflict,
    Duplicate,
    Invalid,
    Unsupported,
    Engine,
    ShutdownFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Outcome {
    Confirmed {
        revision: Revision,
    },
    Refused {
        reason: Refusal,
        message: String,
    },
    TrackCreated {
        revision: Revision,
        track: TrackId,
        loops: Vec<LoopId>,
    },
    LoopCreated {
        revision: Revision,
        track: TrackId,
        loop_id: LoopId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingOperation {
    pub id: OperationId,
    pub accepted_at: Revision,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationResult {
    pub id: OperationId,
    pub outcome: Outcome,
}

/// Ordinary shared state, without Lua, file workflows, editor drafts or media reads.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionSnapshot {
    pub persistence_epoch: u64,
    pub revision: Revision,
    pub tracks: Vec<TrackState>,
    pub buses: Arc<[BusState]>,
    pub global: GlobalControlState,
    pub master_auto_connect: bool,
    pub loop_smoothing_ms: u32,
    pub status: StatusState,
    pub audio_drivers: AudioDriverRuntimeState,
    pub capabilities: BackendCapabilities,
    pub connections: Arc<ConnectionViewState>,
    pub track_processors: Arc<[TrackProcessorDescriptor]>,
    pub bus_processors: Arc<[TrackProcessorDescriptor]>,
    pub pending: Vec<PendingOperation>,
    pub results: Vec<OperationResult>,
}

impl SessionSnapshot {
    pub fn valid(&self) -> bool {
        self.persistence_epoch != 0
            && self.pending.len() <= MAX_PENDING_OPERATIONS
            && self.results.len() <= MAX_RETAINED_RESULTS
            && self
                .pending
                .iter()
                .all(|pending| operation_valid(pending.id) && pending.accepted_at <= self.revision)
            && self
                .results
                .iter()
                .all(|result| operation_valid(result.id) && result.outcome.bounded())
    }
}

fn operation_valid(id: OperationId) -> bool {
    id.client.0 != 0 && id.sequence != 0
}

impl Outcome {
    pub fn bounded(&self) -> bool {
        match self {
            Self::Confirmed { .. } => true,
            Self::TrackCreated { track, loops, .. } => {
                track.is_valid() && loops.len() <= 128 && loops.iter().all(|id| id.is_valid())
            }
            Self::LoopCreated { track, loop_id, .. } => track.is_valid() && loop_id.is_valid(),
            Self::Refused { message, .. } => message.len() <= 512,
        }
    }
}

/// Typed delivery uses these values directly; it never invokes the wire codec.
#[derive(Clone, Debug)]
pub enum ServiceMessage {
    CapturedImage {
        authority: Authority,
        key: persistence::Key,
        image: Arc<persistence_image::Image>,
    },
    Persistence {
        authority: Authority,
        reply: persistence::Reply,
    },
    Attached {
        authority: Authority,
        host: HostId,
    },
    Rejected(Refusal),
    Snapshot {
        authority: Authority,
        state: Arc<SessionSnapshot>,
    },
    Accepted {
        authority: Authority,
        operation: PendingOperation,
    },
    Finished {
        authority: Authority,
        result: OperationResult,
    },
    Detached {
        authority: Authority,
    },
    ShutdownComplete {
        authority: Authority,
        result: OperationResult,
    },
    Media {
        authority: Authority,
        transfer: TransferId,
        data: Arc<LoopMedia>,
    },
    MediaRefused {
        authority: Authority,
        transfer: TransferId,
        reason: Refusal,
    },
}

impl ServiceMessage {
    pub fn valid(&self) -> bool {
        match self {
            Self::CapturedImage { authority, key, .. } => authority.is_valid() && key.valid(),
            Self::Persistence { authority, reply } => authority.is_valid() && reply.valid(),
            Self::Media {
                authority,
                transfer,
                data,
            } => authority.is_valid() && transfer.0 != 0 && data.valid(),
            Self::MediaRefused {
                authority,
                transfer,
                ..
            } => authority.is_valid() && transfer.0 != 0,
            Self::Attached { authority, host } => authority.is_valid() && host.0 != 0,
            Self::Snapshot { authority, state } => authority.is_valid() && state.valid(),
            Self::Accepted {
                authority,
                operation,
            } => authority.is_valid() && operation_valid(operation.id),
            Self::Finished { authority, result } => {
                authority.is_valid() && operation_valid(result.id) && result.outcome.bounded()
            }
            Self::Detached { authority } => authority.is_valid(),
            Self::ShutdownComplete { authority, result } => {
                authority.is_valid()
                    && operation_valid(result.id)
                    && result.id.client == authority.client
                    && result.outcome.bounded()
            }
            Self::Rejected(_) => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ClientMessage {
    Persistence {
        authority: Authority,
        request: persistence::Request,
    },
    Attach {
        version: u16,
        host: HostId,
        session: SessionId,
        client: ClientId,
    },
    Ready {
        authority: Authority,
        revision: Revision,
    },
    Submit {
        authority: Authority,
        session_epoch: u64,
        operation: OperationId,
        expected_revision: Option<Revision>,
        command: Command,
    },
    Detach {
        authority: Authority,
    },
    Shutdown {
        authority: Authority,
        operation: OperationId,
    },
    ReadMedia {
        authority: Authority,
        transfer: TransferId,
        loop_id: LoopId,
    },
    CancelMedia {
        authority: Authority,
    },
    #[serde(skip)]
    StageImage {
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
        image: Arc<persistence_image::Image>,
    },
    #[serde(skip)]
    CaptureImage {
        authority: Authority,
        key: persistence::Key,
        target: persistence::Target,
    },
}

/// Snapshot chunks share the same bounded record format as commands and results.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum WireMessage {
    Persistence {
        authority: Authority,
        reply: persistence::Reply,
    },
    Client(ClientMessage),
    Attached {
        authority: Authority,
        host: HostId,
    },
    Rejected(Refusal),
    SnapshotBegin {
        authority: Authority,
        transfer: TransferId,
        revision: Revision,
        bytes: u32,
    },
    SnapshotChunk {
        transfer: TransferId,
        index: u32,
        bytes: Vec<u8>,
    },
    SnapshotEnd {
        transfer: TransferId,
    },
    Accepted {
        authority: Authority,
        operation: PendingOperation,
    },
    Finished {
        authority: Authority,
        result: OperationResult,
    },
    Detached {
        authority: Authority,
    },
    ShutdownComplete {
        authority: Authority,
        result: OperationResult,
    },
    MediaBegin {
        authority: Authority,
        transfer: TransferId,
        bytes: u32,
    },
    MediaChunk {
        transfer: TransferId,
        index: u32,
        bytes: Vec<u8>,
    },
    MediaEnd {
        transfer: TransferId,
    },
    MediaRefused {
        authority: Authority,
        transfer: TransferId,
        reason: Refusal,
    },
    ManagementRequest(ManagementRequest),
    ManagementReply(ManagementReply),
    CarrierReady {
        host: HostId,
        session: SessionId,
    },
}

impl WireMessage {
    pub fn valid(&self) -> bool {
        let operation_valid = |authority: &Authority, operation: &OperationId| {
            authority.is_valid() && operation.client == authority.client && operation.sequence != 0
        };
        match self {
            Self::Client(ClientMessage::StageImage { .. } | ClientMessage::CaptureImage { .. }) => {
                false
            }
            Self::Persistence { authority, reply } => authority.is_valid() && reply.valid(),
            Self::Client(ClientMessage::Persistence { authority, request }) => {
                authority.is_valid() && request.valid()
            }
            Self::CarrierReady { host, session } => host.0 != 0 && session.0 != 0,
            Self::ManagementRequest(request) => request.valid(),
            Self::ManagementReply(reply) => reply.valid(),
            Self::MediaBegin {
                authority,
                transfer,
                bytes,
            } => {
                authority.is_valid()
                    && transfer.0 != 0
                    && *bytes > 0
                    && *bytes as usize <= MAX_MEDIA_BYTES
            }
            Self::MediaChunk {
                transfer, bytes, ..
            } => transfer.0 != 0 && !bytes.is_empty() && bytes.len() <= CHUNK_BYTES,
            Self::MediaEnd { transfer } => transfer.0 != 0,
            Self::MediaRefused {
                authority,
                transfer,
                ..
            } => authority.is_valid() && transfer.0 != 0,
            Self::Client(ClientMessage::ReadMedia {
                authority,
                transfer,
                loop_id,
            }) => authority.is_valid() && transfer.0 != 0 && loop_id.is_valid(),
            Self::Client(ClientMessage::CancelMedia { authority }) => authority.is_valid(),
            Self::Client(ClientMessage::Attach {
                host,
                session,
                client,
                ..
            }) => host.0 != 0 && session.0 != 0 && client.0 != 0,
            Self::Client(ClientMessage::Submit {
                authority,
                session_epoch,
                operation,
                command,
                ..
            }) => *session_epoch != 0 && operation_valid(authority, operation) && command.bounded(),
            Self::Client(ClientMessage::Shutdown {
                authority,
                operation,
            }) => operation_valid(authority, operation),
            Self::Client(
                ClientMessage::Ready { authority, .. } | ClientMessage::Detach { authority },
            ) => authority.is_valid(),
            Self::Attached { authority, host } => authority.is_valid() && host.0 != 0,
            Self::SnapshotBegin {
                authority,
                transfer,
                bytes,
                ..
            } => {
                authority.is_valid()
                    && transfer.0 != 0
                    && *bytes > 0
                    && *bytes as usize <= MAX_SNAPSHOT_BYTES
            }
            Self::SnapshotChunk {
                transfer, bytes, ..
            } => transfer.0 != 0 && !bytes.is_empty() && bytes.len() <= CHUNK_BYTES,
            Self::SnapshotEnd { transfer } => transfer.0 != 0,
            Self::Accepted {
                authority,
                operation,
            } => authority.is_valid() && operation.id.client.0 != 0 && operation.id.sequence != 0,
            Self::Finished { authority, result } => {
                authority.is_valid()
                    && result.id.client.0 != 0
                    && result.id.sequence != 0
                    && result.outcome.bounded()
            }
            Self::Detached { authority } => authority.is_valid(),
            Self::ShutdownComplete { authority, result } => {
                authority.is_valid()
                    && operation_valid(authority, &result.id)
                    && result.outcome.bounded()
            }
            Self::Rejected(_) => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AudioMediaChannel {
    pub samples: Arc<[f32]>,
    pub start_offset: i32,
    pub capture_alignment_frames: i32,
    pub preplay: u32,
}
pub type MidiMediaEvent = MidiEventState;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MidiMediaChannel {
    pub events: Arc<[MidiMediaEvent]>,
    pub content_revision: u64,
    pub start_offset: i32,
    pub preplay: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoopMedia {
    pub content_revision: u64,
    pub loop_id: LoopId,
    pub audio: Vec<AudioMediaChannel>,
    pub midi: Vec<MidiMediaChannel>,
}
impl LoopMedia {
    pub fn valid(&self) -> bool {
        if !self.loop_id.is_valid() || self.audio.len() > 16 || self.midi.len() > 16 {
            return false;
        }
        let mut bytes = 0usize;
        for channel in &self.audio {
            let Some(size) = channel
                .samples
                .len()
                .checked_mul(4)
                .and_then(|size| bytes.checked_add(size))
            else {
                return false;
            };
            bytes = size;
        }
        for channel in &self.midi {
            for event in channel.events.iter() {
                if event.data.len() > 1024 {
                    return false;
                }
                let Some(size) = bytes.checked_add(event.data.len() + 32) else {
                    return false;
                };
                bytes = size;
            }
        }
        bytes <= MAX_MEDIA_BYTES - 4096
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ManagementRequest {
    Status {
        version: u16,
        host: HostId,
        session: SessionId,
    },
    Shutdown {
        authority: Authority,
        operation: OperationId,
    },
    Result {
        authority: Authority,
        operation: OperationId,
    },
    ClaimShutdown {
        version: u16,
        host: HostId,
        session: SessionId,
        operation: OperationId,
    },
}
impl ManagementRequest {
    pub fn valid(&self) -> bool {
        match self {
            Self::Status { host, session, .. } => host.0 != 0 && session.0 != 0,
            Self::ClaimShutdown {
                host,
                session,
                operation,
                ..
            } => host.0 != 0 && session.0 != 0 && operation_valid(*operation),
            Self::Shutdown {
                authority,
                operation,
            }
            | Self::Result {
                authority,
                operation,
            } => {
                authority.is_valid()
                    && operation_valid(*operation)
                    && operation.client == authority.client
            }
        }
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ManagementReply {
    Status {
        version: u16,
        host: HostId,
        session: SessionId,
        ready: bool,
        busy: bool,
        closed: bool,
    },
    Refused(Refusal),
    Accepted {
        authority: Authority,
        operation: PendingOperation,
    },
    Finished {
        authority: Authority,
        result: OperationResult,
    },
}
impl ManagementReply {
    pub fn valid(&self) -> bool {
        match self {
            Self::Status { host, session, .. } => host.0 != 0 && session.0 != 0,
            Self::Refused(_) => true,
            Self::Accepted {
                authority,
                operation,
            } => {
                authority.is_valid()
                    && operation_valid(operation.id)
                    && operation.id.client == authority.client
            }
            Self::Finished { authority, result } => {
                authority.is_valid()
                    && operation_valid(result.id)
                    && result.id.client == authority.client
                    && result.outcome.bounded()
            }
        }
    }
}
