use cxx::UniquePtr;
use cxx_qt;

#[cxx_qt::bridge]
mod ffi {

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qjsvalue.h");
        type QJSValue;
        type QVariant = cxx_qt_lib::QVariant;
    }

    unsafe extern "C++Qt" {

        #[rust_name = "qvariant_call_as_callable_qjsvalue"]
        pub fn callQVariantAsCallableQJSValue(obj: &QVariant) -> Result<()>;
    }
}

pub use ffi::*;