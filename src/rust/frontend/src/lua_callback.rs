use mlua;
use std::sync::Arc;

pub trait LuaCallback {
    fn call(
        &self,
        lua: &Arc<mlua::Lua>,
        args: mlua::MultiValue,
    ) -> Result<mlua::Value, anyhow::Error>;
}
