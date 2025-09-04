use std::{collections::HashMap, i64};

use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use cxx_qt_lib_shoop::qvariant_helpers::qvariantmap_to_qvariant;
use mlua::{self, FromLua};

pub trait IntoLuaExtended {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value>;
}

pub trait IntoLuaMultiExtended {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue>;
}

impl<T: mlua::IntoLuaMulti> IntoLuaMultiExtended for T {
    fn into_lua_multi(self, lua: &mlua::Lua) -> mlua::Result<mlua::MultiValue> {
        T::into_lua_multi(self, lua)
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
pub trait FromLuaExtended: Sized {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self>;
}

macro_rules! specific_builtin_impl {
    ($T:ty) => {
        impl FromLuaExtended for $T {
            fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
                <$T as mlua::FromLua>::from_lua(value, lua)
            }
        }

        impl IntoLuaExtended for $T {
            fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
                <$T as mlua::IntoLua>::into_lua(self, lua)
            }
        }
    };
}

specific_builtin_impl!(i32);
specific_builtin_impl!(i64);
specific_builtin_impl!(String);
specific_builtin_impl!(f32);
specific_builtin_impl!(f64);
specific_builtin_impl!(bool);

impl FromLuaExtended for QString {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        <String as mlua::FromLua>::from_lua(value, lua).map(|s| QString::from(s))
    }
}

impl FromLuaExtended for QVariant {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        let type_name = value.type_name();

        if value.is_table() {
            return Ok(
                qvariantmap_to_qvariant(&QMap::<QMapPair_QString_QVariant>::from_lua(value, lua)?)
                    .map_err(|e| mlua::Error::FromLuaConversionError {
                        from: type_name,
                        to: "QVariant".to_string(),
                        message: Some(format!("Failed to convert to QVariantMap: {e}")),
                    })?,
            );
        }

        match value {
            mlua::Value::Nil => Ok(QVariant::default()),
            mlua::Value::Boolean(v) => Ok(QVariant::from(&v)),
            mlua::Value::Integer(v) => Ok(QVariant::from(&v)),
            mlua::Value::Number(v) => Ok(QVariant::from(&v)),
            mlua::Value::String(v) => Ok(QVariant::from(&QString::from(&v.to_string_lossy()))),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: type_name,
                to: "QVariant".to_string(),
                message: Some("Unsupported".to_string()),
            }),
        }
    }
}

impl FromLuaExtended for QList<QVariant> {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        let mut rval: QList<QVariant> = QList::default();
        let mut minval: i64 = i64::MAX;
        if !value.is_table() {
            return Err(mlua::Error::FromLuaConversionError {
                from: "non-table",
                to: "QList<QVariant>".to_string(),
                message: Some("value is not a table".to_string()),
            });
        }
        let mut hashmap: HashMap<i64, mlua::Value> = HashMap::from_lua(value, lua)?;

        for (key, _) in hashmap.iter() {
            minval = std::cmp::min(minval, *key);
        }

        for i in minval..minval + (hashmap.len() as i64) {
            let value = hashmap
                .remove(&i)
                .ok_or(mlua::Error::FromLuaConversionError {
                    from: "table",
                    to: "QList<QVariant>".to_string(),
                    message: Some(format!(
                        "Missing list index {i} in table starting at {minval}"
                    )),
                })?;
            let variant = QVariant::from_lua(value, lua)?;
            rval.append(variant);
        }

        Ok(rval)
    }
}

impl FromLuaExtended for QMap<QMapPair_QString_QVariant> {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        let mut rval: QMap<QMapPair_QString_QVariant> = QMap::default();
        if !value.is_table() {
            return Err(mlua::Error::FromLuaConversionError {
                from: "non-table",
                to: "QMap<QString,QVariant>".to_string(),
                message: Some("value is not a table".to_string()),
            });
        }

        value
            .as_table()
            .unwrap()
            .for_each(|key, value| -> mlua::Result<()> {
                let key: QString = QString::from(<String as mlua::FromLua>::from_lua(key, lua)?);
                let variant = QVariant::from_lua(value, lua)?;
                rval.insert(key, variant);
                Ok(())
            })?;

        Ok(rval)
    }
}
