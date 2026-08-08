use std::{collections::HashMap, fmt::Display, i64};

use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use cxx_qt_lib_shoop::{
    qjsvalue::qvariant_qjsvalue_convert_js_objects,
    qmetatype_helpers::*,
    qvariant_helpers::{
        qvariant_to_qlist_u8, qvariant_to_qstringlist, qvariant_to_qvariantlist,
        qvariant_to_qvariantmap, qvariant_type_id, qvariant_type_name, qvariantmap_to_qvariant,
    },
};
use omnilua::{self, FromLua};

pub type LuaMultiValue = omnilua::Variadic<omnilua::Value>;

pub trait IntoLuaExtended {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value>;
}

pub trait IntoLuaMultiExtended {
    fn into_lua_multi(self, lua: &omnilua::Lua) -> omnilua::Result<LuaMultiValue>;
}

impl<T: omnilua::IntoLuaMulti> IntoLuaMultiExtended for T {
    fn into_lua_multi(self, lua: &omnilua::Lua) -> omnilua::Result<LuaMultiValue> {
        Ok(T::into_lua_multi(self, lua)?.into())
    }
}

pub trait FromLuaMultiExtended: Sized {
    fn from_lua_multi(value: LuaMultiValue, lua: &omnilua::Lua) -> omnilua::Result<Self>;
}

impl<T: omnilua::FromLuaMulti> FromLuaMultiExtended for T {
    fn from_lua_multi(value: LuaMultiValue, lua: &omnilua::Lua) -> omnilua::Result<Self> {
        T::from_lua_multi(value.into_vec(), lua)
    }
}

pub trait FromLuaExtended: Sized {
    fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self>;
}

macro_rules! specific_builtin_impl {
    ($T:ty) => {
        impl FromLuaExtended for $T {
            fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self> {
                <$T as omnilua::FromLua>::from_lua(value, lua)
            }
        }

        impl IntoLuaExtended for $T {
            fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
                <$T as omnilua::IntoLua>::into_lua(self, lua)
            }
        }
    };
}

specific_builtin_impl!(i32);
specific_builtin_impl!(i64);
specific_builtin_impl!(String);
specific_builtin_impl!(f64);
specific_builtin_impl!(bool);

impl FromLuaExtended for u8 {
    fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self> {
        let value = <i64 as omnilua::FromLua>::from_lua(value, lua)?;
        u8::try_from(value).map_err(|_| conversion_error("Lua integer is outside u8 range"))
    }
}

impl IntoLuaExtended for u8 {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        IntoLuaExtended::into_lua(i64::from(self), lua)
    }
}

impl FromLuaExtended for f32 {
    fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self> {
        Ok(<f64 as omnilua::FromLua>::from_lua(value, lua)? as f32)
    }
}

impl IntoLuaExtended for f32 {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        IntoLuaExtended::into_lua(f64::from(self), lua)
    }
}

impl FromLuaExtended for QString {
    fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self> {
        <String as omnilua::FromLua>::from_lua(value, lua).map(QString::from)
    }
}

impl IntoLuaExtended for QString {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        self.to_string().into_lua(lua)
    }
}

impl FromLuaExtended for QVariant {
    fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self> {
        if matches!(value, omnilua::Value::Table(_)) {
            let map = QMap::<QMapPair_QString_QVariant>::from_lua(value, lua)?;
            return qvariantmap_to_qvariant(&map).map_err(|error| {
                conversion_error(format!("failed to convert QVariantMap: {error}"))
            });
        }

        match value {
            omnilua::Value::Nil => Ok(QVariant::default()),
            omnilua::Value::Boolean(value) => Ok(QVariant::from(&value)),
            omnilua::Value::Integer(value) => Ok(QVariant::from(&value)),
            omnilua::Value::Number(value) => Ok(QVariant::from(&value)),
            omnilua::Value::String(value) => Ok(QVariant::from(&QString::from(value.to_str()?))),
            other => Err(conversion_error(format!(
                "unsupported {} to QVariant conversion",
                value_type_name(&other)
            ))),
        }
    }
}

