use cxx_qt;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qsharedpointer_qobject.h");
        type QSharedPointer_QObject = crate::cxx_qt_lib_shoop::qsharedpointer_qobject::QSharedPointer_QObject;

        include!("cxx-qt-lib-shoop/qvariant_qsharedpointer_qobject.h");

        #[rust_name = "qvariant_to_qsharedpointer_qobject"]
        fn qvariantToQSharedPointerQObject(variant: &QVariant) -> Result<UniquePtr<QSharedPointer_QObject>>;

        #[rust_name = "qsharedpointer_qobject_to_qvariant"]
        unsafe fn qSharedPointerQObjectToQVariant(obj : &QSharedPointer_QObject) -> Result<QVariant>;
    }
}

pub use ffi::QVariant;

pub fn qvariant_to_qsharedpointer_qobject(variant: &QVariant) -> Option<cxx::UniquePtr<ffi::QSharedPointer_QObject>> {
    let result = ffi::qvariant_to_qsharedpointer_qobject(variant);

    match result {
        Ok(ptr) => {
            if ptr.is_null() {
                None
            } else {
                Some(ptr)
            }
        },
        Err(err) => {
            println!("Failed to convert QVariant to QSharedPointer<QObject>: {:?}", err);
            None
        },
    }
}

pub fn qsharedpointer_qobject_to_qvariant(obj : &ffi::QSharedPointer_QObject) -> QVariant {
    unsafe {
        ffi::qsharedpointer_qobject_to_qvariant(obj).unwrap()
    }
}