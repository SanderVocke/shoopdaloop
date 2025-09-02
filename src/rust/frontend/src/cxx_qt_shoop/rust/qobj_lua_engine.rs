use std::collections::HashMap;
use std::path::PathBuf;

use crate::cxx_qt_shoop::qobj_lua_engine_bridge::ffi::*;
use crate::init::GLOBAL_CONFIG;
use crate::lua_conversions::FromLuaExtended;
use crate::lua_engine::LuaEngine as WrappedLuaEngine;
use common::logging::macros::*;
use cxx_qt::CxxQtType;
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
            let lua_val = self.engine.as_ref().unwrap().evaluate::<mlua::Value>(
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
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_lua_engine(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_eval_expression_sandboxed() {
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
