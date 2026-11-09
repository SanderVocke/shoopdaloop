use crate::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioRenderMode {
    #[default]
    Current,
    Chunks,
    ChunksDescriptors,
    Compact,
}

impl AudioRenderMode {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "current" => Ok(Self::Current),
            "chunks" => Ok(Self::Chunks),
            "chunks-descriptors" => Ok(Self::ChunksDescriptors),
            "compact" => Ok(Self::Compact),
            _ => Err(Error::InvalidConfig),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Chunks => "chunks",
            Self::ChunksDescriptors => "chunks-descriptors",
            Self::Compact => "compact",
        }
    }

    pub fn chunked(self) -> bool {
        self != Self::Current
    }
}
