use config::config::ShoopConfig;
use std::env;
use std::path::{Path, PathBuf};

use config;
use shoopdaloop::shoopdaloop_main;

use common::logging::macros::*;
shoop_log_unit!("Main");

pub fn main() {
    common::init().unwrap();
    let _crash_handler = crashhandling::init_crashhandling(
        std::env::args().any(|arg| arg == "--crash-handling-server"),
        "--crash-handling-server",
    );
    info!("Start client!");

    // For normalizing Windows paths
    let normalize_path = |path: &Path| -> PathBuf {
        PathBuf::from(
            std::fs::canonicalize(path)
                .unwrap()
                .to_str()
                .unwrap()
                .trim_start_matches(r"\\?\"),
        )
    };

    let executable_path = env::current_exe().unwrap();
    // Assumption is that we are in {root}/bin
    let installed_path = normalize_path(executable_path.parent().unwrap());

    let config: ShoopConfig =
        ShoopConfig::_load_default(&installed_path).expect("Failed to load config");

    let errcode = shoopdaloop_main(config);
    std::process::exit(errcode);
}
