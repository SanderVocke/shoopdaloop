use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Mode a loop is in. Discriminants match `shoop_loop_mode_t` in
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum LoopMode {
    Unknown = 0,
    #[default]
    Stopped = 1,
    Playing = 2,
    Recording = 3,
    Replacing = 4,
    PlayingDryThroughWet = 5,
    RecordingDryIntoWet = 6,
}

impl LoopMode {
    /// Modes that advance the play position, i.e. that consume loop content.
    /// `Recording` is excluded: it grows length instead of advancing position.
    pub fn is_playing_mode(self) -> bool {
        matches!(
            self,
            Self::Playing
                | Self::Replacing
                | Self::PlayingDryThroughWet
                | Self::RecordingDryIntoWet
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[test]
    fn discriminants_match_c_abi() {
        check!(LoopMode::Unknown as u32 == 0);
        check!(LoopMode::Stopped as u32 == 1);
        check!(LoopMode::Playing as u32 == 2);
        check!(LoopMode::Recording as u32 == 3);
        check!(LoopMode::Replacing as u32 == 4);
        check!(LoopMode::PlayingDryThroughWet as u32 == 5);
        check!(LoopMode::RecordingDryIntoWet as u32 == 6);
    }

    #[test]
    fn playing_modes() {
        check!(LoopMode::Playing.is_playing_mode());
        check!(LoopMode::Replacing.is_playing_mode());
        check!(LoopMode::PlayingDryThroughWet.is_playing_mode());
        check!(LoopMode::RecordingDryIntoWet.is_playing_mode());
        check!(!LoopMode::Recording.is_playing_mode());
        check!(!LoopMode::Stopped.is_playing_mode());
        check!(!LoopMode::Unknown.is_playing_mode());
    }
}
