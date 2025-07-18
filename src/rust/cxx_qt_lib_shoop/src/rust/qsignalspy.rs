use cxx::{type_id, ExternType};
use cxx_qt;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qsignalspy.h");
        type QSignalSpy = super::QSignalSpyRust;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::qobject::QObject;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qsignalspy.h");
        #[rust_name = "qsignalspy_count"]
        unsafe fn qsignalspyCount(obj: &QSignalSpy) -> Result<i32>;

        #[rust_name = "qsignalspy_make_raw"]
        unsafe fn qsignalspyCreate(obj: *const QObject, signal: String) -> Result<*mut QSignalSpy>;
    }
}

#[repr(C)]
pub struct QSignalSpyRust {}

pub use ffi::QSignalSpy;

impl QSignalSpy {
    pub fn count(&self) -> Result<i32, cxx::Exception> {
        unsafe { ffi::qsignalspy_count(self) }
    }
}

pub fn make_raw(
    obj: *const ffi::QObject,
    signal: String,
) -> Result<*mut QSignalSpy, cxx::Exception> {
    unsafe { ffi::qsignalspy_make_raw(obj, signal) }
}

unsafe impl ExternType for QSignalSpy {
    type Id = type_id!("QSignalSpy");
    type Kind = cxx::kind::Opaque;
}
