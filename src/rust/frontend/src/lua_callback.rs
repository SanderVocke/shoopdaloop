use omnilua;
use std::sync::Arc;

pub trait LuaCallback {
    fn call(
        &self,
        lua: &Arc<omnilua::Lua>,
        args: omnilua::Variadic<omnilua::Value>,
    ) -> Result<omnilua::Value, anyhow::Error>;
}
