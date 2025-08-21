use anyhow;
use backend_bindings::LoopMode;
use cxx_qt_lib::{QList, QMap, QMapPair_QString_QVariant, QString, QVariant};
use cxx_qt_lib_shoop::qvariant_helpers::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use crate::references_qobject::ReferencesQObject;

type QVariantMap = QMap<QMapPair_QString_QVariant>;
type QVariantList = QList<QVariant>;

pub struct LoopReference<T>
where
    T: ReferencesQObject,
{
    pub obj: T,
}

impl<T: ReferencesQObject> Hash for LoopReference<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.obj.as_qobject_ref().hash(state);
    }
}

impl<T: ReferencesQObject> PartialEq for LoopReference<T> {
    fn eq(&self, other: &Self) -> bool {
        self.obj.as_qobject_ref() == other.obj.as_qobject_ref()
    }
}

impl<T: ReferencesQObject> Eq for LoopReference<T> {}

impl<T: ReferencesQObject> Clone for LoopReference<T> {
    fn clone(&self) -> Self {
        Self {
            obj: self.obj.copy(),
        }
    }
}

impl<T: ReferencesQObject> Debug for LoopReference<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.obj.as_qobject_ref().fmt(f)
    }
}
pub struct CompositeLoopIterationEvents<T>
where
    T: ReferencesQObject,
{
    pub loops_start: HashMap<LoopReference<T>, Option<LoopMode>>,
    pub loops_end: HashSet<LoopReference<T>>,
    pub loops_ignored: HashSet<LoopReference<T>>,
}

impl<T: ReferencesQObject> Default for CompositeLoopIterationEvents<T> {
    fn default() -> Self {
        Self {
            loops_start: HashMap::default(),
            loops_end: HashSet::default(),
            loops_ignored: HashSet::default(),
        }
    }
}

impl<T: ReferencesQObject> Clone for CompositeLoopIterationEvents<T> {
    fn clone(&self) -> Self {
        Self {
            loops_start: self.loops_start.clone(),
            loops_end: self.loops_end.clone(),
            loops_ignored: self.loops_ignored.clone(),
        }
    }
}

impl<T: ReferencesQObject> PartialEq for CompositeLoopIterationEvents<T> {
    fn eq(&self, other: &Self) -> bool {
        self.loops_start == other.loops_start
            && self.loops_end == other.loops_end
            && self.loops_ignored == other.loops_ignored
    }
}

impl<T: ReferencesQObject> Debug for CompositeLoopIterationEvents<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeLoopIterationEvents")
            .field("loops_start", &self.loops_start)
            .field("loops_end", &self.loops_end)
            .field("loops_ignored", &self.loops_ignored)
            .finish();
        Ok(())
    }
}

pub type CompositeLoopIteration = i32;

pub struct CompositeLoopSchedule<T>
where
    T: ReferencesQObject,
{
    pub data: BTreeMap<CompositeLoopIteration, CompositeLoopIterationEvents<T>>,
}

impl<T: ReferencesQObject> Default for CompositeLoopSchedule<T> {
    fn default() -> Self {
        Self {
            data: BTreeMap::default(),
        }
    }
}

impl<T: ReferencesQObject> Clone for CompositeLoopSchedule<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
        }
    }
}

impl<T: ReferencesQObject> PartialEq for CompositeLoopSchedule<T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl<T: ReferencesQObject> Debug for CompositeLoopSchedule<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeLoopSchedule")
            .field("data", &self.data)
            .finish();
        Ok(())
    }
}

impl<T: ReferencesQObject> CompositeLoopIterationEvents<T> {
    pub fn to_qvariantmap(&self) -> QVariantMap {
        let mut loops_start: QVariantList = QList::default();
        for (loop_start, loop_mode) in self.loops_start.iter() {
            let mut entry: QVariantList = QList::default();
            entry.append(loop_start.obj.to_qvariant().unwrap_or(QVariant::default()));
            let mode: QVariant = match loop_mode {
                Some(mode) => {
                    let mode = mode.clone() as isize as i32;
                    QVariant::from(&mode)
                }
                None => QVariant::default(),
            };
            entry.append(mode);

            loops_start.append(qvariantlist_to_qvariant(&entry).unwrap());
        }
        let loops_start = qvariantlist_to_qvariant(&loops_start).unwrap();

        let mut loops_end: QVariantList = QList::default();
        let mut loops_ignored: QVariantList = QList::default();
        for loop_obj in self.loops_end.iter() {
            loops_end.append(loop_obj.obj.to_qvariant().unwrap_or(QVariant::default()));
        }
        for loop_obj in self.loops_ignored.iter() {
            loops_ignored.append(loop_obj.obj.to_qvariant().unwrap());
        }
        let loops_end = qvariantlist_to_qvariant(&loops_end).unwrap();
        let loops_ignored = qvariantlist_to_qvariant(&loops_ignored).unwrap();

        let mut map: QVariantMap = QMap::default();
        map.insert(QString::from("loops_start"), loops_start);
        map.insert(QString::from("loops_end"), loops_end);
        map.insert(QString::from("loops_ignored"), loops_ignored);

        map
    }

