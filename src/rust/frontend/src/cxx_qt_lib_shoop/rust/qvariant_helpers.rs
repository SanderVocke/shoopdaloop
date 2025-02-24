use cxx_qt;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib-shoop/qvariant_helpers.h");

        #[rust_name = "qvariant_type_name"]
        fn qvariantTypeName(obj: &QVariant) -> Result<&str>;

        #[rust_name = "qvariant_to_qobject_ptr"]
        fn qvariantToQObjectPtr(variant: &QVariant) -> Result<*mut QObject>;
    }
}

pub use ffi::QVariant;
pub use ffi::qvariant_type_name;

pub fn qvariant_to_qobject_ptr(variant: &QVariant) -> Option<*mut ffi::QObject> {
    let result = ffi::qvariant_to_qobject_ptr(variant);

    match result {
        Ok(ptr) => {
            if ptr.is_null() {
                None
            } else {
                Some(ptr)
            }
        },
        Err(err) => {
            println!("Failed to convert QVariant to QObject pointer: {:?}", err);
            None
        },
    }
}