impl IntoLuaExtended for QVariant {
    fn into_lua(mut self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        if self.is_null() {
            return Ok(omnilua::Value::Nil);
        }

        let type_id = qvariant_type_id(&self)
            .map_err(|_| conversion_error("failed to get QVariant type ID"))?;

        macro_rules! convert {
            ($T:ty) => {
                IntoLuaExtended::into_lua(
                    self.value::<$T>().ok_or_else(|| {
                        conversion_error(format!(
                            "failed to get {} QVariant value",
                            qvariant_type_name(&self).unwrap_or("unknown")
                        ))
                    })?,
                    lua,
                )
            };
        }

        match type_id {
            value if value == qmetatype_id_int() => convert!(i64),
            value
                if value == qmetatype_id_int64()
                    || qvariant_type_name(&self).unwrap_or("unknown") == "qlonglong" =>
            {
                convert!(i64)
            }
            value if value == qmetatype_id_uint() => convert!(i64),
            value
                if value == qmetatype_id_uchar()
                    || qvariant_type_name(&self).unwrap_or("unknown") == "uchar" =>
            {
                convert!(u8)
            }
            value if value == qmetatype_id_uint64() => convert!(i64),
            value if value == qmetatype_id_bool() => convert!(bool),
            value if value == qmetatype_id_float() => convert!(f32),
            value if value == qmetatype_id_double() => convert!(f64),
            value if value == qmetatype_id_qstring() => IntoLuaExtended::into_lua(
                self.value::<QString>()
                    .ok_or_else(|| conversion_error("QVariant value is not a QString"))?
                    .to_string(),
                lua,
            ),
            value if value == qmetatype_id_qvariantmap() => qvariant_to_qvariantmap(&self)
                .map_err(|error| {
                    conversion_error(format!("failed to extract QVariantMap: {error}"))
                })?
                .into_lua(lua),
            value if value == qmetatype_id_qvariantlist() => qvariant_to_qvariantlist(&self)
                .map_err(|error| {
                    conversion_error(format!("failed to extract QVariantList: {error}"))
                })?
                .into_lua(lua),
            value if value == qmetatype_id_qstringlist() => qvariant_to_qstringlist(&self)
                .map_err(|error| {
                    conversion_error(format!("failed to extract QStringList: {error}"))
                })?
                .into_lua(lua),
            value if value == qmetatype_id_qlist_u8() => qvariant_to_qlist_u8(&self)
                .map_err(|error| conversion_error(format!("failed to extract QList<u8>: {error}")))?
                .into_lua(lua),
            value if value == qmetatype_id_qjsvalue() => {
                let pin_self = std::pin::Pin::new(&mut self);
                let converted =
                    qvariant_qjsvalue_convert_js_objects(pin_self).map_err(|error| {
                        conversion_error(format!(
                            "failed to convert JS objects in QVariant: {error}"
                        ))
                    })?;
                if !converted {
                    return Err(conversion_error(
                        "failed to convert QJSValue: post-conversion value is still JS",
                    ));
                }
                self.into_lua(lua)
            }
            _ => Err(conversion_error("unsupported QVariant to Lua conversion")),
        }
    }
}

impl FromLuaExtended for QList<QVariant> {
    fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self> {
        if !matches!(value, omnilua::Value::Table(_)) {
            return Err(conversion_error("value is not a table for QList<QVariant>"));
        }
        let mut hashmap: HashMap<i64, omnilua::Value> = HashMap::from_lua(value, lua)?;
        let mut result = QList::default();
        if hashmap.is_empty() {
            return Ok(result);
        }
        let min_value = *hashmap.keys().min().expect("non-empty map has a minimum");
        for index in min_value..min_value + hashmap.len() as i64 {
            let value = hashmap.remove(&index).ok_or_else(|| {
                conversion_error(format!(
                    "missing list index {index} in table starting at {min_value}"
                ))
            })?;
            result.append(QVariant::from_lua(value, lua)?);
        }
        Ok(result)
    }
}

impl IntoLuaExtended for QList<QVariant> {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        sequence_value(
            lua,
            self.iter().map(|value| {
                IntoLuaExtended::into_lua(value.clone(), lua).unwrap_or(omnilua::Value::Nil)
            }),
        )
    }
}

impl IntoLuaExtended for QList<u8> {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        sequence_value(
            lua,
            self.iter()
                .map(|value| IntoLuaExtended::into_lua(*value, lua).unwrap_or(omnilua::Value::Nil)),
        )
    }
}

impl IntoLuaExtended for QList<QString> {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        sequence_value(
            lua,
            self.iter().map(|value| {
                IntoLuaExtended::into_lua(value.clone(), lua).unwrap_or(omnilua::Value::Nil)
            }),
        )
    }
}

impl FromLuaExtended for QMap<QMapPair_QString_QVariant> {
    fn from_lua(value: omnilua::Value, lua: &omnilua::Lua) -> omnilua::Result<Self> {
        let omnilua::Value::Table(table) = value else {
            return Err(conversion_error(
                "value is not a table for QMap<QString, QVariant>",
            ));
        };
        let mut result = QMap::default();
        for (key, value) in table.raw_pairs()? {
            let key = QString::from(<String as omnilua::FromLua>::from_lua(key, lua)?);
            result.insert(key, QVariant::from_lua(value, lua)?);
        }
        Ok(result)
    }
}

impl IntoLuaExtended for QMap<QMapPair_QString_QVariant> {
    fn into_lua(self, lua: &omnilua::Lua) -> omnilua::Result<omnilua::Value> {
        let table = lua.create_table()?;
        for (key, value) in self.iter() {
            let key = IntoLuaExtended::into_lua(key.clone(), lua)?;
            let value = IntoLuaExtended::into_lua(value.clone(), lua)?;
            table.set(key, value)?;
        }
        Ok(omnilua::Value::Table(table))
    }
}

fn sequence_value(
    lua: &omnilua::Lua,
    values: impl IntoIterator<Item = omnilua::Value>,
) -> omnilua::Result<omnilua::Value> {
    let table = lua.create_table()?;
    for (index, value) in values.into_iter().enumerate() {
        table.set(index + 1, value)?;
    }
    Ok(omnilua::Value::Table(table))
}

fn conversion_error(message: impl Display) -> omnilua::Error {
    omnilua::LuaError::runtime(format_args!("{message}")).into()
}

fn value_type_name(value: &omnilua::Value) -> &'static str {
    match value {
        omnilua::Value::Nil => "nil",
        omnilua::Value::Boolean(_) => "boolean",
        omnilua::Value::Integer(_) | omnilua::Value::Number(_) => "number",
        omnilua::Value::String(_) => "string",
        omnilua::Value::Table(_) => "table",
        omnilua::Value::Function(_) => "function",
        omnilua::Value::UserData(_) => "userdata",
        omnilua::Value::LightUserData(_) => "light userdata",
        omnilua::Value::Thread(_) => "thread",
    }
}
