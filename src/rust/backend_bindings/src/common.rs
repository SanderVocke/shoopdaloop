use crate::integer_enum;
use crate::ffi;

integer_enum! {
    pub enum BackendResult {
        Failure = ffi::shoop_result_t_Failure,
        Success = ffi::shoop_result_t_Success,
    }
}