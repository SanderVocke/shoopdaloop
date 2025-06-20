use anyhow::{self, Context};
use toml;
use serde;
use std::env;
use std::path::{Path, PathBuf};

use common::logging::macros::*;
shoop_log_unit!("Main");

#[derive(Clone)]
pub struct ShoopConfig {
    pub version: String,
    pub description: String,
    pub install_info: String,
    pub qml_dir: String,
    pub lua_dir: String,
    pub resource_dir: String,
    pub schemas_dir: String,
    pub qt_plugins_dir: String,
    pub pythonhome: String,
    pub pythonpaths: Vec<String>,
    pub dynlibpaths: Vec<String>,
    pub additional_qml_dirs: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ShoopTomlConfig {
    pub qml_dir : Option<String>,
    pub lua_dir : Option<String>,
    pub resource_dir : Option<String>,
    pub schemas_dir : Option<String>,
    pub qt_plugins_dir : Option<String>,
    pub pythonhome : Option<String>,
    pub pythonpaths : Option<Vec<String>>,
    pub dynlibpaths : Option<Vec<String>>,
    pub additional_qml_dirs: Option<Vec<String>>,
}

impl ShoopTomlConfig {
    pub fn apply_overrides(self: Self, config : &mut ShoopConfig) {
        if self.qml_dir.is_some() { config.qml_dir = self.qml_dir.unwrap() }
        if self.lua_dir.is_some() { config.lua_dir = self.lua_dir.unwrap() }
        if self.resource_dir.is_some() { config.resource_dir = self.resource_dir.unwrap() }
        if self.schemas_dir.is_some() { config.schemas_dir = self.schemas_dir.unwrap() }
        if self.qt_plugins_dir.is_some() { config.qt_plugins_dir = self.qt_plugins_dir.unwrap() }
        if self.pythonhome.is_some() { config.pythonhome = self.pythonhome.unwrap() }
        if self.pythonpaths.is_some() { config.pythonpaths = self.pythonpaths.unwrap() }
        if self.dynlibpaths.is_some() { config.dynlibpaths = self.dynlibpaths.unwrap() }
        if self.additional_qml_dirs.is_some() { config.additional_qml_dirs = self.additional_qml_dirs.unwrap() }
    }

    pub fn from_config(config: &ShoopConfig) -> Self {
        let defaults = ShoopConfig::default();
        let mut result = Self::default();
        if config.qml_dir != defaults.qml_dir { result.qml_dir = Some(config.qml_dir.clone()) }
        if config.lua_dir != defaults.lua_dir { result.lua_dir = Some(config.lua_dir.clone()) }
        if config.resource_dir != defaults.resource_dir { result.resource_dir = Some(config.resource_dir.clone()) }
        if config.schemas_dir != defaults.schemas_dir { result.schemas_dir = Some(config.schemas_dir.clone()) }
        if config.qt_plugins_dir != defaults.qt_plugins_dir { result.qt_plugins_dir = Some(config.qt_plugins_dir.clone()) }
        if config.pythonhome != defaults.pythonhome { result.pythonhome = Some(config.pythonhome.clone()) }
        if config.pythonpaths != defaults.pythonpaths { result.pythonpaths = Some(config.pythonpaths.clone()) }
        if config.dynlibpaths != defaults.dynlibpaths { result.dynlibpaths = Some(config.dynlibpaths.clone()) }
        if config.additional_qml_dirs != defaults.additional_qml_dirs { result.additional_qml_dirs = Some(config.additional_qml_dirs.clone()) }

        result
    }

