#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qquickitem.h");
        type QQuickItem = cxx_qt_lib_shoop::qquickitem::QQuickItem;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickItem]
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
         include!("cxx-qt-lib-shoop/make_unique.h");
        #[rust_name = "make_unique_lua_engine"]
        fn make_unique() -> UniquePtr<LuaEngine>;

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

    impl cxx_qt::Constructor<(*mut QQuickItem,), NewArguments = (*mut QQuickItem,)> for LuaEngine {}
    impl cxx_qt::Constructor<(), NewArguments = ()> for LuaEngine {}
}

use std::collections::HashMap;

use crate::lua_engine::LuaEngine as WrappedLuaEngine;
use ffi::*;

pub struct LuaEngineRust {
    pub engine: Option<WrappedLuaEngine>,
}

impl Default for LuaEngineRust {
    fn default() -> Self {
        Self {
            engine: None,
        }
    }
}

impl cxx_qt::Constructor<(*mut QQuickItem,)> for LuaEngine {
    type BaseArguments = (*mut QQuickItem,); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (*mut QQuickItem,); // Will be passed to the "new" function

    fn route_arguments(
        args: (*mut QQuickItem,),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        (args, args, ())
    }

    fn new(_parent: (*mut QQuickItem,)) -> LuaEngineRust {
        LuaEngineRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LuaEngine::initialize_impl(self);
    }
}

impl cxx_qt::Constructor<()> for LuaEngine {
    type BaseArguments = (); // Will be passed to the base class constructor
    type InitializeArguments = (); // Will be passed to the "initialize" function
    type NewArguments = (); // Will be passed to the "new" function

    fn route_arguments(
        _args: (),
    ) -> (
        Self::NewArguments,
        Self::BaseArguments,
        Self::InitializeArguments,
    ) {
        ((), (), ())
    }

    fn new(_args: ()) -> LuaEngineRust {
        LuaEngineRust::default()
    }

    fn initialize(self: std::pin::Pin<&mut Self>, _: Self::InitializeArguments) {
        LuaEngine::initialize_impl(self);
    }
}
