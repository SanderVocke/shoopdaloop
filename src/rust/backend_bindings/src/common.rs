use crate::integer_enum;
use crate::ffi;

integer_enum! {
    pub enum BackendResult {
        Success = ffi::shoop_result_t_Success,
        Failure = ffi::shoop_result_t_Failure,
    }
}