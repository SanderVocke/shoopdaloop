use std::env;
use std::path::PathBuf;
use anyhow;
use backend;
use py_env;

const SRC_DIR : &str = env!("CARGO_MANIFEST_DIR");

#[path = "src/config.rs"]
mod config;

fn dev_config_path() -> PathBuf {
    PathBuf::from(SRC_DIR).join("../../..").join("shoop-dev-config.toml")
}

fn generate_dev_config() -> Result<config::ShoopConfig, anyhow::Error> {
    let shoop_src_root_dir = PathBuf::from(SRC_DIR).join("../../..");

    let dynlib_paths : Vec<String> = backend::runtime_link_dirs()
                                 .iter()
                                 .map(|p| p.to_string_lossy().to_string())
                                 .collect();
    // For dev config, stack our Python source dir on top of the dev venv.
    // That way, any changes to Python code will be immediately used as opposed
    // to loading the most recently built wheel.
    let shoop_py_src = shoop_src_root_dir.join("src/python");
    let python_paths : Vec<String> =
        vec![ shoop_py_src.to_string_lossy().to_string(), py_env::dev_env_pythonpath() ].into_iter()
        .chain(dynlib_paths.clone().into_iter()).collect();

    let mut config = config::ShoopConfig::default();
    config.qml_dir = shoop_src_root_dir.join("src/qml").to_str().unwrap().to_string();
    config.lua_dir = shoop_src_root_dir.join("src/lua").to_str().unwrap().to_string();
    config.resource_dir = shoop_src_root_dir.join("resources").to_str().unwrap().to_string();
    config.schemas_dir = shoop_src_root_dir.join("src/session_schemas").to_str().unwrap().to_string();
    config.pythonhome = String::from("");
    config.pythonpaths = python_paths;
    config.dynlibpaths = dynlib_paths;

    Ok(config)
}

fn main_impl() -> Result<(), anyhow::Error> {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
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
        let dev_config = dev_config_path();
        let dev_config_str = dev_config.to_string_lossy();

        write_config(&dev_config, &generate_dev_config()?)?;

        println!("cargo:rustc-env=SHOOP_DEV_CONFIG_PATH={dev_config_str}");

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