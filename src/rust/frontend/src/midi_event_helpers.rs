use anyhow;
use backend_bindings::MidiEvent;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use cxx_qt_lib_shoop::qvariant_helpers::{
    qlist_i32_to_qvariant, qvariant_to_qlist_i32, qvariant_to_qvariantlist, qvariant_to_qvariantmap, qvariant_type_name, qvariantmap_to_qvariant
};

pub trait MidiEventToQVariant {
    fn to_qvariantmap(&self) -> QMap<QMapPair_QString_QVariant>;
    fn to_qvariant(&self) -> QVariant;
    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
    fn from_qvariantmap(qvariantmap: &QMap<QMapPair_QString_QVariant>) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

impl MidiEventToQVariant for MidiEvent {
    fn to_qvariantmap(&self) -> QMap<QMapPair_QString_QVariant> {
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

    fn from_qvariantmap(qvariantmap: &QMap<QMapPair_QString_QVariant>) -> Result<Self, anyhow::Error> {
        let data_variant: QVariant = qvariantmap
            .get(&QString::from("data"))
            .ok_or(anyhow::anyhow!("no data key in map"))?;
        let data: QList<QVariant> = qvariant_to_qvariantlist(&data_variant)?;
        if data.len() <= 0 {
            return Err(anyhow::anyhow!("Empty MIDI msg"));
        }
        let data: Vec<u8> = data.iter().map(|v| -> Result<u8, anyhow::Error> {
            let integer = v.value::<i32>().ok_or(anyhow::anyhow!("Unable to convert MIDI data element to integer"))?;
            Ok(integer as u8)
        }).collect::<Result<Vec<u8>, anyhow::Error>>()?;

        let time: QVariant = qvariantmap
            .get(&QString::from("time"))
            .ok_or(anyhow::anyhow!("no time key in map"))?;
        let time: i32 = time
            .value::<i32>()
            .ok_or(anyhow::anyhow!("invalid time value"))?;
        Ok(Self { data, time })
    }
}
