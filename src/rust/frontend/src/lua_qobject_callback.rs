use std::marker::PhantomData;

use cxx_qt_lib_shoop::{
    invokable::{invoke, Invokable},
    qobject::QObject,
    qpointer::{qpointer_to_qobject, QPointerQObject},
};
use mlua;

use crate::lua_callback::LuaCallback;

pub trait LuaQObjectCallbackTarget {
    fn lua_qobject_target(&self) -> Result<*mut QObject, anyhow::Error>;
}

impl LuaQObjectCallbackTarget for cxx::UniquePtr<QPointerQObject> {
    fn lua_qobject_target(&self) -> Result<*mut QObject, anyhow::Error> {
        match unsafe { qpointer_to_qobject(self) } {
            qobj if qobj.is_null() => Err(anyhow::anyhow!("QPointerQObject is null")),
            qobj => Ok(qobj),
        }
    }
}

struct LuaQObjectCallback<Q, R, Args>
where
    Args: FromLuaMultiExtended,
    R: IntoLuaExtended,
    Q: LuaQObjectCallbackTarget,
{
    qobject: Q,
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

impl<Args: FromLuaMultiExtended, R: IntoLuaExtended, Q: LuaQObjectCallbackTarget> LuaCallback
    for LuaQObjectCallback<Q, R, Args>
where
    QObject: Invokable<R, Args>,
{
    fn call(&self, lua: &mlua::Lua, args: mlua::MultiValue) -> Result<mlua::Value, anyhow::Error> {
        let args: Args = Args::from_lua_multi(args, lua)
            .map_err(|e| anyhow::anyhow!("Could not map args: {e}"))?;

        let result: R = unsafe {
            let qobject = self.qobject.lua_qobject_target()?;
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

pub fn create_lua_qobject_callback_qpointer<R, Args>(
    obj: cxx::UniquePtr<QPointerQObject>,
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, rc::Rc};

    use cxx_qt_lib_shoop::{connection_types, qobject::AsQObject, qpointer::qpointer_from_qobject};

    use super::*;
    use crate::{
        cxx_qt_shoop::test::qobj_generic_test_item::GenericTestItem,
        lua_engine::{LuaEngine, LuaScope},
    };

    #[test]
    fn test_basic_qobject_invokable() {
        let mut eng = LuaEngine::default();
        eng.initialize(|_| Err(anyhow::anyhow!("n/a")), HashMap::default())
            .unwrap();

        let mut obj = GenericTestItem::make_unique();
        let qobj = obj.pin_mut();
        let qobj = unsafe { qobj.pin_mut_qobject_ptr() };
        let qpointer = unsafe { qpointer_from_qobject(qobj) };

        let callback = create_lua_qobject_callback_qpointer::<i32, (i32, i32)>(
            qpointer,
            "add(::std::int32_t,::std::int32_t)",
            connection_types::DIRECT_CONNECTION,
        );
        let callback: Rc<Box<dyn LuaCallback>> = Rc::new(Box::new(callback));

        eng.register_callback("qobj_add", LuaScope::Sandboxed, &callback)
            .unwrap();

        assert_eq!(
            eng.evaluate::<i32>("return qobj_add(1, 2)", None, true).unwrap(),
            3
        );
    }
}
