use anyhow::{self, Context};
use toml;
use serde;
use std::env;
use std::path::{Path, PathBuf};

use common::logging::macros::*;
shoop_log_unit!("Main");

pub const VERSION : &str = env!("CARGO_PKG_VERSION");
pub const DESCRIPTION : &str = env!("CARGO_PKG_DESCRIPTION");

#[derive(Clone)]
pub struct ShoopConfig {
    pub version: String,
    pub description: String,
    pub install_info: String,
    pub qml_dir: String,
    pub lua_dir: String,
    pub resource_dir: String,
    pub schemas_dir: String,
    pub pythonpaths: Vec<String>,
    pub dynlibpaths: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ShoopTomlConfig {
    pub qml_dir : Option<String>,
    pub lua_dir : Option<String>,
    pub resource_dir : Option<String>,
    pub schemas_dir : Option<String>,
    pub pythonpaths : Option<Vec<String>>,
    pub dynlibpaths : Option<Vec<String>>,
}

impl ShoopTomlConfig {
    pub fn apply_overrides(self: Self, config : &mut ShoopConfig) {
        if self.qml_dir.is_some() { config.qml_dir = self.qml_dir.unwrap() }
        if self.lua_dir.is_some() { config.lua_dir = self.lua_dir.unwrap() }
        if self.resource_dir.is_some() { config.resource_dir = self.resource_dir.unwrap() }
        if self.schemas_dir.is_some() { config.schemas_dir = self.schemas_dir.unwrap() }
        if self.pythonpaths.is_some() { config.pythonpaths = self.pythonpaths.unwrap() }
    }

    pub fn from_config(config: &ShoopConfig) -> Self {
        let defaults = ShoopConfig::default();
        let mut result = Self::default();
        if config.qml_dir != defaults.qml_dir { result.qml_dir = Some(config.qml_dir.clone()) }
        if config.lua_dir != defaults.lua_dir { result.lua_dir = Some(config.lua_dir.clone()) }
        if config.resource_dir != defaults.resource_dir { result.resource_dir = Some(config.resource_dir.clone()) }
        if config.schemas_dir != defaults.schemas_dir { result.schemas_dir = Some(config.schemas_dir.clone()) }
        if config.pythonpaths != defaults.pythonpaths { result.pythonpaths = Some(config.pythonpaths.clone()) }

        result
    }
}

impl Default for ShoopConfig {
    fn default() -> ShoopConfig {
        let normalize_path = |path: &Path| -> PathBuf {
            PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
        };
        let executable_path = env::current_exe().unwrap();
        let installed_path = normalize_path(executable_path.parent().unwrap());

        let maybe_pythonpaths_env 
            = env::var("SHOOP_OVERRIDE_PYTHONPATHS").ok();
        let python_paths : Vec<String> =
            if maybe_pythonpaths_env.is_some() {
                maybe_pythonpaths_env
                    .unwrap()
                    .split(common::fs::PATH_LIST_SEPARATOR)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            } else {
                vec![installed_path.join("lib/python").to_str().unwrap().to_string()]
            };

        let maybe_dynlibpaths_env 
            = env::var("SHOOP_OVERRIDE_DYNLIBPATHS").ok();
        let dynlib_paths : Vec<String> =
            if maybe_dynlibpaths_env.is_some() {
                maybe_dynlibpaths_env
                    .unwrap()
                    .split(common::fs::PATH_LIST_SEPARATOR)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            } else {
                Vec::default()
            };
        
        let default_qml_dir = installed_path.join("lib/qml").to_str().unwrap().to_string();
        let default_lua_dir = installed_path.join("lib/lua").to_str().unwrap().to_string();
        let default_resource_dir = installed_path.join("resources").to_str().unwrap().to_string();
        let default_schemas_dir = installed_path.join("lib/session_schemas").to_str().unwrap().to_string();

        ShoopConfig {
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: env!("CARGO_PKG_DESCRIPTION").to_string(),
            install_info: format!("installed in {}", installed_path.to_str().unwrap()),
            qml_dir: env::var("SHOOP_OVERRIDE_QML_DIR").unwrap_or(default_qml_dir.clone()),
            lua_dir: env::var("SHOOP_OVERRIDE_LUA_DIR").unwrap_or(default_lua_dir.clone()),
            resource_dir: env::var("SHOOP_OVERRIDE_RESOURCE_DIR").unwrap_or(default_resource_dir.clone()),
            schemas_dir: env::var("SHOOP_OVERRIDE_SCHEMAS_DIR").unwrap_or(default_schemas_dir.clone()),
            pythonpaths: python_paths,
            dynlibpaths: dynlib_paths,
        }
    }
}

impl ShoopConfig {
    pub fn parse_toml_values(base_config : &ShoopConfig,
                             content_str : &str) -> Result<ShoopConfig, anyhow::Error> {
        let mut config : ShoopConfig = base_config.clone();
        let toml_config : ShoopTomlConfig = toml::from_str(content_str)
            .context("Failed to parse TOML")?;
        toml_config.apply_overrides(&mut config);
        Ok(config)
    }

    pub fn serialize_toml_values(self: &Self) -> Result<String, anyhow::Error> {
        let toml_config = ShoopTomlConfig::from_config(self);
        toml::to_string(&toml_config)
            .context("Could not serialize config to TOML")
    }

    pub fn load() -> Result<ShoopConfig, anyhow::Error> {
        let normalize_path = |path: &Path| -> PathBuf {
            PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
        };

        let executable_path = env::current_exe().unwrap();
        // Assumption is that we are in {root}/bin
        let installed_path = normalize_path(executable_path.parent().unwrap()
                                                       .parent().unwrap());

        let mut config : ShoopConfig = ShoopConfig::default();
        let config_path = env::var("SHOOP_CONFIG")
                        .map_or(installed_path.join("shoop-config.toml"), |v| PathBuf::from(v));
        if std::fs::exists(&config_path).expect("Could not check for config file existence") {
            debug!("Loading config file: {:?}", config_path);
            let contents = std::fs::read_to_string(config_path)
                                                    .expect("Could not read config file");
            config = ShoopConfig::parse_toml_values(&mut config, contents.as_str())
                        .expect("Could not parse config file");
        } else {
            debug!("No config file found, using defaults.");
        }
        Ok(config)
    }
}