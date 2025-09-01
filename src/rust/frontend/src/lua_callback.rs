use mlua;

pub trait LuaCallback {
    fn call(&self, lua: &mlua::Lua, args: mlua::MultiValue) -> Result<mlua::Value, anyhow::Error>;
}
