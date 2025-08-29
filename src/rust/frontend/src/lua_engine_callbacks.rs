use mlua;

pub trait LuaEngineCallback {
    fn call(&self, args: impl mlua::IntoLuaMulti)
        -> Result<impl mlua::FromLuaMulti, anyhow::Error>;
}
