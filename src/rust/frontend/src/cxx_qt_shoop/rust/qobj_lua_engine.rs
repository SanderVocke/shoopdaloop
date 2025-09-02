use crate::cxx_qt_shoop::qobj_lua_engine_bridge::ffi::*;
use crate::lua_engine::LuaEngine as WrappedLuaEngine;

impl LuaEngine {
        pub fn evaluate(
            self: &LuaEngine,
            code: QString,
            maybe_script_name: QVariant,
            sandboxed: bool,
        ) -> QVariant { todo!(); }

        pub fn execute(
            self: &LuaEngine,
            code: QString,
            maybe_script_name: QVariant,
            sandboxed: bool,
        ) { todo!(); }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_lua_engine(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}