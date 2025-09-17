pub mod config;
use std::path::PathBuf;

use common::logging::macros::*;
shoop_log_unit!("Config");

pub fn dev_config_path() -> PathBuf {
    PathBuf::from(option_env!("SHOOP_DEV_CONFIG_PATH").unwrap())
}

use anyhow;

pub fn config_dynlib_env_var(
    config: &config::ShoopConfig,
) -> Result<(String, String), anyhow::Error> {
    let runtime_link_path_var = if cfg!(target_os = "windows") {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };

    let mut result = std::env::var(runtime_link_path_var)
        .unwrap_or_default()
        .to_string();

    if config.dynlibpaths.len() > 0 {
        let dynlibpaths_string = config.dynlibpaths.join(common::util::PATH_LIST_SEPARATOR);

        debug!(
            "Adding to {}: {}",
            runtime_link_path_var, dynlibpaths_string
        );

        result = format!(
            "{}{}{}",
            dynlibpaths_string,
            common::util::PATH_LIST_SEPARATOR,
            &result
        );
    }

    Ok((runtime_link_path_var.to_string(), result))
}
