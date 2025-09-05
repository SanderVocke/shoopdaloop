use crate::ffi;
use enum_iterator::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum ChannelMode {
    Disabled = ffi::shoop_channel_mode_t_ChannelMode_Disabled as i32,
    Direct = ffi::shoop_channel_mode_t_ChannelMode_Direct as i32,
    Dry = ffi::shoop_channel_mode_t_ChannelMode_Dry as i32,
    Wet = ffi::shoop_channel_mode_t_ChannelMode_Wet as i32,
}

impl Default for ChannelMode {
    fn default() -> Self {
        ChannelMode::Disabled
    }
}
