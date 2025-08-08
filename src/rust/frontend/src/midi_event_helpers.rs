use backend_bindings::MidiEvent;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QVariant, QString, QList};
use anyhow;

type QMap_QString_QVariant = QMap<QMapPair_QString_QVariant>;

pub trait MidiEventToQVariant {
    fn to_qvariantmap(&self) -> QMap_QString_QVariant;
    fn to_qvariant(&self) -> QVariant;
    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> where Self: Sized;
    fn from_qvariantmap(qvariantmap: &QMap_QString_QVariant) -> Result<Self, anyhow::Error> where Self: Sized;
}

impl MidiEventToQVariant for MidiEvent {
    fn to_qvariantmap(&self) -> QMap_QString_QVariant {
        let mut result = QMap::default();
        let mut data : QList<i32> = QList::default();
        result.insert(QString::from("time"), QVariant::from(&self.time));
        self.data.iter().for_each(|v| data.append(*v as i32));
        result.insert(QString::from("data"), QVariant::from(&data));
        result
    }
    
    fn to_qvariant(&self) -> QVariant {
        todo!()
    }
    
    fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> {
        todo!()
    }
    
    fn from_qvariantmap(qvariantmap: &QMap_QString_QVariant) -> Result<Self, anyhow::Error> {
        todo!()
    }
}
