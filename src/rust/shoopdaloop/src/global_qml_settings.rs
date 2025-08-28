use cxx_qt_lib::*;
use std::iter;
use std::path::PathBuf;

#[derive(Debug)]
pub struct GlobalQmlSettings {
    pub backend_type: backend_bindings::AudioDriverType,
    pub load_session_on_startup: Option<PathBuf>,
    pub test_grab_screens_dir: Option<PathBuf>,
    pub developer_mode: bool,
    pub quit_after: Option<i32>,
    pub monkey_tester: bool,
    pub qml_dir: String,
    pub lua_dir: String,
    pub resource_dir: String,
    pub schemas_dir: String,
    pub version_string: String,
}

impl GlobalQmlSettings {
    pub fn as_named_qvariants(self: &Self) -> impl Iterator<Item = (QString, QVariant)> {
        fn option_pathbuf_to_qvariant(p: Option<PathBuf>) -> QVariant {
            match p {
                Some(pathbuf) => QVariant::from(&QString::from(pathbuf.to_str().unwrap())),
                None => <QVariant as Default>::default(),
            }
        }

        fn option_i32_to_qvariant(i: Option<i32>) -> QVariant {
            match i {
                Some(i) => QVariant::from(&i),
                None => <QVariant as Default>::default(),
            }
        }

        iter::once((
            QString::from("backend_type"),
            QVariant::from(&(self.backend_type as i32)),
        ))
        .chain(iter::once((
            QString::from("load_session_on_startup"),
            option_pathbuf_to_qvariant(self.load_session_on_startup.clone()),
        )))
        .chain(iter::once((
            QString::from("test_grab_screens_dir"),
            option_pathbuf_to_qvariant(self.test_grab_screens_dir.clone()),
        )))
        .chain(iter::once((
            QString::from("developer_mode"),
            QVariant::from(&self.developer_mode),
        )))
        .chain(iter::once((
            QString::from("quit_after"),
            option_i32_to_qvariant(self.quit_after),
        )))
        .chain(iter::once((
            QString::from("monkey_tester"),
            QVariant::from(&self.monkey_tester),
        )))
        .chain(iter::once((
            QString::from("qml_dir"),
            QVariant::from(&QString::from(&self.qml_dir)),
        )))
        .chain(iter::once((
            QString::from("lua_dir"),
            QVariant::from(&QString::from(&self.lua_dir)),
        )))
        .chain(iter::once((
            QString::from("resource_dir"),
            QVariant::from(&QString::from(&self.resource_dir)),
        )))
        .chain(iter::once((
            QString::from("schemas_dir"),
            QVariant::from(&QString::from(&self.schemas_dir)),
        )))
    }

    pub fn as_qvariantmap(self: &Self) -> QMap<QMapPair_QString_QVariant> {
        let mut map = QMap::<QMapPair_QString_QVariant>::default();
        for (key, value) in self.as_named_qvariants() {
            map.insert(key, value);
        }
        map
    }
}
