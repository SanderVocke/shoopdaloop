use std::env;
use config::config::ShoopConfig;
use std::path::{Path, PathBuf};

use shoopdaloop::shoopdaloop_main;
use config;

use common::logging::macros::*;
shoop_log_unit!("Main");

pub fn main() {
    common::init().unwrap();
    frontend::engine_update_thread::init();

    // For normalizing Windows paths
    let normalize_path = |path: &Path| -> PathBuf {
        PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
    };

    let executable_path = env::current_exe().unwrap();
    // Assumption is that we are in {root}/bin
    let installed_path = normalize_path(executable_path.parent().unwrap());

    let config : ShoopConfig = ShoopConfig::load_default(&installed_path).expect("Failed to load config");

    let errcode = shoopdaloop_main(config);
    std::process::exit(errcode);
}
