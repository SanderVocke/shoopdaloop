use anyhow;
use backend_bindings::MidiEvent;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use cxx_qt_lib_shoop::qvariant_helpers::{
    qlist_i32_to_qvariant, qvariant_to_qlist_i32, qvariant_to_qvariantmap, qvariantmap_to_qvariant,
};

type QMap_QString_QVariant = QMap<QMapPair_QString_QVariant>;

pub trait MidiEventToQVariant {
    fn to_qvariantmap(&self) -> QMap_QString_QVariant;
    fn to_qvariant(&self) -> QVariant;
    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
    fn from_qvariantmap(qvariantmap: &QMap_QString_QVariant) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

impl MidiEventToQVariant for MidiEvent {
    fn to_qvariantmap(&self) -> QMap_QString_QVariant {
        let mut result = QMap::default();
        let mut data: QList<i32> = QList::default();
        result.insert(QString::from("time"), QVariant::from(&self.time));
        self.data.iter().for_each(|v| data.append(*v as i32));
        result.insert(QString::from("data"), qlist_i32_to_qvariant(&data).unwrap());
        result
    }

    fn to_qvariant(&self) -> QVariant {
        qvariantmap_to_qvariant(&self.to_qvariantmap()).unwrap()
    }

    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> {
        Self::from_qvariantmap(&qvariant_to_qvariantmap(qvariant).unwrap())
    }

    fn from_qvariantmap(qvariantmap: &QMap_QString_QVariant) -> Result<Self, anyhow::Error> {
        let data_variant: QVariant = qvariantmap
            .get(&QString::from("data"))
            .ok_or(anyhow::anyhow!("no data key in map"))?;
        let time: QVariant = qvariantmap
            .get(&QString::from("time"))
            .ok_or(anyhow::anyhow!("no time key in map"))?;
        let data: QList<i32> = qvariant_to_qlist_i32(&data_variant)?;
        let data: Vec<u8> = data.iter().map(|v| *v as u8).collect();
        let time: i32 = time
            .value::<i32>()
            .ok_or(anyhow::anyhow!("invalid time value"))?;
        Ok(Self { data, time })
    }
}
