use crate::integer_enum;
use crate::ffi;

integer_enum! {
    pub enum BackendResult {
        Success = ffi::shoop_result_t_Success,
        Failure = ffi::shoop_result_t_Failure,
    }
}

integer_enum! {
    pub enum ChannelMode {
        Disabled = ffi::shoop_channel_mode_t_ChannelMode_Disabled,
        Direct = ffi::shoop_channel_mode_t_ChannelMode_Direct,
        Dry = ffi::shoop_channel_mode_t_ChannelMode_Dry,
        Wet = ffi::shoop_channel_mode_t_ChannelMode_Wet,
    }
}