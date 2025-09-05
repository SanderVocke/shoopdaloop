use crate::ffi;
use enum_iterator::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum BackendResult {
    Success = ffi::shoop_result_t_Success,
    Failure = ffi::shoop_result_t_Failure,
}
