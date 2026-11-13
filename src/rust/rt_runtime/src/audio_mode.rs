use crate::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioRenderMode {
    #[default]
    Current,
    Chunks,
    ChunksDescriptors,
}

impl AudioRenderMode {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "current" => Ok(Self::Current),
            "chunks" => Ok(Self::Chunks),
            "chunks-descriptors" => Ok(Self::ChunksDescriptors),
            _ => Err(Error::InvalidConfig),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Chunks => "chunks",
            Self::ChunksDescriptors => "chunks-descriptors",
        }
    }

    pub fn chunked(self) -> bool {
        matches!(self, Self::Chunks | Self::ChunksDescriptors)
    }
}
