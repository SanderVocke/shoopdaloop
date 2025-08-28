use std::{collections::HashMap, path::Path};

use common::logging::macros::*;
use mlua;
shoop_log_unit!("Frontend.LuaEngine");

pub struct LuaEngine {
    lua: mlua::Lua,
    preloaded_libs: HashMap<String, String>,
    run_sandboxed: Option<mlua::Function>,
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self {
            lua: mlua::Lua::new(),
            preloaded_libs: HashMap::new(),
            run_sandboxed: None,
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
        todo!();
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
        todo!();
    }

    pub fn initialize(
        &mut self,
        builtin_lib_dir: &Path,
        builtin_script_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        debug!("Initializing engine");
        self.register_print_functions()?;
        self.preload_libs(builtin_lib_dir)?;

        todo!();
    }
}
