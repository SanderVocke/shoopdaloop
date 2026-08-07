use anyhow::anyhow;
use mlua::Lua;

pub const KEYBOARD_SCRIPT: &str = include_str!("../../../lua/builtins/keyboard.lua");
pub const AKAI_APC_MINI_MK1_SCRIPT: &str =
    include_str!("../../../lua/builtins/akai_apc_mini_mk1.lua");

pub const BUILTIN_LIBRARIES: &[(&str, &str)] = &[
    (
        "shoop_control",
        include_str!("../../../lua/lib/shoop_control.lua"),
    ),
    (
        "shoop_coords",
        include_str!("../../../lua/lib/shoop_coords.lua"),
    ),
    (
        "shoop_format",
        include_str!("../../../lua/lib/shoop_format.lua"),
    ),
    (
        "shoop_helpers",
        include_str!("../../../lua/lib/shoop_helpers.lua"),
    ),
    (
        "shoop_midi",
        include_str!("../../../lua/lib/shoop_midi.lua"),
    ),
];

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    pub fn new() -> Self {
        Self { lua: Lua::new() }
    }

    pub fn evaluate_integer(&self, source: &str) -> anyhow::Result<i64> {
        self.lua
            .load(source)
            .eval()
            .map_err(|error| anyhow!("could not evaluate Lua integer expression: {error}"))
    }

    pub fn check_syntax(&self, name: &str, source: &str) -> anyhow::Result<()> {
        self.lua
            .load(source)
            .set_name(name)
            .into_function()
            .map_err(|error| anyhow!("could not compile Lua source {name}: {error}"))?;
        Ok(())
    }
}

impl Default for LuaRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_is_constructed_and_used_on_its_actor_thread() {
        let value = std::thread::spawn(|| {
            let runtime = LuaRuntime::new();
            runtime.evaluate_integer("return 20 + 22").unwrap()
        })
        .join()
        .unwrap();

        assert_eq!(value, 42);
    }

    #[test]
    fn production_lua_sources_are_embedded_and_syntactically_valid() {
        let runtime = LuaRuntime::new();
        runtime
            .check_syntax("keyboard.lua", KEYBOARD_SCRIPT)
            .unwrap();
        runtime
            .check_syntax("akai_apc_mini_mk1.lua", AKAI_APC_MINI_MK1_SCRIPT)
            .unwrap();
        for (name, source) in BUILTIN_LIBRARIES {
            runtime.check_syntax(name, source).unwrap();
        }
    }
}
