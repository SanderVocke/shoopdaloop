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
        })
    }
}

impl std::error::Error for Error {}
