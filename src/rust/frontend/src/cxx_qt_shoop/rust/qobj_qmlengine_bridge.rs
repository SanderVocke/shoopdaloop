use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("<QQmlEngine>");
        type QQmlEngine;

        include!("<QObject>");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QQmlEngine]
        type QmlEngine = super::QmlEngineRust;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_raw.h");

        #[rust_name = "make_raw_qmlengine"]
        unsafe fn make_raw_with_one_arg(parent : *mut QObject) -> *mut QmlEngine;
    }
}

pub use ffi::QmlEngine;

pub struct QmlEngineRust {}

impl Default for QmlEngineRust {
    fn default() -> QmlEngineRust {
        QmlEngineRust {}
    }
}
