use cxx;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qqmlerror.h");
        type QQmlError;
    }
}

pub use ffi::*;