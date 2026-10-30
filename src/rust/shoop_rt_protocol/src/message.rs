//! Messages exchanged over a carrier, and their lanes.

use crate::schema::{
    BulkChunk, BulkRead, Capabilities, DriverStatus, Limits, Outcome, Request, StateRecord,
    Telemetry,
};

/// Capacity class of a message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lane {
    /// Requests, results, lifecycle and state: never dropped.
    Reliable,
    /// Latest values: a newer record supersedes an undelivered older one.
    Telemetry,
    /// Chunked reads, limited by bulk credits.
    Bulk,
}

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
            Self::Telemetry(_) => Lane::Telemetry,
            Self::Bulk(_) => Lane::Bulk,
            _ => Lane::Reliable,
        }
    }
}
