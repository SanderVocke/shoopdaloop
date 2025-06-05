use std::env;
use std::path::PathBuf;
use anyhow;
use anyhow::Context;
use backend;
use py_env;
use common;

const SRC_DIR : &str = env!("CARGO_MANIFEST_DIR");

#[path = "src/config.rs"]
mod config;

fn generate_dev_config() -> Result<config::ShoopConfig, anyhow::Error> {
    let shoop_src_root_dir = PathBuf::from(SRC_DIR).join("../../..");

    let dynlib_paths : Vec<String> = backend::runtime_link_dirs()
                                 .iter()
                                 .map(|p| p.to_string_lossy().to_string())
                                 .collect();
    let python_paths : Vec<String> =
        vec![ py_env::dev_env_pythonpath() ].into_iter()
        .chain(dynlib_paths.clone().into_iter()).collect();

    let mut config = config::ShoopConfig::default();
    config.qml_dir = shoop_src_root_dir.join("src/qml").to_str().unwrap().to_string();
    config.lua_dir = shoop_src_root_dir.join("src/lua").to_str().unwrap().to_string();
    config.resource_dir = shoop_src_root_dir.join("resources").to_str().unwrap().to_string();
    config.schemas_dir = shoop_src_root_dir.join("src/session_schemas").to_str().unwrap().to_string();
    config.pythonpaths = python_paths;
    config.dynlibpaths = dynlib_paths;

    Ok(config)
}

fn generate_portable_config() -> Result<config::ShoopConfig, anyhow::Error> {
    Ok(config::ShoopConfig::default())
}

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        println!("cargo:rustc-env=SHOOP_RUNTIME_LINK_PATHS=");
        return Ok(());
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let write_config = |filename : &PathBuf, config : &config::ShoopConfig| -> Result<(), anyhow::Error> {
            if filename.exists() { std::fs::remove_file(filename)?; }
            std::fs::write(filename, config.serialize_toml_values()?)?;
            Ok(())
        };
        // Write config files
        let configs_dir = PathBuf::from(env::var("OUT_DIR").unwrap().as_str());
        // std::fs::create_dir(&configs_dir)?;
        write_config(&configs_dir.join("shoop-dev-config.toml"), &generate_dev_config()?)?;
        write_config(&configs_dir.join("shoop-portable-config.toml"), &generate_portable_config()?)?;
        Ok(())
    }
}

fn main() {
    match main_impl() {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Error: {:?}\nBacktrace: {:?}", e, e.backtrace());
            std::process::exit(1);
        }
    }
}