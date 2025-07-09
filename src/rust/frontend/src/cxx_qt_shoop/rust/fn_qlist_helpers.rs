use cxx::ExternType;
use cxx_qt_lib::{QList, QListElement, QVariant};
use std::any::{type_name, Any};

pub fn try_as_list_into<T>(list: &QList<QVariant>) -> Result<Vec<T>, anyhow::Error>
where
    T: TryFrom<QVariant> + std::any::Any,
{
    list.iter()
        .map(|v: &QVariant| -> Result<T, anyhow::Error> {
            let converted: T = v.clone().try_into().map_err(|_| {
                anyhow::anyhow!(
                    "Error converting from QVariant to {:?}, metatype {:?}",
                    type_name::<T>(),
                    crate::cxx_qt_lib_shoop::qvariant_helpers::qvariant_type_name(v)
                        .unwrap_or("Unknown")
                )
            })?;
            Ok(converted)
        })
        .collect()
}

pub fn as_qlist<RustT, QtT>(vec: &Vec<RustT>) -> Result<QList<QtT>, anyhow::Error>
where
    RustT: TryInto<QtT> + Clone + std::any::Any,
    QtT: QListElement + ExternType<Kind = cxx::kind::Trivial>,
{
    let mut list = QList::<QtT>::default();
    for elem in vec {
        let elem: QtT = elem
            .clone()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Conversion error"))?;
        QList::<QtT>::append(&mut list, elem);
    }
    Ok(list)
}

pub fn as_qlist_qvariant<T>(vec: &Vec<T>) -> Result<QList<QVariant>, anyhow::Error>
where
    T: TryInto<QVariant> + Clone + Any,
{
    as_qlist::<T, QVariant>(vec)
}
