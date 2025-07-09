use anyhow::anyhow;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QVariant, QVariantValue};
use std::any::{type_name, Any};
use std::collections::HashMap;

pub fn try_as_hashmap_into<T>(
    list: &QMap<QMapPair_QString_QVariant>,
) -> Result<HashMap<String, T>, anyhow::Error>
where
    T: TryFrom<QVariant> + Any,
{
    list.iter()
        .map(
            |p: (&QString, &QVariant)| -> Result<(String, T), anyhow::Error> {
                let key = p.0.to_string();
                let converted_val: T = p.1.clone().try_into().map_err(|_| {
                    anyhow::anyhow!(
                        "Error converting from QVariant to {:?}, variant metatype is {:?}",
                        type_name::<T>(),
                        crate::cxx_qt_lib_shoop::qvariant_helpers::qvariant_type_name(p.1)
                            .unwrap_or("Unknown")
                    )
                })?;
                Ok((key, converted_val))
            },
        )
        .collect()
}

pub fn try_as_hashmap_convertto<T>(
    list: &QMap<QMapPair_QString_QVariant>,
) -> Result<HashMap<String, T>, anyhow::Error>
where
    T: QVariantValue + Any,
{
    list.iter()
        .map(
            |p: (&QString, &QVariant)| -> Result<(String, T), anyhow::Error> {
                let key = p.0.to_string();
                if !T::can_convert(p.1) {
                    return Err(anyhow!(
                        "Cannot convert QVariant to {:?}, metatype is {:?}",
                        type_name::<T>(),
                        crate::cxx_qt_lib_shoop::qvariant_helpers::qvariant_type_name(p.1)
                            .unwrap_or("Unknown")
                    ));
                }
                let converted_val = T::value_or_default(p.1);
                Ok((key, converted_val))
            },
        )
        .collect()
}
