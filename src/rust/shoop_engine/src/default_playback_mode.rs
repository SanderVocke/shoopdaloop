use crate::LoopMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DefaultPlaybackMode {
    #[default]
    Regular,
    DryThroughWet,
}

impl DefaultPlaybackMode {
    pub const fn loop_mode(self) -> LoopMode {
        match self {
            Self::Regular => LoopMode::Playing,
            Self::DryThroughWet => LoopMode::PlayingDryThroughWet,
        }
    }
}
