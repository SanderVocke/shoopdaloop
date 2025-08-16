use anyhow::anyhow;
use backend_bindings::{PortDataType, PortDirection};
use core::fmt::Debug;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString, QVariant};
use cxx_qt_lib_shoop::qvariant_helpers::{qvariant_to_qvariantmap, qvariantmap_to_qvariant};
use std::fmt;

pub struct ExternalPortDescriptor {
    pub name: String,
    pub direction: PortDirection,
    pub data_type: PortDataType,
}

impl Clone for ExternalPortDescriptor {
    fn clone(&self) -> Self {
        ExternalPortDescriptor {
            name: self.name.clone(),
            direction: self.direction,
            data_type: self.data_type,
        }
    }
}

impl Default for ExternalPortDescriptor {
    fn default() -> Self {
        Self {
            name: String::from(""),
            direction: PortDirection::Any,
            data_type: PortDataType::Any,
        }
    }
}

impl ExternalPortDescriptor {
    pub fn from_qvariantmap(
        map: &QMap<QMapPair_QString_QVariant>,
    ) -> Result<ExternalPortDescriptor, anyhow::Error> {
        let mut rval = ExternalPortDescriptor::default();

        if map.len() != 3 {
            return Err(anyhow!(
                "QVariantMap is not an ExternalPortDescriptor: not 3 keys"
            ));
        }

        let (name_key, direction_key, data_type_key) = (
            QString::from("name"),
            QString::from("direction"),
            QString::from("data_type"),
        );
        let name_var: QVariant = map
            .get(&name_key)
            .ok_or(anyhow!("Not an ExternalPortDescriptor: name missing"))?;
        let direction_var: QVariant = map
            .get(&direction_key)
            .ok_or(anyhow!("Not an ExternalPortDescriptor: direction missing"))?;
        let data_type_var: QVariant = map
            .get(&data_type_key)
            .ok_or(anyhow!("Not an ExternalPortDescriptor: data_type missing"))?;

        let name_qstr = name_var
            .value::<QString>()
            .ok_or(anyhow!("Not an ExternalPortDescriptor: name not a QString"))?;
        let direction = direction_var.value::<i32>().ok_or(anyhow!(
            "Not an ExternalPortDescriptor: direction not an int"
        ))?;
        let data_type = data_type_var.value::<i32>().ok_or(anyhow!(
            "Not an ExternalPortDescriptor: direction not an int"
        ))?;

        rval.name = name_qstr.to_string();
        rval.direction = PortDirection::try_from(direction)
            .map_err(|_| anyhow!("Not an ExternalPortDescriptor: direction out of range"))?;
        rval.data_type = PortDataType::try_from(data_type)
            .map_err(|_| anyhow!("Not an ExternalPortDescriptor: data_type out of range"))?;

        Ok(rval)
    }

    pub fn to_qvariantmap(self: &Self) -> QMap<QMapPair_QString_QVariant> {
        let mut rval = QMap::<QMapPair_QString_QVariant>::default();
        let (name_key, direction_key, data_type_key) = (
            QString::from("name"),
            QString::from("direction"),
            QString::from("data_type"),
        );
        let (name, direction, data_type) = (
            QString::from(self.name.as_str()),
            self.direction as i32,
            self.data_type as i32,
        );
        let (name_variant, direction_variant, data_type_variant) = (
            QVariant::from(&name),
            QVariant::from(&direction),
            QVariant::from(&data_type),
        );
        rval.insert(name_key, name_variant);
        rval.insert(direction_key, direction_variant);
        rval.insert(data_type_key, data_type_variant);

        rval
    }
}

impl TryInto<QVariant> for ExternalPortDescriptor {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<QVariant, Self::Error> {
        let map = self.to_qvariantmap();
        let variant = qvariantmap_to_qvariant(&map)?;
        Ok(variant)
    }
}

impl TryFrom<QVariant> for ExternalPortDescriptor {
    type Error = anyhow::Error;

    fn try_from(v: QVariant) -> Result<ExternalPortDescriptor, Self::Error> {
        let qvariantmap = qvariant_to_qvariantmap(&v)?;
        let desc = ExternalPortDescriptor::from_qvariantmap(&qvariantmap)?;
        Ok(desc)
    }
}

impl Debug for ExternalPortDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ExternalPortDescriptor {{ name: {}, direction: {:?}, data_type: {:?} }}",
            self.name, self.direction, self.data_type
        )
    }
}
