use cxx::UniquePtr;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        include!("cxx-qt-lib-shoop/qweakpointer_qobject.h");
        type QObject = crate::qobject::QObject;
        type QWeakPointer_QObject;

        include!("cxx-qt-lib-shoop/qsharedpointer_qobject.h");
        type QSharedPointer_QObject = crate::qsharedpointer_qobject::QSharedPointer_QObject;

        #[rust_name = "qweakpointer_qobject_to_strong"]
        pub unsafe fn qWeakPointerToStrong(
            ptr: &QWeakPointer_QObject,
        ) -> Result<UniquePtr<QSharedPointer_QObject>>;

        #[rust_name = "qweakpointer_qobject_from_strong"]
        pub unsafe fn qSharedPointerToWeak(
            ptr: &QSharedPointer_QObject,
        ) -> UniquePtr<QWeakPointer_QObject>;

        #[rust_name = "qweakpointer_qobject_copy"]
        pub unsafe fn qWeakPointerCopy(
            ptr: &QWeakPointer_QObject,
        ) -> Result<UniquePtr<QWeakPointer_QObject>>;
    }
}

pub use ffi::{QSharedPointer_QObject, QWeakPointer_QObject};

impl QWeakPointer_QObject {
    pub fn from_strong(ptr: &QSharedPointer_QObject) -> cxx::UniquePtr<QWeakPointer_QObject> {
        unsafe { ffi::qweakpointer_qobject_from_strong(ptr) }
    }

    pub fn copy(&self) -> Result<UniquePtr<QWeakPointer_QObject>, cxx::Exception> {
        unsafe { ffi::qweakpointer_qobject_copy(self) }
    }

    pub fn to_strong(&self) -> Result<UniquePtr<QSharedPointer_QObject>, cxx::Exception> {
        unsafe { ffi::qweakpointer_qobject_to_strong(self) }
    }
}
