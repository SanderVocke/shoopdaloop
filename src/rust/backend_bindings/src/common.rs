use crate::ffi;
use crate::integer_enum;

integer_enum! {
    pub enum BackendResult {
        Success = ffi::shoop_result_t_Success,
        Failure = ffi::shoop_result_t_Failure,
    }
}
