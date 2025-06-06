const OUT_DIR : &str = env!("OUT_DIR");
use std::env;
use std::path::PathBuf;
use common::logging::macros::*;

shoop_log_unit!("Main");

fn main() -> std::io::Result<()> {
    common::init().unwrap();
    let config_path= PathBuf::from(OUT_DIR).join("shoop-dev-config.toml");
    debug!("Using config: {config_path:?}");
    env::set_var("SHOOP_CONFIG", config_path.to_string_lossy().to_string());
    config::launcher::launcher()
}