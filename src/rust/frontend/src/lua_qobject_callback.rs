use std::marker::PhantomData;

use cxx_qt_lib_shoop::{
    invokable::{invoke, Invokable},
    qobject::{AsQObject, QObject},
};
use mlua;

use crate::lua_callback::LuaCallback;

struct LuaQObjectCallback<R, Args>
where
    Args: FromLuaMultiExtended,
    R: IntoLuaExtended,
{
    qobject: Box<dyn AsQObject>,
    method_signature: String,
    connection_type: u32,
    phantom_args: PhantomData<Args>,
    phantom_return: PhantomData<R>,
}

pub trait IntoLuaExtended {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value>;
}

impl<T: mlua::IntoLua> IntoLuaExtended for T {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        T::into_lua(self, lua)
    }
}

pub trait FromLuaMultiExtended: Sized {
    fn from_lua_multi(value: mlua::MultiValue, lua: &mlua::Lua) -> mlua::Result<Self>;
}

impl<T: mlua::FromLuaMulti> FromLuaMultiExtended for T {
    fn from_lua_multi(value: mlua::MultiValue, lua: &mlua::Lua) -> mlua::Result<Self> {
        T::from_lua_multi(value, lua)
    }
}

impl<Args: FromLuaMultiExtended, R: IntoLuaExtended> LuaCallback for LuaQObjectCallback<R, Args>
where
    QObject: Invokable<R, Args>,
{
    fn call(&self, lua: &mlua::Lua, args: mlua::MultiValue) -> Result<mlua::Value, anyhow::Error> {
        let args: Args = Args::from_lua_multi(args, lua)
            .map_err(|e| anyhow::anyhow!("Could not map args: {e}"))?;

        let result: R = unsafe {
            let qobject = self.qobject.qobject_ref() as *const QObject;
            let qobject = qobject as *mut QObject;
            invoke::<_, R, _>(
                qobject.as_mut().unwrap(),
                &self.method_signature,
                self.connection_type,
                &args,
            )?
        };

        let r_lua = R::into_lua(result, lua)
            .map_err(|e| anyhow::anyhow!("Could not map return value: {e}"))?;

        Ok(r_lua)
    }
}

pub fn create_lua_qobject_callback<R, Args>(
    obj: Box<dyn AsQObject>,
    signature: &str,
    connection_type: u32,
) -> impl LuaCallback
where
    Args: FromLuaMultiExtended,
    R: IntoLuaExtended,
    QObject: Invokable<R, Args>,
{
    LuaQObjectCallback {
        qobject: obj,
        method_signature: signature.to_string(),
        connection_type: connection_type,
        phantom_args: PhantomData,
        phantom_return: PhantomData,
    }
}
