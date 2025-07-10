use crate::ffi;
use crate::integer_enum;

integer_enum! {
    pub enum ChannelMode {
        Disabled = ffi::shoop_channel_mode_t_ChannelMode_Disabled,
        Direct = ffi::shoop_channel_mode_t_ChannelMode_Direct,
        Dry = ffi::shoop_channel_mode_t_ChannelMode_Dry,
        Wet = ffi::shoop_channel_mode_t_ChannelMode_Wet,
    }
}
