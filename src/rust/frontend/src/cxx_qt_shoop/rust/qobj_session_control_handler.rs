use crate::cxx_qt_shoop::qobj_session_control_handler_bridge::SessionControlHandlerLuaTarget;
use crate::cxx_qt_shoop::{
    qobj_lua_engine_bridge::ffi::LuaEngine, qobj_session_control_handler_bridge::ffi::*,
};
use crate::lua_callback::LuaCallback;
use crate::lua_engine::LuaEngineImpl;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QList;
use cxx_qt_lib_shoop::qpointer::{qpointer_from_qobject, QPointerQObject};
use cxx_qt_lib_shoop::qvariant_helpers::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr, qvariant_to_qvariantlist, qvariantlist_to_qvariant};
use cxx_qt_lib_shoop::{qobject::FromQObject, qpointer::qpointer_to_qobject};
use mlua::FromLua;
use std::boxed::Box;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::rc::{Rc, Weak};
shoop_log_unit!("Frontend.SessionControlHandler");

fn as_coords(val: &mlua::Value) -> Option<(i64, i64)> {
    if let mlua::Value::Table(table) = val {
        let len = table.len().unwrap_or(0);
        if len == 2
            && table
                .sequence_values::<mlua::Value>()
                .filter(|v| v.is_ok())
                .count()
                == 2
            && table
                .sequence_values::<mlua::Value>()
                .filter(|v| v.is_ok())
                .fold(true, |acc, v| {
                    acc && v.unwrap_or(mlua::Value::Nil).is_integer()
                })
        {
            let mut iter = table.sequence_values::<mlua::Value>();
            let first = iter.next().unwrap().unwrap_or(mlua::Value::Nil);
            let second = iter.next().unwrap().unwrap_or(mlua::Value::Nil);
            return Some((first.as_integer().unwrap(), second.as_integer().unwrap()));
        }
    }
    None
}

fn as_multi_coords(val: &mlua::Value) -> Option<Vec<(i64, i64)>> {
    if let mlua::Value::Table(table) = val {
        let len = table.len().unwrap_or(0);
        if len
            == table
                .sequence_values::<mlua::Value>()
                .filter(|v| v.is_ok())
                .count() as i64
        {
            let as_coords: Vec<Option<(i64, i64)>> = table
                .sequence_values::<mlua::Value>()
                .map(|v| match v {
                    Ok(v) => as_coords(&v),
                    Err(_) => None,
                })
                .collect();
            if as_coords.iter().fold(true, |acc, v| acc && v.is_some()) {
                return Some(as_coords.iter().map(|v| v.unwrap()).collect());
            }
        }
    }
    None
}

