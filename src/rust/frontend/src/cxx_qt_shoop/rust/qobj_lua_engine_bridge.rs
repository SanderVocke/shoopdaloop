#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type LuaEngine = super::LuaEngineRust;

        #[qinvokable]
        pub fn evaluate(
            self: &LuaEngine,
            code: QString,
            maybe_script_name: QVariant,
            sandboxed: bool,
        ) -> QVariant;

        #[qinvokable]
        pub fn execute(
            self: &LuaEngine,
            code: QString,
            maybe_script_name: QVariant,
            sandboxed: bool,
        );
    }

    unsafe extern "C++" {

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_type_lua_engine"]
        unsafe fn register_qml_type(
            inference_example: *mut LuaEngine,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

use crate::lua_engine::LuaEngine as WrappedLuaEngine;

pub struct LuaEngineRust {
    pub engine: WrappedLuaEngine,
}

impl Default for LuaEngineRust {
    fn default() -> Self {
        Self {
            engine: WrappedLuaEngine::default(),
        }
    }
}
