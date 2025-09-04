use std::rc::Rc;
use mlua;

pub trait LuaCallback {
    fn call(&self, lua: &Rc<mlua::Lua>, args: mlua::MultiValue) -> Result<mlua::Value, anyhow::Error>;
}
