//! Compact errors suitable for returning directly from rendering.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidConfig,
    Capacity,
    Exhausted,
    InvalidShape,
    InvalidFrame,
    InvalidId,
    InvalidEvent,
    Full,
    Disconnected,
    InvalidState,
    InvalidGraph,
    Cycle,
    PortType,
    StaleEpoch,
    Busy,
    InvalidParameter,
    InvalidOffset,
    Incomplete,
    Budget,
    StaleRevision,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidConfig => "invalid runtime configuration",
            Self::Capacity => "capacity exceeded",
            Self::Exhausted => "identity or frame space exhausted",
            Self::InvalidShape => "invalid buffer shape",
            Self::InvalidFrame => "unexpected frame position",
            Self::InvalidId => "invalid or stale identity",
            Self::InvalidEvent => "invalid event range or timestamp",
            Self::Full => "bounded storage is full",
            Self::Disconnected => "peer endpoint disconnected",
            Self::InvalidState => "operation is invalid in the current state",
            Self::InvalidGraph => "invalid graph descriptor or connection",
            Self::Cycle => "graph contains a cycle",
            Self::PortType => "incompatible port types",
            Self::StaleEpoch => "graph epoch no longer matches",
            Self::Busy => "object is still referenced by the active graph",
            Self::InvalidParameter => "unsupported parameter or value",
            Self::InvalidOffset => "transfer offset does not match the received prefix",
            Self::Incomplete => "resource content is incomplete",
            Self::Budget => "callback work budget exhausted",
            Self::StaleRevision => "resource revision no longer matches",
        })
    }
}

impl std::error::Error for Error {}
