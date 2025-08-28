use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qjsonobject::QJsonObject;

pub use crate::cxx_qt_shoop::qobj_settings_io_bridge::ffi::SettingsIO;
use crate::cxx_qt_shoop::qobj_settings_io_bridge::ffi::*;
use std::{path::{Path, PathBuf}, pin::Pin};
use common::logging::macros::*;
shoop_log_unit!("Frontend.SettingsIO");

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_type_settingsio(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl SettingsIO {
    fn settings_dir(&self) -> PathBuf {
        PathBuf::from(directories::ProjectDirs::from("com", "ShoopDaLoop", "ShoopDaLoop").unwrap().config_dir())
    }

    pub fn save_settings(
        self: &SettingsIO,
        settings: QMap_QString_QVariant,
        override_filename: QVariant,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let json = QJsonObject::from_variant_map(&settings)
                .map_err(|e| anyhow::anyhow!("Failed to convert: {e}"))?
                .to_json()
                .map_err(|e| anyhow::anyhow!("Failed to convert: {e}"))?;
            let filename = match override_filename.is_null() {
                true => String::from("settings.json"),
                false => override_filename.value::<QString>().unwrap().to_string()
            };
            let path = self.settings_dir().join(filename);
            std::fs::write(path, json)?;
            Ok(())
        }() {
            error!("Could not write settings file: {e}");
        }
    }

    pub fn load_settings(self: &SettingsIO, override_filename: QVariant) -> QMap_QString_QVariant {
        todo!();
    }
}
