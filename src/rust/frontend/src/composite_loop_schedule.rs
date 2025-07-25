use cxx_qt_lib_shoop::{qobject::QObject, qvariant_qobject::{qobject_ptr_to_qvariant, qvariant_to_qobject_ptr}, qvariant_qvariantlist::{qvariant_as_qvariantlist, qvariantlist_as_qvariant}, qvariant_qvariantmap::qvariantmap_as_qvariant};
use cxx_qt_lib::{QMap, QList, QVariant, QMapPair_QString_QVariant, QString};
use std::collections::{HashMap, HashSet};
use shoop_py_backend::shoop_loop::LoopMode;
use anyhow;

type QVariantMap = QMap<QMapPair_QString_QVariant>;
type QVariantList = QList<QVariant>;

#[derive(Default)]
pub struct CompositeLoopIterationEvents {
    pub loops_start   : HashMap<*mut QObject, LoopMode>,
    pub loops_end     : HashSet<*mut QObject>,
    pub loops_ignored : HashSet<*mut QObject>,
}

pub type CompositeLoopIteration = i32;

pub struct CompositeLoopSchedule {
    pub data : HashMap<CompositeLoopIteration, CompositeLoopIterationEvents>
}

impl CompositeLoopIterationEvents {
    pub fn to_qvariantmap(&self) -> QVariantMap {
        let mut loops_start : QVariantList = QList::default();
        for (loop_start, loop_mode) in self.loops_start.iter() {
            let mut entry : QVariantList = QList::default();
            entry.append(qobject_ptr_to_qvariant(*loop_start));
            let mode : i32 = loop_mode.clone() as isize as i32;
            entry.append(QVariant::from(&mode));

            loops_start.append(qvariantlist_as_qvariant(&entry).unwrap());
        }
        let loops_start = qvariantlist_as_qvariant(&loops_start).unwrap();

        let mut loops_end : QVariantList = QList::default();
        let mut loops_ignored : QVariantList = QList::default();
        for loop_obj in self.loops_end.iter() {
            loops_end.append(qobject_ptr_to_qvariant(*loop_obj));
        }
        for loop_obj in self.loops_ignored.iter() {
            loops_ignored.append(qobject_ptr_to_qvariant(*loop_obj));
        }
        let loops_end = qvariantlist_as_qvariant(&loops_end).unwrap();
        let loops_ignored = qvariantlist_as_qvariant(&loops_ignored).unwrap();

        let mut map : QVariantMap = QMap::default();
        map.insert(QString::from("loops_start"), loops_start);
        map.insert(QString::from("loops_end"), loops_end);
        map.insert(QString::from("loops_ignored"), loops_ignored);

        map
    }

    pub fn from_qvariantmap(map : &QVariantMap) -> Result<Self, anyhow::Error> {
        let mut result = Self::default();
        match map.get(&QString::from("loops_start")) {
            Some(loops_start) => {
                let loops_start = qvariant_as_qvariantlist(&loops_start)?;
                for entry in loops_start.iter() {
                    let entry = qvariant_as_qvariantlist(&entry)?;
                    let loop_start = entry.get(0).ok_or(anyhow::anyhow!("No loop start entry in schedule elem"))?;
                    let loop_mode = entry.get(1).ok_or(anyhow::anyhow!("No loop mode entry in schedule elem"))?;
                    let loop_start = qvariant_to_qobject_ptr(loop_start).ok_or(anyhow::anyhow!("Loop start entry is not an object ptr"))?;
                    let loop_mode = loop_mode.value::<i32>().ok_or(anyhow::anyhow!("Loop mode is not an integer"))?;
                    todo!();
                }
            },
            None => return Err(anyhow::anyhow!("CompositeLoopSchedule: missing loops_start")),
        }

        Ok(result)
    }
}

impl CompositeLoopSchedule {
    pub fn to_qvariantmap(&self) -> QVariantMap {
        // TODO: There is no built in integer mapping type in cxx-qt. Therefore,
        // we stringify the integer keys.
        let mut map : QVariantMap = QMap::default();
        for (iteration, events) in &self.data {
            let events_map : QVariantMap = events.to_qvariantmap();
            let events = qvariantmap_as_qvariant(&events_map).unwrap();
            let key = QString::from(format!("{iteration}"));
            map.insert(key, events);
        }

        map
    }
}