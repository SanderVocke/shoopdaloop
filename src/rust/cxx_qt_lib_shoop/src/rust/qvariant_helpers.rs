use cxx_qt;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qvariant_helpers.h");

        #[rust_name = "qvariant_type_name"]
        fn qvariantTypeName(obj: &QVariant) -> Result<&str>;

        #[rust_name = "qvariant_type_id"]
        fn qvariantTypeId(obj: &QVariant) -> Result<i32>;
    }
}

pub use ffi::*;
