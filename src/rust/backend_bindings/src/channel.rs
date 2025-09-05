use crate::ffi;
use enum_iterator::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum ChannelMode {
    Disabled = ffi::shoop_channel_mode_t_ChannelMode_Disabled,
    Direct = ffi::shoop_channel_mode_t_ChannelMode_Direct,
    Dry = ffi::shoop_channel_mode_t_ChannelMode_Dry,
    Wet = ffi::shoop_channel_mode_t_ChannelMode_Wet,
}

impl Default for ChannelMode {
    fn default() -> Self {
        ChannelMode::Disabled
    }
}
