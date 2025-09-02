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
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::{Rc, Weak};
shoop_log_unit!("Frontend.SessionControlHandler");

impl SessionControlHandlerLuaTarget {
    pub fn install_on_lua_engine(&self, engine: *mut QObject) {
        struct SessionControlCallback {
            weak_target : Weak<RefCell<SessionControlHandlerLuaTarget>>,
            callable : Box<dyn Fn(&SessionControlHandlerLuaTarget, mlua::MultiValue) -> Result<mlua::Value, anyhow::Error>>,
        }

        impl LuaCallback for SessionControlCallback {
            fn call(&self, lua: &mlua::Lua, args: mlua::MultiValue) -> Result<mlua::Value, anyhow::Error> {
                let strong_self = self.weak_target.upgrade().ok_or(anyhow::anyhow!("Session control target went out of scope"))?;
                let strong_self = strong_self.borrow();
                (self.callable)(&strong_self, args)
            }
        }

        if let Err(e) = || -> Result<(), anyhow::Error> {
            let mut engine = unsafe { LuaEngine::from_qobject_mut_ptr(engine)? };
            let mut engine_rust = engine.as_mut().rust_mut();
            let mut wrapped_engine = engine_rust
                .engine
                .as_mut()
                .ok_or(anyhow::anyhow!("Wrapped engine not initialized"))?;

            let mut lua_module: mlua::Table = wrapped_engine
                .lua
                .borrow()
                .lua
                .create_table()
                .map_err(|e| anyhow::anyhow!("Could not create table: {e}"))?;

            // Macro for adding a callback method to the module repeatedly.
            macro_rules! add_callback {
                ($name: expr, $callback: expr) => {
                    {
                        let weak_clone = self.weak_self.clone();
                        let callback = SessionControlCallback {
                            weak_target: weak_clone,
                            callable: Box::new($callback),
                        };
                        let callback_rc : Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(callback));
                        let callback_lua = wrapped_engine.create_callback_fn($name, &callback_rc)?;
                        lua_module
                            .set($name, callback_lua)
                            .map_err(|e| anyhow::anyhow!("Could not set module callback {}: {e}", $name))?;
                    }
                }
            }

            // @shoop_lua_fn_docstring.start
            // shoop_control.loop_count(loop_selector) -> int
            // Count the amount of loops given by the selector.
            // @shoop_lua_fn_docstring.end
            add_callback!("loop_count", |_,_| {
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
