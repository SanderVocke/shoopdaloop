use cxx;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qpointer.h");
        type QPointerQObject;
        type QObject;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        #[rust_name=qpointer_to_qobject]
        pub unsafe fn qpointerToQObject(p: &QPointerQObject) -> *mut QObject;

        #[rust_name=qpointer_from_qobject]
        pub unsafe fn qpointerFromQObject(obj: *mut QObject) -> UniquePtr<QPointerQObject>;

        #[rust_name=qvariant_from_qpointer]
        pub unsafe fn qvariantFromQPointer(p: &QPointerQObject) -> QVariant;
    }
}

pub use ffi::*;
