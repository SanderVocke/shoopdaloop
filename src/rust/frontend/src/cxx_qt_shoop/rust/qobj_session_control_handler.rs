use std::pin::Pin;

use cxx_qt_lib_shoop::qpointer::qpointer_to_qobject;

use crate::cxx_qt_shoop::{qobj_lua_engine_bridge::ffi::LuaEngine, qobj_session_control_handler_bridge::ffi::*};

impl SessionControlHandler {
    pub fn install_on_lua_engine(self: Pin<&mut SessionControlHandler>, engine: *mut QObject) {
        let engine = LuaEngine::from_qobject_mut(engine);

        todo!();
    }

    fn select_loop_by_coords(
        self: Pin<&mut SessionControlHandler>,
        x: i64,
        y: i64,
    ) -> *mut QObject {
        if !self.loop_references.contains_key(&(x, y)) {
            std::ptr::null_mut()
        } else {
            unsafe {
                qpointer_to_qobject(self.loop_references.get(&(x, y)).unwrap().as_ref().unwrap())
            }
        }
    }

    fn select_loops_by_coords(
        mut self: Pin<&mut SessionControlHandler>,
        coords: impl Iterator<Item = (i64, i64)>,
    ) -> Vec<*mut QObject> {
        {
            coords
                .map(|(x, y)| self.as_mut().select_loop_by_coords(x, y))
                .collect()
        }
    }

    fn select_loops(self: Pin<&mut SessionControlHandler>, selector: mlua::Value) {
        todo!();
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_session_control_handler(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
