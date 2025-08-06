#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-shoop/ShoopQmlEngine.h");
        type ShoopQmlEngine;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = ShoopQmlEngine]
        type QmlEngine = super::QmlEngineRust;

        #[inherit]
        #[qinvokable]
        #[cxx_name = "setRootContextProperty"]
        unsafe fn set_root_context_property(
            self: Pin<&mut QmlEngine>,
            key: &QString,
            value: &QVariant,
        );

        #[inherit]
        #[cxx_name = "collectGarbage"]
        fn collect_garbage(self: Pin<&mut QmlEngine>);

        #[inherit]
        #[cxx_name = "addImportPath"]
        fn add_import_path(self: Pin<&mut QmlEngine>, path: &QString);

        #[inherit]
        #[cxx_name = "clearSingletons"]
        fn clear_singletons(self: Pin<&mut QmlEngine>);

        #[inherit]
        #[cxx_name = "closeRoot"]
        fn close_root(self: Pin<&mut QmlEngine>);

        #[inherit]
        #[cxx_name = "getRootWindow"]
        fn get_root_window(self: Pin<&mut QmlEngine>) -> Result<*mut QObject>;

        #[inherit]
        #[cxx_name = "deleteLater"]
        fn delete_later(self: Pin<&mut QmlEngine>);

        #[inherit]
        fn load(self: Pin<&mut QmlEngine>, path: &QString);

        #[inherit]
        #[qsignal]
        unsafe fn quit(self: Pin<&mut QmlEngine>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_raw.h");

        #[rust_name = "make_raw_qmlengine"]
        unsafe fn make_raw_with_one_arg(parent: *mut QObject) -> *mut QmlEngine;

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "qmlengine_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut QmlEngine) -> *mut QObject;

        #[rust_name = "qmlengine_qobject_from_ref"]
        fn qobjectFromRef(obj: &QmlEngine) -> &QObject;
    }
}

pub use ffi::QmlEngine;

use cxx_qt_lib_shoop::qobject::AsQObject;

pub struct QmlEngineRust {
    pub initialized: bool,
}

impl Default for QmlEngineRust {
    fn default() -> QmlEngineRust {
        QmlEngineRust { initialized: false }
    }
}

impl AsQObject for QmlEngine {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::qmlengine_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::qmlengine_qobject_from_ref(self) as *const ffi::QObject
    }
}