    pub fn substitute_root(&mut self, root: &Path) {
        let substitute = |path: &mut Option<String>| {
            if let Some(path_str) = path {
                *path_str = path_str.replace("$ROOT", root.to_str().unwrap());
            }
        };
        substitute(&mut self.qml_dir);
        substitute(&mut self.lua_dir);
        substitute(&mut self.resource_dir);
        substitute(&mut self.schemas_dir);
        substitute(&mut self.qt_plugins_dir);
        substitute(&mut self.pythonhome);
        if self.pythonpaths.is_some() {
            self.pythonpaths = Some(self.pythonpaths.as_ref().unwrap().iter()
            .map(|s| s.replace("$ROOT", root.to_str().unwrap())).collect());
        }
        if self.dynlibpaths.is_some() {
            self.dynlibpaths = Some(self.dynlibpaths.as_ref().unwrap().iter()
            .map(|s| s.replace("$ROOT", root.to_str().unwrap())).collect());
        }
        if self.additional_qml_dirs.is_some() {
            self.additional_qml_dirs = Some(self.additional_qml_dirs.as_ref().unwrap().iter()
            .map(|s| s.replace("$ROOT", root.to_str().unwrap())).collect());
        }
    }
}

impl Default for ShoopConfig {
    fn default() -> ShoopConfig {
        let normalize_path = |path: &Path| -> PathBuf {
            PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
        };
        let executable_path = env::current_exe().unwrap();
        let installed_path = normalize_path(executable_path.parent().unwrap());
        ShoopConfig {
            version: common::shoop_version().to_string(),
            description: common::shoop_description().to_string(),
            install_info: format!("installed in {}", installed_path.to_str().unwrap()),
            qml_dir: String::from(""),
            lua_dir: String::from(""),
            resource_dir: String::from(""),
            schemas_dir: String::from(""),
            qt_plugins_dir: String::from(""),
            pythonhome : String::from(""),
            pythonpaths: vec![],
            dynlibpaths: vec![],
            additional_qml_dirs: vec![],
        }
    }
}

impl ShoopConfig {
    pub fn parse_toml_values(base_config : &ShoopConfig,
                             content_str : &str,
                             maybe_root_path : Option<&Path>) -> Result<ShoopConfig, anyhow::Error> {
        let mut config : ShoopConfig = base_config.clone();
        let mut toml_config : ShoopTomlConfig = toml::from_str(content_str)
            .context("Failed to parse TOML")?;
        if maybe_root_path.is_some() {
            toml_config.substitute_root(maybe_root_path.unwrap());
        }
        toml_config.apply_overrides(&mut config);
        Ok(config)
    }

    pub fn serialize_toml_values(self: &Self) -> Result<String, anyhow::Error> {
        let toml_config = ShoopTomlConfig::from_config(self);
        toml::to_string(&toml_config)
            .context("Could not serialize config to TOML")
    }

    pub fn load(config_path : &Path, maybe_substitute_root_path : Option<&Path>) -> Result<ShoopConfig, anyhow::Error> {
        let normalize_path = |path: &Path| -> PathBuf {
            PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
        };
        let config_path = normalize_path(config_path);

        let mut config : ShoopConfig = ShoopConfig::default();
        if std::fs::exists(&config_path).expect("Could not check for config file existence") {
            debug!("Loading config file: {:?}", config_path);
            let contents = std::fs::read_to_string(config_path)
                                                    .expect("Could not read config file");
            config = ShoopConfig::parse_toml_values(&mut config, contents.as_str(), maybe_substitute_root_path)
                        .expect("Could not parse config file");
        } else {
            return Err(anyhow::Error::msg(format!("Could not find config file: {:?}", config_path)));
        }
        Ok(config)
    }

    pub fn load_default(root_path : &Path) -> Result<ShoopConfig, anyhow::Error> {
        let normalize_path = |path: &Path| -> PathBuf {
            PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
        };

        let executable_path = env::current_exe().unwrap();
        // Assumption is that we are in {root}/bin
        let installed_path = normalize_path(executable_path.parent().unwrap());
        let config_path = env::var("SHOOP_CONFIG")
                        .map_or(installed_path.join("shoop-config.toml"), |v| PathBuf::from(v));
        if std::fs::exists(&config_path).expect("Could not check for config file existence") {
            return ShoopConfig::load(&config_path, Some(root_path));
        } else {
            debug!("No config file found, using defaults.");
            return Ok(ShoopConfig::default());
        }
    }
}