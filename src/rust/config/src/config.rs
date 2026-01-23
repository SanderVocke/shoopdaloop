use anyhow::{self, Context};
use serde;
use std::env;
use std::path::{Path, PathBuf};
use toml;

use common::logging::macros::*;
shoop_log_unit!("Main");

#[derive(Clone, Debug)]
pub struct ShoopConfig {
    pub _version: String,
    pub _description: String,
    pub _install_info: String,
    pub qml_dir: String,
    pub lua_dir: String,
    pub resource_dir: String,
    pub schemas_dir: String,
    pub qt_plugins_dir: String,
    pub dynlibpaths: Vec<String>,
    pub additional_qml_dirs: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ShoopTomlConfig {
    pub qml_dir: Option<String>,
    pub lua_dir: Option<String>,
    pub resource_dir: Option<String>,
    pub schemas_dir: Option<String>,
    pub qt_plugins_dir: Option<String>,
    pub dynlibpaths: Option<Vec<String>>,
    pub additional_qml_dirs: Option<Vec<String>>,
}

impl ShoopTomlConfig {
    pub fn _apply_overrides(self: Self, config: &mut ShoopConfig) {
        if let Some(v) = self.qml_dir {
            config.qml_dir = v;
        }
        if let Some(v) = self.lua_dir {
            config.lua_dir = v;
        }
        if let Some(v) = self.resource_dir {
            config.resource_dir = v;
        }
        if let Some(v) = self.schemas_dir {
            config.schemas_dir = v;
        }
        if let Some(v) = self.qt_plugins_dir {
            config.qt_plugins_dir = v;
        }
        if let Some(v) = self.dynlibpaths {
            config.dynlibpaths = v;
        }
        if let Some(v) = self.additional_qml_dirs {
            config.additional_qml_dirs = v;
        }
    }

    pub fn from_config(config: &ShoopConfig) -> Self {
        let defaults = ShoopConfig::default();
        let mut result = Self::default();
        if config.qml_dir != defaults.qml_dir {
            result.qml_dir = Some(config.qml_dir.clone())
        }
        if config.lua_dir != defaults.lua_dir {
            result.lua_dir = Some(config.lua_dir.clone())
        }
        if config.resource_dir != defaults.resource_dir {
            result.resource_dir = Some(config.resource_dir.clone())
        }
        if config.schemas_dir != defaults.schemas_dir {
            result.schemas_dir = Some(config.schemas_dir.clone())
        }
        if config.qt_plugins_dir != defaults.qt_plugins_dir {
            result.qt_plugins_dir = Some(config.qt_plugins_dir.clone())
        }
        if config.dynlibpaths != defaults.dynlibpaths {
            result.dynlibpaths = Some(config.dynlibpaths.clone())
        }
        if config.additional_qml_dirs != defaults.additional_qml_dirs {
            result.additional_qml_dirs = Some(config.additional_qml_dirs.clone())
        }

        result
    }

