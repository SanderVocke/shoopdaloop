use config::config::ShoopConfig;
use std::env;
use std::path::{Path, PathBuf};

use config;
use shoopdaloop::shoopdaloop_main;

use common::logging::macros::*;
shoop_log_unit!("Main");

pub fn main() {
    let errcode = match || -> Result<i32, anyhow::Error> {
        common::init()?;

        // For normalizing Windows paths
        let normalize_path = |path: &Path| -> Result<PathBuf, anyhow::Error> {
            Ok(PathBuf::from(
                std::fs::canonicalize(path)
                    .map_err(|e| anyhow::anyhow!("Could not canonicalize path: {e}"))?
                    .to_str()
                    .ok_or(anyhow::anyhow!("Could not convert path to string"))?
                    .trim_start_matches(r"\\?\"),
            ))
        };

        let executable_path = env::current_exe()
            .map_err(|e| anyhow::anyhow!("Could not find current executable: {e}"))?;
        // Assumption is that we are in {root}/bin
        let installed_path = normalize_path(
            executable_path
                .parent()
                .ok_or(anyhow::anyhow!("Executable path has no parent directory"))?,
        )?;

        let config: ShoopConfig = ShoopConfig::_load_default(&installed_path)?;
        Ok(shoopdaloop_main(config))
    }() {
        Ok(code) => code,
        Err(e) => {
            error!("Failed to initialize: {e}");
            1
        }
    };

    std::process::exit(errcode);
}
