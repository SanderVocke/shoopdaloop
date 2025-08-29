use std::{collections::HashMap, path::Path};

use common::logging::macros::*;
use mlua;
shoop_log_unit!("Frontend.LuaEngine");

pub struct LuaEngine {
    lua: mlua::Lua,
    preloaded_libs: HashMap<String, String>,
    run_sandboxed: Option<mlua::Function>,
    require: Option<mlua::Function>,
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self {
            lua: mlua::Lua::new(),
            preloaded_libs: HashMap::new(),
            run_sandboxed: None,
            require: None,
        }
    }
}

impl LuaEngine {
    fn register_print_functions(&mut self) -> Result<(), anyhow::Error> {
        let globals = self.lua.globals();

        globals
            .set(
                "__shoop_print",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        info!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_trace",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        trace!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_debug",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        debug!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_info",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        info!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_warning",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        warn!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;
        globals
            .set(
                "__shoop_print_error",
                self.lua
                    .create_function(|_, (msg,): (String,)| {
                        error!("{msg}");
                        Ok(())
                    })
                    .map_err(|e| anyhow::anyhow!("Failed to create print function: {e}"))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to register print function: {e}"))?;

        Ok(())
    }

    fn preload_libs(&mut self, builtin_lib_dir: &Path) -> Result<(), anyhow::Error> {
        for lib in glob::glob(format!("{}/*.lua", builtin_lib_dir.to_string_lossy()))? {
            let lib = lib?;
            debug!("Preloading library: {lib:?}");
            let content = std::fs::read_to_string(lib)?;
            self.preloaded_libs
                .insert(lib.to_string_lossy().to_string(), content);
        }
        Ok(())
    }

    pub fn execute_builtin_script(
        &mut self,
        script_subpath: &str,
        sandboxed: bool,
    ) -> Result<(), anyhow::Error> {
        todo!();
    }

    pub fn prepare_sandbox(&mut self) -> Result<(), anyhow::Error> {
        self.execute_builtin_script("system/sandbox.lua", false)?;
        let globals = self.lua.globals();
        self.run_sandboxed = Some(
            globals
                .get("__shoop_run_sandboxed")
                .map_err(|e| anyhow::anyhow!("failed to get __shoop_run_sandboxed: {e}"))?,
        );
        self.require = Some(
            self.lua
                .create_function(|_, (name,): (String,)| Ok(self.require(name)?))
                .map_err(|e| anyhow::anyhow!("Could not create require function: {e}"))?,
        );
    }

    pub fn require(&self, name: &str) -> Result<(), anyhow::Error> {
        if name == "shoop_control" {
            // Special built-in library which is always loaded
            return self.evaluate::<mlua::FromLua>("__shoop_control_interface", None, false);
        }
        if !self.preloaded_libs.contains_key(name) {
            return Err(anyhow::anyhow!("Cannot require unloaded library: {name}"));
        }
        Ok(self
            .lua
            .load(self.preloaded_libs.get(name))
            .eval::<mlua::FromLua>()?);
    }

    pub fn evaluate<R>(
        &self,
        code: &str,
        script_name: Option<&str>,
        sandboxed: bool,
    ) -> Result<(), anyhow::Error> {
        trace!(
            "evaluate (sandboxed: {sandboxed}, name: {}:\n{code}",
            script_name.unwrap_or("(none)")
        );
        let result: R = match sandboxed {
            true => self
                .run_sandboxed
                .ok_or(anyhow::anyhow!("no sandbox function set"))?
                .call::<R>((code,))?,
            false => self.lua.load(code).eval::<R>()?,
        };
        Ok(result)
    }

    pub fn execute(
        &self,
        code: &str,
        script_name: Option<&str>,
        sandboxed: bool,
    ) -> Result<(), anyhow::Error> {
        self.evaluate::<()>(code, script_name, sandboxed)
    }

    pub fn set_toplevel(
        &mut self,
        key: &str,
        value: impl mlua::IntoLua,
    ) -> Result<(), anyhow::Error> {
        let registrar = self
            .lua
            .load(format!("return function(value) {} = value end", key));
        registrar.call((value,))()
    }

    pub fn initialize(
        &mut self,
        builtin_lib_dir: &Path,
        builtin_script_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        debug!("Initializing engine");
        self.register_print_functions()?;
        self.preload_libs(builtin_lib_dir)?;

        let globals = self.lua.globals();
        globals.set(
            "__shoop_builtin_libs_path",
            format!(
                "{}/?.lua",
                builtin_lib_dir.to_str().unwrap().replace("\\", "\\\\")
            ),
        );
        self.lua
            .load("package.path = package.path .. \";\" .. __shoop_builtin_libs_path")
            .set_name("extend package path")
            .exec()?;

        self.set_toplevel("require", self.require);

        todo!();
    }
}
