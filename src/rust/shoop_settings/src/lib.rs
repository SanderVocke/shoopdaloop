use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const SETTINGS_FILENAME: &str = "settings.json";
pub const CARLA_HOSTING_MODE_KEY: &str = "carla_hosting_mode";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarlaHostingMode {
    #[default]
    InProcess,
    Subprocess,
}

impl CarlaHostingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InProcess => "in_process",
            Self::Subprocess => "subprocess",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UserSettings {
    pub carla_hosting_mode: CarlaHostingMode,
}

impl UserSettings {
    pub fn from_json(value: &Value) -> Result<Self> {
        let schema = value
            .get("schema")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("settings are missing a schema"))?;
        if schema != "settings.1" {
            return Err(anyhow!("unsupported settings schema {schema}"));
        }
        let configuration = value
            .get("configuration")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("settings are missing a configuration object"))?;
        let carla_hosting_mode = match configuration.get(CARLA_HOSTING_MODE_KEY) {
            None => CarlaHostingMode::InProcess,
            Some(value) => serde_json::from_value(value.clone())
                .context("invalid Carla hosting mode in settings")?,
        };
        Ok(Self { carla_hosting_mode })
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path)
            .with_context(|| format!("could not read settings from {}", path.display()))?;
        let value: Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("could not parse settings from {}", path.display()))?;
        Self::from_json(&value)
    }

    pub fn load_or_default(path: &Path) -> (Self, Option<anyhow::Error>) {
        match Self::load(path) {
            Ok(settings) => (settings, None),
            Err(error) => (Self::default(), Some(error)),
        }
    }
}

pub fn default_settings_path() -> Result<PathBuf> {
    let project = directories::ProjectDirs::from("com", "ShoopDaLoop", "ShoopDaLoop")
        .ok_or_else(|| anyhow!("could not determine project directories"))?;
    Ok(project.config_dir().join(SETTINGS_FILENAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn existing_settings(extra: &str) -> Value {
        serde_json::from_str(&format!(
            r#"{{
                "schema":"settings.1",
                "configuration":{{
                    "midi_settings":{{"schema":"midi_settings.1"}},
                    "script_settings":{{"schema":"script_settings.1"}}
                    {extra}
                }}
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn old_settings_default_to_in_process_without_losing_compatibility() {
        let settings = UserSettings::from_json(&existing_settings("")).unwrap();
        assert_eq!(settings.carla_hosting_mode, CarlaHostingMode::InProcess);
    }

    #[test]
    fn subprocess_setting_round_trips_as_typed_value() {
        let settings =
            UserSettings::from_json(&existing_settings(r#", "carla_hosting_mode":"subprocess""#))
                .unwrap();
        assert_eq!(settings.carla_hosting_mode, CarlaHostingMode::Subprocess);
        assert_eq!(settings.carla_hosting_mode.as_str(), "subprocess");
    }

    #[test]
    fn missing_file_defaults_and_malformed_file_is_observable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILENAME);
        assert_eq!(UserSettings::load(&path).unwrap(), UserSettings::default());
        std::fs::write(&path, b"not json").unwrap();
        let (settings, error) = UserSettings::load_or_default(&path);
        assert_eq!(settings, UserSettings::default());
        assert!(error.is_some());
    }

    #[test]
    fn unknown_schema_and_mode_are_rejected() {
        let mut value = existing_settings("");
        value["schema"] = Value::String("settings.2".to_owned());
        assert!(UserSettings::from_json(&value).is_err());
        assert!(
            UserSettings::from_json(&existing_settings(r#", "carla_hosting_mode":"remote""#))
                .is_err()
        );
    }
}
