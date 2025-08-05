use cxx;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qqmlerrorlist.h");
        type QQmlErrorList;
    }
}

pub use ffi::*;