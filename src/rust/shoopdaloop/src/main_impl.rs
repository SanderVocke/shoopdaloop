use config::config::ShoopConfig;
use std::env;
use std::path::{Path, PathBuf};

use config;
use shoopdaloop::shoopdaloop_main;

use common::logging::macros::*;
shoop_log_unit!("Main");

pub fn main() {
    common::init().expect("Failed to initialize common");

    // For normalizing Windows paths
    let normalize_path = |path: &Path| -> PathBuf {
        PathBuf::from(
            std::fs::canonicalize(path)
                .expect("Failed to canonicalize path")
                .to_str()
                .expect("Path contains invalid UTF-8")
                .trim_start_matches(r"\\?\"),
        )
    };

    let executable_path = env::current_exe().expect("Failed to get current executable path");
    // Assumption is that we are in {root}/bin
    let installed_path = normalize_path(executable_path.parent().expect("Executable path has no parent"));

    let config: ShoopConfig =
        ShoopConfig::_load_default(&installed_path).expect("Failed to load config");

    let errcode = shoopdaloop_main(config);
    std::process::exit(errcode);
}
