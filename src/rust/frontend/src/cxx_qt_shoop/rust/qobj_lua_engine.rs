use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::cxx_qt_shoop::qobj_lua_engine_bridge::{
    ffi::*, RustToLuaCallback, WrappedLuaCallbackRust,
};
use crate::init::GLOBAL_CONFIG;
use crate::lua_conversions::{FromLuaExtended, IntoLuaExtended};
use crate::lua_engine::LuaEngine as WrappedLuaEngine;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::invokable::invoke;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::qtimer::QTimer;
shoop_log_unit!("Frontend.LuaEngine");

impl LuaEngine {
    pub fn initialize_impl(mut self: std::pin::Pin<&mut Self>) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let lua_dir = GLOBAL_CONFIG
                .get()
                .as_ref()
                .ok_or(anyhow::anyhow!("Global config not initialized"))?
                .lua_dir
                .clone();

            let lua_dir_cloned = lua_dir.clone();
            let get_builtin_script = move |name: &str| -> Result<String, anyhow::Error> {
                let path = PathBuf::from(format!("{}/{}", lua_dir_cloned, name));
                if !path.exists() {
                    return Err(anyhow::anyhow!(
                        "Non-existent built-in script: {name}. Tried: {path:?}"
                    ));
                }
                let contents = std::fs::read_to_string(path)
                    .map_err(|e| anyhow::anyhow!("Failed to read builtin script file: {e}"))?;
                Ok(contents)
            };

            let mut libs: HashMap<String, String> = HashMap::default();
            for file in glob::glob(&format!("{}/lib/*.lua", lua_dir))
                .map_err(|e| anyhow::anyhow!("Could not glob for Lua libs: {e}"))?
            {
                let file = file?;
                let name = file
                    .file_stem()
                    .ok_or(anyhow::anyhow!("No stem for {file:?}"))?;
                let contents = std::fs::read_to_string(&file)?;
                libs.insert(name.to_string_lossy().to_string(), contents);
            }

            let mut engine = WrappedLuaEngine::default();
            engine.initialize(get_builtin_script, libs)?;
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.engine = Some(engine);

            trace!("Lua engine initialized");

