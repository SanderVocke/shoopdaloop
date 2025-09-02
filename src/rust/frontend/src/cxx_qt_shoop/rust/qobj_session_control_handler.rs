use crate::cxx_qt_shoop::qobj_session_control_handler_bridge::SessionControlHandlerLuaTarget;
use crate::cxx_qt_shoop::{
    qobj_lua_engine_bridge::ffi::LuaEngine, qobj_session_control_handler_bridge::ffi::*,
};
use crate::lua_callback::LuaCallback;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qpointer::qpointer_from_qobject;
use cxx_qt_lib_shoop::{qobject::FromQObject, qpointer::qpointer_to_qobject};
use std::boxed::Box;
use std::pin::Pin;
use std::rc::Rc;
shoop_log_unit!("Frontend.SessionControlHandler");

impl SessionControlHandlerLuaTarget {
    pub fn install_on_lua_engine(&self, engine: *mut QObject) {

        // Macro for creating a boxed callback which is ready for registering in a
        // Lua engine. Each callback comes with its own struct implementation.
        macro_rules! create_callback {
            ($name:ident, $callback:expr) => {
                || -> Rc<Box<dyn LuaCallback>> {
                    struct $name {}
                    impl LuaCallback for $name {
                        fn call(
                            &self,
                            _: &mlua::Lua,
                            args: mlua::MultiValue,
                        ) -> Result<mlua::Value, anyhow::Error> {
                            $callback(args)
                        }
                    }
                    let cb: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new($name {}));
                    cb
                }()
            };
        }

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let engine = unsafe { LuaEngine::from_qobject_mut_ptr(engine)? };
            let wrapped_engine = engine
                .engine
                .as_ref()
                .ok_or(anyhow::anyhow!("Wrapped engine not initialized"))?;

            let mut lua_module: mlua::Table = wrapped_engine
                .lua
                .borrow()
                .lua
                .create_table()
                .map_err(|e| anyhow::anyhow!("Could not create table: {e}"))?;

            let loop_count =
                create_callback!(LoopCountCallback, |args: mlua::MultiValue| -> Result<
                    mlua::Value,
                    anyhow::Error,
                > {
                    todo!();
                });

            todo!();
        }() {
            error!("Could not install on Lua engine: {e}");
        }
    }

    fn loop_count(&self, selector: mlua::Value) -> i64 {
        self.select_loops(selector).len() as i64
    }

    fn select_loop_by_coords(&self, x: i64, y: i64) -> *mut QObject {
        todo!();
    }

    fn select_loops_by_coords(
        &self,
        coords: impl Iterator<Item = (i64, i64)>,
    ) -> Vec<*mut QObject> {
        {
            coords
                .map(|(x, y)| self.select_loop_by_coords(x, y))
                .collect()
        }
    }

    fn select_loops(&self, selector: mlua::Value) -> Vec<*mut QObject> {
        todo!();
    }
}

impl SessionControlHandler {
    pub fn install_on_lua_engine(self: Pin<&mut SessionControlHandler>, engine: *mut QObject) { todo!(); }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_session_control_handler(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
