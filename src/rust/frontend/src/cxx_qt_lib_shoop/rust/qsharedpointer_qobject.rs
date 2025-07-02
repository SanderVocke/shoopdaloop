use cxx::UniquePtr;
use cxx_qt;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qsharedpointer_qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;
        type QSharedPointer_QObject;

        #[rust_name = "qsharedpointer_qobject_from_ptr_delete_later"]
        unsafe fn qSharedPointerFromPtrDeleteLater(ptr: *mut QObject) -> Result<UniquePtr<QSharedPointer_QObject>>;

        #[rust_name = "qsharedpointer_qobject_copy"]
        unsafe fn qSharedPointerCopy(other: &QSharedPointer_QObject) -> Result<UniquePtr<QSharedPointer_QObject>>;

        #[rust_name = "qsharedpointer_qobject_data"]
        unsafe fn qSharedPointerData(ptr: &QSharedPointer_QObject) -> Result<*mut QObject>;
    }
}

pub use ffi::QSharedPointer_QObject;

impl QSharedPointer_QObject {
    pub fn from_ptr_delete_later(ptr: *mut ffi::QObject) -> Result<UniquePtr<QSharedPointer_QObject>, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qobject_from_ptr_delete_later(ptr) }
    }

    pub fn copy(&self) -> Result<UniquePtr<QSharedPointer_QObject>, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qobject_copy(self) }
    }

    pub fn data(&self) -> Result<*mut ffi::QObject, cxx::Exception> {
        unsafe { ffi::qsharedpointer_qobject_data(self) }
    }
}