    pub fn _substitute_root(&mut self, root: &Path) {
        let root_str = root.to_string_lossy().into_owned();
        let substitute = |path: &mut Option<String>| {
            if let Some(path_str) = path {
                *path_str = path_str.replace("$ROOT", &root_str);
            }
        };
        substitute(&mut self.qml_dir);
        substitute(&mut self.lua_dir);
        substitute(&mut self.resource_dir);
        substitute(&mut self.schemas_dir);
        substitute(&mut self.qt_plugins_dir);
        if let Some(dynlibpaths) = &mut self.dynlibpaths {
            for s in dynlibpaths.iter_mut() {
                *s = s.replace("$ROOT", &root_str);
            }
        }
        if let Some(additional_qml_dirs) = &mut self.additional_qml_dirs {
            for s in additional_qml_dirs.iter_mut() {
                *s = s.replace("$ROOT", &root_str);
            }
        }
    }
}

impl Default for ShoopConfig {
    fn default() -> ShoopConfig {
        let normalize_path = |path: &Path| -> PathBuf {
            match std::fs::canonicalize(path) {
                Ok(p) => PathBuf::from(
                    p.to_string_lossy()
                        .trim_start_matches(r"\\?\")
                        .to_string(),
                ),
                Err(_) => path.to_path_buf(),
            }
        };
        let executable_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("shoopdaloop"));
        let installed_path = normalize_path(executable_path.parent().unwrap_or(Path::new(".")));
        ShoopConfig {
            _version: common::shoop_version().to_string(),
            _description: common::shoop_description().to_string(),
            _install_info: format!("installed in {}", installed_path.to_string_lossy()),
            qml_dir: String::from(""),
            lua_dir: String::from(""),
            resource_dir: String::from(""),
            schemas_dir: String::from(""),
            qt_plugins_dir: String::from(""),
            dynlibpaths: vec![],
            additional_qml_dirs: vec![],
        }
    }
}

impl ShoopConfig {
    pub fn _parse_toml_values(
        base_config: &ShoopConfig,
        content_str: &str,
        maybe_root_path: Option<&Path>,
    ) -> Result<ShoopConfig, anyhow::Error> {
        let mut config: ShoopConfig = base_config.clone();
        let mut toml_config: ShoopTomlConfig =
            toml::from_str(content_str).context("Failed to parse TOML")?;
        if let Some(root_path) = maybe_root_path {
            toml_config._substitute_root(root_path);
        }
        toml_config._apply_overrides(&mut config);
        Ok(config)
    }

    pub fn serialize_toml_values(self: &Self) -> Result<String, anyhow::Error> {
        let toml_config = ShoopTomlConfig::from_config(self);
        toml::to_string(&toml_config).context("Could not serialize config to TOML")
    }

    pub fn _load(
        config_path: &Path,
        maybe_substitute_root_path: Option<&Path>,
    ) -> Result<ShoopConfig, anyhow::Error> {
        let normalize_path = |path: &Path| -> PathBuf {
            match std::fs::canonicalize(path) {
                Ok(p) => PathBuf::from(
                    p.to_string_lossy()
                        .trim_start_matches(r"\\?\")
                        .to_string(),
                ),
                Err(_) => path.to_path_buf(),
            }
        };
        let config_path = normalize_path(config_path);

        let mut config: ShoopConfig = ShoopConfig::default();
        if config_path.exists() {
            debug!("Loading config file: {:?}", config_path);
            let contents = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Could not read config file: {:?}", config_path))?;
            config = ShoopConfig::_parse_toml_values(
                &config,
                contents.as_str(),
                maybe_substitute_root_path,
            )
            .with_context(|| format!("Could not parse config file: {:?}", config_path))?;
        } else {
            return Err(anyhow::Error::msg(format!(
                "Could not find config file: {:?}",
                config_path
            )));
        }
        Ok(config)
    }

    pub fn _load_default(root_path: &Path) -> Result<ShoopConfig, anyhow::Error> {
        let normalize_path = |path: &Path| -> PathBuf {
            match std::fs::canonicalize(path) {
                Ok(p) => PathBuf::from(
                    p.to_string_lossy()
                        .trim_start_matches(r"\\?\")
                        .to_string(),
                ),
                Err(_) => path.to_path_buf(),
            }
        };

        let executable_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("shoopdaloop"));
        // Assumption is that we are in {root}/bin
        let installed_path = normalize_path(executable_path.parent().unwrap_or(Path::new(".")));
        let config_path = env::var("SHOOP_CONFIG")
            .map_or(installed_path.join("shoop-config.toml"), |v| {
                PathBuf::from(v)
            });
        if config_path.exists() {
            return ShoopConfig::_load(&config_path, Some(root_path));
        } else {
            debug!("No config file found, using defaults.");
            return Ok(ShoopConfig::default());
        }
    }
}
