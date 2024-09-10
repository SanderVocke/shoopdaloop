use cxx_qt;

#[cxx_qt::bridge(cxx_file_stem="qvariant_helpers")]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib-shoop/qvariant_helpers.h");

        #[rust_name = "qvariant_type_name"]
        fn qvariantTypeName(obj: &QVariant) -> Result<&str>;
    }

    extern "RustQt" {
        #[qobject]
        type DummyQVariant = super::DummyQVariantRust;
    }
}

#[derive(Default)]
pub struct DummyQVariantRust {}

pub use ffi::QVariant;
pub use ffi::qvariant_type_name;