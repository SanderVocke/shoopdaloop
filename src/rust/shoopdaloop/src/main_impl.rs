use config::config::ShoopConfig;
use std::env;
use std::path::{Path, PathBuf};

use config;
use shoopdaloop::shoopdaloop_main;

use common::logging::macros::*;
shoop_log_unit!("Main");

pub fn main() {
    if std::env::args().any(|arg| arg == "--tracing") {
        common::tracing_helpers::set_tracing_enabled(true);
    }

    if let Err(e) = common::init() {
        eprintln!("Failed to initialize common: {}", e);
        std::process::exit(1);
    }

    // For normalizing Windows paths
    let normalize_path = |path: &Path| -> Result<PathBuf, String> {
        let canonical = std::fs::canonicalize(path)
            .map_err(|e| format!("Failed to canonicalize path: {}", e))?;
        let s = canonical.to_str().ok_or("Path contains invalid UTF-8")?;
        Ok(PathBuf::from(s.trim_start_matches(r"\\?\")))
    };

    let run = || -> Result<i32, String> {
        let executable_path = env::current_exe()
            .map_err(|e| format!("Failed to get current executable path: {}", e))?;

        let parent = executable_path
            .parent()
            .ok_or("Executable path has no parent")?;

        let installed_path = normalize_path(parent)?;

        let config: ShoopConfig = ShoopConfig::_load_default(&installed_path)
            .map_err(|e| format!("Failed to load config: {}", e))?;

        Ok(shoopdaloop_main(config))
    };

    match run() {
        Ok(errcode) => std::process::exit(errcode),
        Err(e) => {
            eprintln!("Error during startup: {}", e);
            std::process::exit(1);
        }
    }
}