            Ok(())
        }() {
            error!("Failed to initialize: {e}");
        }
    }

    pub fn evaluate(
        self: &LuaEngine,
        code: QString,
        maybe_script_name: QVariant,
        sandboxed: bool,
    ) -> QVariant {
        match || -> Result<QVariant, anyhow::Error> {
            let maybe_script_name: Option<String> =
                maybe_script_name.value::<QString>().map(|s| s.to_string());
            let lua_val = self
                .engine
                .as_ref()
                .ok_or(anyhow::anyhow!("engine not initialized"))?
                .evaluate::<mlua::Value>(
                    code.to_string().as_str(),
                    maybe_script_name.as_ref().map(|s| s.as_str()),
                    sandboxed,
                )?;
            let eng_impl = self.engine.as_ref().unwrap().lua.borrow();
            QVariant::from_lua(lua_val, &eng_impl.lua)
                .map_err(|e| anyhow::anyhow!("Failed to map to QVariant: {e}"))
        }() {
            Ok(value) => value,
            Err(e) => {
                error!("Could not evaluate: {e}");
                QVariant::default()
            }
        }
    }

    pub fn execute(self: &LuaEngine, code: QString, maybe_script_name: QVariant, sandboxed: bool) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let maybe_script_name: Option<String> =
                maybe_script_name.value::<QString>().map(|s| s.to_string());
            self.engine.as_ref().unwrap().execute(
                code.to_string().as_str(),
                maybe_script_name.as_ref().map(|s| s.as_str()),
                sandboxed,
            )
        }() {
            error!("Could not execute: {e}");
        }
    }

    pub fn make_unique() -> cxx::UniquePtr<Self> {
        make_unique_lua_engine()
    }

    pub fn ensure_engine_destroyed(self: std::pin::Pin<&mut Self>) {
        self.rust_mut().engine = None;
    }

    pub fn create_qt_to_lua_callback_fn(
        mut self: Pin<&mut LuaEngine>,
        name: QString,
        code: QString,
    ) -> *mut WrappedLuaCallback {
        match || -> Result<*mut WrappedLuaCallback, anyhow::Error> {
            let self_qobj = unsafe { self.as_mut().pin_mut_qobject_ptr() };
            let engine = self
                .engine
                .as_ref()
                .ok_or(anyhow::anyhow!("No engine set"))?;
            let cb = engine.evaluate::<mlua::Function>(
                code.to_string().as_str(),
                Some(name.to_string().as_str()),
                true,
            )?;
            let wrapped = WrappedLuaCallback::create_raw_with_parent(
                RustToLuaCallback {
                    callback: cb,
                    weak_lua: Arc::downgrade(&engine.lua.try_borrow()?.lua),
                },
                self_qobj,
            );

            debug!(
                "Created Qt to Lua callback \"{:?}\" with code:\n{:?}",
                name, code
            );

            Ok(wrapped)
        }() {
            Ok(cb) => cb,
            Err(e) => {
                error!("Could not create Lua callback: {e}");
                std::ptr::null_mut()
            }
        }
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_lua_engine(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl Drop for WrappedLuaCallbackRust {
    fn drop(&mut self) {
        debug!("Dropping wrapped Lua callback");
    }
}

impl WrappedLuaCallback {
    // Create a raw instance without a parent. It will configure
    // the timer to call-back to itself, call Lua and self-destruct.
    pub fn create_one_shot_timed(duration_ms: usize, callback: RustToLuaCallback) {
        unsafe {
            let obj: *mut WrappedLuaCallback = make_raw_wrapped_lua_callback();
            let mut obj_pin = std::pin::Pin::new_unchecked(&mut *obj);
            let qobj = obj_pin.as_mut().pin_mut_qobject_ptr();

            *obj_pin.callback.borrow_mut() = Some(callback);

            let timer: *mut QTimer = QTimer::make_raw_with_parent(qobj);
            let mut timer_pin = std::pin::Pin::new_unchecked(&mut *timer);
            let timer_qobj = QTimer::qobject_from_ptr(timer_pin.as_mut());

            connect_or_report(
                &mut *timer_qobj,
                "timeout()",
                &mut *qobj,
                "call_and_delete()",
                connection_types::QUEUED_CONNECTION,
            );

            timer_pin.as_mut().set_interval(duration_ms as i32);
            timer_pin.as_mut().set_single_shot(true);
            timer_pin.as_mut().start();
        }
    }

    // Create an unique ptr instance.
    pub fn create_unique_ptr(callback: RustToLuaCallback) -> cxx::UniquePtr<WrappedLuaCallback> {
        let mut cb = unsafe { make_unique_wrapped_lua_callback() };
        cb.as_mut().unwrap().rust_mut().callback = RefCell::new(Some(callback));
        cb
    }

    pub fn create_raw_with_parent(
        callback: RustToLuaCallback,
        parent: *mut QObject,
    ) -> *mut WrappedLuaCallback {
        let cb = unsafe { make_raw_wrapped_lua_callback_with_parent(parent) };
        let pin = unsafe { std::pin::Pin::new_unchecked(&mut *cb) };
        let mut rust_mut = pin.rust_mut();
        rust_mut.callback = RefCell::new(Some(callback));
        cb
    }

    pub fn call_impl(
        self: Pin<&mut WrappedLuaCallback>,
        override_stored_args: Option<&mlua::MultiValue>,
    ) {
        match || -> Result<QVariant, anyhow::Error> {
            let callback = self.callback.borrow();
            let callback = callback
                .as_ref()
                .ok_or(anyhow::anyhow!("No callback set"))?;
            let lua = callback
                .weak_lua
                .upgrade()
                .ok_or(anyhow::anyhow!("Lua went out of scope"))?;
            let lua = lua.as_ref();
            let args = override_stored_args.unwrap_or(&self.stored_arg);
            let rval = callback
                .callback
                .call::<mlua::Value>(args.clone())
                .map_err(|e| anyhow::anyhow!("failed to call callback: {e}"))?;
            let rval = QVariant::from_lua(rval, lua)
                .map_err(|e| anyhow::anyhow!("failed to convert return value: {e}"))?;

            Ok(rval)
        }() {
            Ok(rval) => rval,
            Err(e) => {
                error!("Could not call wrapped Lua callback: {e}");
                QVariant::default()
            }
        };
    }

    pub fn call(self: Pin<&mut WrappedLuaCallback>) {
        trace!("wrapped lua callback: call no args");
        self.call_impl(Some(&mlua::MultiValue::new()));
    }

    pub fn call_with_arg(mut self: Pin<&mut WrappedLuaCallback>, arg: QVariant) {
        trace!("wrapped lua callback: call with arg");
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let converted: mlua::Value;
            {
                let rust_mut = self.as_mut().rust_mut();
                let callback = rust_mut.callback.borrow();
                let callback = callback
                    .as_ref()
                    .ok_or(anyhow::anyhow!("No callback set"))?;
                let lua = callback
                    .weak_lua
                    .upgrade()
                    .ok_or(anyhow::anyhow!("Lua went out of scope"))?;
                let lua = lua.as_ref();
                converted = arg
                    .into_lua(lua)
                    .map_err(|e| anyhow::anyhow!("Could not convert arg to lua: {e}"))?;
            }
            self.as_mut()
                .call_impl(Some(&mlua::MultiValue::from_vec(vec![converted])));
            Ok(())
        }() {
            error!("Could not call wrapped lua callback: {e}")
        }
    }

    pub fn call_with_stored_arg(self: Pin<&mut WrappedLuaCallback>) {
        trace!("wrapped lua callback: call with stored arg");
        self.call_impl(None)
    }

    pub fn call_and_delete(mut self: Pin<&mut WrappedLuaCallback>) {
        trace!("wrapped lua callback: call and delete");
        self.as_mut().call_impl(Some(&mlua::MultiValue::new()));
        self.as_mut().delete_later();
    }

    pub fn delete_later(mut self: Pin<&mut WrappedLuaCallback>) {
        unsafe {
            let qobj = self.as_mut().pin_mut_qobject_ptr();
            if let Err(e) = invoke::<_, (), _>(
                &mut *qobj,
                "deleteLater()",
                connection_types::QUEUED_CONNECTION,
                &(),
            ) {
                error!("Failed to delete wrapped Lua callback: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use config::config::ShoopConfig;

    use super::*;

    #[test]
    fn test_basic_eval_expression_sandboxed() {
        GLOBAL_CONFIG.get_or_init(|| ShoopConfig::default());
        let eng = LuaEngine::make_unique();
        assert_eq!(
            eng.as_ref()
                .unwrap()
                .evaluate(QString::from("return 1 + 1"), QVariant::default(), true)
                .value::<i64>()
                .unwrap(),
            2
        );
    }

    #[test]
    fn test_basic_eval_expression_unsandboxed() {
        GLOBAL_CONFIG.get_or_init(|| ShoopConfig::default());
        let eng = LuaEngine::make_unique();
        assert_eq!(
            eng.as_ref()
                .unwrap()
                .evaluate(QString::from("return 1 + 1"), QVariant::default(), false)
                .value::<i64>()
                .unwrap(),
            2
        );
    }
}
