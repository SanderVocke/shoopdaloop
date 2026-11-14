//! Messages exchanged over a carrier, and their lanes.

use crate::schema::{
    BulkChunk, BulkRead, Capabilities, DriverStatus, Limits, Outcome, Request, StateRecord,
    Telemetry,
};

pub use shoop_transport::Lane;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientMessage {
    Hello {
        version: u16,
    },
    Request {
        correlation: u64,
        request: Request,
    },
    /// Ask for a full-state record stream.
    Resync,
    ObserveSnapshots(bool),
    ConfigureSnapshots(crate::snapshot::SnapshotDelivery),
    RequestSnapshot {
        after: u32,
    },
    Bulk(BulkRead),
    Goodbye,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisconnectReason {
    Shutdown,
    Fault,
    ProtocolError,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineMessage {
    Snapshot(crate::snapshot::Chunk),
    SnapshotReply(crate::snapshot::Chunk),
    /// Ends a pull response, including when no new snapshot is available.
    SnapshotReady {
        sequence: u32,
    },
    SnapshotConfiguration {
        graph: u64,
        topology: u64,
    },
    Profiling(profiling_protocol::Record),
    Welcome {
        version: u16,
        capabilities: Capabilities,
        limits: Limits,
        driver: DriverStatus,
    },
    /// The client's version is not spoken; the connection ends.
    Rejected {
        version: u16,
    },
    Result {
        correlation: u64,
        outcome: Outcome,
    },
    Telemetry(Telemetry),
    State(StateRecord),
    Bulk(BulkChunk),
    LimitsChanged(Limits),
    Disconnected(DisconnectReason),
}

impl ClientMessage {
    pub fn lane(&self) -> Lane {
        match self {
            Self::Bulk(_) => Lane::Bulk,
            _ => Lane::Reliable,
        }
    }
}

impl EngineMessage {
    pub fn lane(&self) -> Lane {
        match self {
            Self::Profiling(_) => Lane::Profiling,
            Self::Telemetry(_) | Self::Snapshot(_) => Lane::Telemetry,
            Self::Bulk(_) => Lane::Bulk,
            _ => Lane::Reliable,
        }
    }
}