impl SessionControlHandlerLuaTarget {
    pub fn install_on_lua_engine(&mut self, engine: *mut QObject) {
        struct SessionControlCallback {
            weak_target: Weak<RefCell<SessionControlHandlerLuaTarget>>,
            weak_engine: Weak<RefCell<LuaEngineImpl>>,
            callable: Box<
                dyn Fn(
                    &SessionControlHandlerLuaTarget,
                    &mlua::Lua,
                    mlua::MultiValue,
                ) -> Result<mlua::Value, anyhow::Error>,
            >,
        }

        impl LuaCallback for SessionControlCallback {
            fn call(
                &self,
                lua: &mlua::Lua,
                args: mlua::MultiValue,
            ) -> Result<mlua::Value, anyhow::Error> {
                let strong_self = self
                    .weak_target
                    .upgrade()
                    .ok_or(anyhow::anyhow!("Session control target went out of scope"))?;
                let strong_self = strong_self.borrow();
                let strong_engine = self
                    .weak_engine
                    .upgrade()
                    .ok_or(anyhow::anyhow!("Lua engine went out of scope"))?;
                let strong_engine = strong_engine.borrow();
                (self.callable)(&strong_self, &strong_engine.lua, args)
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
                ($name: expr, $callback: expr) => {{
                    let weak_clone = self.weak_self.clone();
                    let callback = SessionControlCallback {
                        weak_engine: Rc::downgrade(&wrapped_engine.lua),
                        weak_target: weak_clone,
                        callable: Box::new($callback),
                    };
                    let callback_rc: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(callback));
                    let callback_lua = wrapped_engine.create_callback_fn($name, &callback_rc)?;
                    lua_module.set($name, callback_lua).map_err(|e| {
                        anyhow::anyhow!("Could not set module callback {}: {e}", $name)
                    })?;
                    self.callbacks.push(callback_rc);
                }};
            }

            add_callback!("loop_count", |self_ref, lua, args| {
                self_ref.loop_count(lua, args)
            });

            wrapped_engine.set_toplevel("__shoop_control", lua_module)?;

            Ok(())
        }() {
            error!("Could not install on Lua engine: {e}");
        }
    }

    // @shoop_lua_fn_docstring.start
    // shoop_control.loop_count(loop_selector) -> int
    // Count the amount of loops given by the selector.
    // @shoop_lua_fn_docstring.end
    fn loop_count(
        &self,
        lua: &mlua::Lua,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("Expected 1 argument, got {}", args.len()));
        }
        Ok(mlua::Value::Integer(
            self.select_loops(lua, args.get(0).unwrap_or(&mlua::Value::Nil))?
                .len() as i64,
        ))
    }

    fn select_loop_by_coords(&self, x: i64, y: i64) -> Option<*mut QObject> {
        self.loop_references
            .get(&(x, y))
            .map(|r| unsafe {
                match r.as_ref() {
                    Some(r) => match qpointer_to_qobject(r) {
                        v if v == std::ptr::null_mut() => None,
                        other => Some(other),
                    },
                    None => None,
                }
            })
            .unwrap_or(None)
    }

    fn select_loops_by_coords(
        &self,
        coords: impl Iterator<Item = (i64, i64)>,
    ) -> Vec<*mut QObject> {
        {
            coords
                .map(|(x, y)| {
                    self.select_loop_by_coords(x, y)
                        .unwrap_or(std::ptr::null_mut())
                })
                .collect()
        }
    }

    fn select_loops(
        &self,
        lua: &mlua::Lua,
        selector: &mlua::Value,
    ) -> Result<Vec<*mut QObject>, anyhow::Error> {
        match selector {
            mlua::Value::Nil => Ok(vec![]),
            mlua::Value::Table(_) => {
                if let Some(coords) = as_coords(selector) {
                    match self.select_loop_by_coords(coords.0, coords.1) {
                        Some(ptr) => Ok(vec![ptr]),
                        None => Ok(vec![]),
                    }
                } else if let Some(multi_coords) = as_multi_coords(selector) {
                    Ok(self.select_loops_by_coords(multi_coords.into_iter()))
                } else {
                    todo!()
                }
            }
            _ => todo!(),
        }
    }
}

impl SessionControlHandler {
    pub fn install_on_lua_engine(self: Pin<&mut SessionControlHandler>, engine: *mut QObject) {
        self.lua_target.borrow_mut().install_on_lua_engine(engine);
    }

    pub fn set_loop_references(
        self: Pin<&mut SessionControlHandler>,
        loop_references: QList_QVariant,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let mut result: BTreeMap<(i64, i64), cxx::UniquePtr<QPointerQObject>> =
                BTreeMap::default();

            for (x, outer) in loop_references.iter().enumerate() {
                let list = qvariant_to_qvariantlist(&outer)?;
                for (y, l) in list.iter().enumerate() {
                    let qobject = qvariant_to_qobject_ptr(&l)?;
                    result.insert((x as i64, y as i64), unsafe {
                        qpointer_from_qobject(qobject)
                    });
                }
            }

            self.lua_target.borrow_mut().loop_references = result;

            Ok(())
        }() {
            error!("Could not set loop references: {e}");
        }
    }

    pub fn get_loop_references(self: &SessionControlHandler) -> QList_QVariant {
        let mut vec : Vec<Vec<*mut QObject>> = Vec::default();
        
        for ((x, y), qpointer) in self.lua_target.borrow().loop_references.iter() {
            vec.resize(std::cmp::max(vec.len(), *x as usize + 1), Vec::default());
            vec[*x as usize].resize(std::cmp::max(vec[*x as usize].len(), *y as usize + 1), std::ptr::null_mut());
            vec[*x as usize][*y as usize] = unsafe { qpointer_to_qobject(qpointer.as_ref().unwrap()) };
        }

        let mut result : QList_QVariant = QList::default();
        for sublist in vec {
            let mut q_sublist : QList_QVariant = QList::default();
            for qobject in sublist {
                q_sublist.append(qobject_ptr_to_qvariant(&qobject).unwrap_or(QVariant::default()));
            }
            result.append(qvariantlist_to_qvariant(&q_sublist).unwrap_or(QVariant::default()));
        }

        result
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_session_control_handler(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
