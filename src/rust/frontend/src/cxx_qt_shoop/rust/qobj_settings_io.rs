use cxx_qt_lib_shoop::qjsonobject::QJsonObject;

pub use crate::cxx_qt_shoop::qobj_settings_io_bridge::ffi::SettingsIO;
use crate::cxx_qt_shoop::qobj_settings_io_bridge::ffi::*;
use common::logging::macros::*;
use std::path::PathBuf;
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
        PathBuf::from(
            directories::ProjectDirs::from("com", "ShoopDaLoop", "ShoopDaLoop")
                .expect("Could not determine project directories")
                .config_dir(),
        )
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
            debug!("Settings to save: {json}");
            let filename = match override_filename.is_null() {
                true => "settings.json".to_string(),
                false => override_filename
                    .value::<QString>()
                    .ok_or(anyhow::anyhow!("override_filename is not a string"))?
                    .to_string(),
            };
            let settings_dir = self.settings_dir();
            if !settings_dir.exists() {
                std::fs::create_dir_all(settings_dir)?;
            }
            let path = self.settings_dir().join(filename);
            std::fs::write(&path, json)?;
            info!("Saved settings to {path:?}");
            Ok(())
        }() {
            error!("Could not write settings file: {e}");
        }
    }

    pub fn load_settings(self: &SettingsIO, override_filename: QVariant) -> QVariant {
        match || -> Result<QVariant, anyhow::Error> {
            let filename = match override_filename.is_null() {
                true => "settings.json".to_string(),
                false => override_filename
                    .value::<QString>()
                    .ok_or(anyhow::anyhow!("override_filename is not a string"))?
                    .to_string(),
            };
            let path = self.settings_dir().join(filename);
            if !path.exists() {
                info!(
                    "No settings file found at {}, using default values",
                    path.display()
                );
                return Ok(QVariant::default());
            }
            let json = std::fs::read_to_string(&path)?;
            let json = QJsonObject::from_json(&json)
                .map_err(|e| anyhow::anyhow!("Failed to convert: {e}"))?;
            let json = json.as_ref().ok_or(anyhow::anyhow!("Failed to convert"))?;
            let jsonstr = json
                .to_json()
                .map_err(|e| anyhow::anyhow!("Could not stringify json: {e}"))?;
            let json = json
                .to_variant()
                .map_err(|e| anyhow::anyhow!("Failed to convert: {e}"))?;
            debug!("Loaded settings: {jsonstr}");
            info!("Loaded settings from {}", path.display());
            Ok(json)
        }() {
            Ok(result) => result,
            Err(e) => {
                error!("Could not read settings file: {e}");
                QVariant::default()
            }
        }
    }
}