    pub fn to_qvariant(&self) -> QVariant {
        let map = self.to_qvariantmap();
        qvariantmap_to_qvariant(&map).unwrap()
    }

    pub fn from_qvariantmap(map: &QVariantMap) -> Result<Self, anyhow::Error> {
        let mut result = Self::default();

        match map.get(&QString::from("loops_start")) {
            Some(loops_start) => {
                let loops_start = qvariant_to_qvariantlist(&loops_start)?;
                for entry in loops_start.iter() {
                    let entry = qvariant_to_qvariantlist(&entry)?;
                    let loop_start = entry
                        .get(0)
                        .ok_or(anyhow::anyhow!("No loop start entry in schedule elem"))?;
                    let loop_mode = entry
                        .get(1)
                        .ok_or(anyhow::anyhow!("No loop mode entry in schedule elem"))?;
                    let loop_start = LoopReference::<T> {
                        obj: T::from_qvariant(&loop_start).unwrap(),
                    };
                    let loop_mode: Option<LoopMode> = match loop_mode.value::<i32>() {
                        Some(mode) => Some(LoopMode::try_from(mode)?),
                        None => None,
                    };
                    result.loops_start.insert(loop_start, loop_mode);
                }
            }
            None => {
                return Err(anyhow::anyhow!(
                    "CompositeLoopSchedule: missing loops_start"
                ))
            }
        }

        match map.get(&QString::from("loops_end")) {
            Some(loops_end) => {
                let loops_end = qvariant_to_qvariantlist(&loops_end)?;
                for entry in loops_end.iter() {
                    let object = LoopReference::<T> {
                        obj: T::from_qvariant(&entry).unwrap(),
                    };
                    result.loops_end.insert(object);
                }
            }
            None => return Err(anyhow::anyhow!("CompositeLoopSchedule: missing loops_end")),
        }

        match map.get(&QString::from("loops_ignored")) {
            Some(loops_ignored) => {
                let loops_ignored = qvariant_to_qvariantlist(&loops_ignored)?;
                for entry in loops_ignored.iter() {
                    let object = LoopReference::<T> {
                        obj: T::from_qvariant(&entry).unwrap(),
                    };
                    result.loops_ignored.insert(object);
                }
            }
            None => {
                return Err(anyhow::anyhow!(
                    "CompositeLoopSchedule: missing loops_ignored"
                ))
            }
        }

        Ok(result)
    }

    pub fn from_qvariant(qvariant: &QVariant) -> Result<Self, anyhow::Error> {
        let map = qvariant_to_qvariantmap(qvariant).map_err(|e| {
            anyhow::anyhow!("CompositeLoopSchedule: could not convert to QVariantMap: {e}")
        })?;
        Self::from_qvariantmap(&map)
    }
}

impl<T: ReferencesQObject> CompositeLoopSchedule<T> {
    pub fn to_qvariantmap(&self) -> QVariantMap {
        // TODO: There is no built in integer mapping type in cxx-qt. Therefore,
        // we stringify the integer keys.
        let mut map: QVariantMap = QMap::default();
        for (iteration, events) in &self.data {
            let events = events.to_qvariant();
            let key = QString::from(format!("{iteration}"));
            map.insert(key, events);
        }

        map
    }

    pub fn from_qvariantmap(map: &QVariantMap) -> Result<Self, anyhow::Error> {
        let mut result = CompositeLoopSchedule::default();
        for (key, value) in map.iter() {
            let iteration = key.to_string().parse::<CompositeLoopIteration>()?;
            let events = CompositeLoopIterationEvents::from_qvariant(&value)?;
            result.data.insert(iteration, events);
        }

        Ok(result)
    }
}
