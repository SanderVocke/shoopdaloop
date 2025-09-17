use anyhow;
use backend;
use std::env;
use std::path::PathBuf;
use std::process::Command;

const SRC_DIR: &str = env!("CARGO_MANIFEST_DIR");
const MAYBE_QMAKE: Option<&'static str> = option_env!("QMAKE");

fn qmake_command(qmake_path: &str, argstring: &str) -> Command {
    let shell_command = format!("{} {}", qmake_path, argstring);
    return if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", format!("{shell_command}").as_str()]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", format!("{shell_command}").as_str()]);
        cmd
    };
}

#[path = "src/config.rs"]
mod config;

fn dev_config_path() -> PathBuf {
    PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("shoop-dev-config.toml")
}

fn generate_dev_config() -> Result<config::ShoopConfig, anyhow::Error> {
    let shoop_src_root_dir = PathBuf::from(SRC_DIR).join("../../..");
    let qmake = if MAYBE_QMAKE.is_some() {
        MAYBE_QMAKE.unwrap()
    } else {
        "qmake"
    };
    let qt_plugins = qmake_command(qmake, "-query QT_INSTALL_PLUGINS")
        .stderr(std::process::Stdio::inherit())
        .output()?;
    let qt_plugins = String::from_utf8(qt_plugins.stdout)?.trim().to_string();
    let qt_qml = qmake_command(qmake, "-query QT_INSTALL_QML")
        .stderr(std::process::Stdio::inherit())
        .output()?;
    let qt_qml = String::from_utf8(qt_qml.stdout)?.trim().to_string();

    let dynlib_paths: Vec<String> = backend::runtime_link_dirs()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let mut config = config::ShoopConfig::default();
    config.qml_dir = shoop_src_root_dir
        .join("src/qml")
        .to_str()
        .unwrap()
        .to_string();
    config.lua_dir = shoop_src_root_dir
        .join("src/lua")
        .to_str()
        .unwrap()
        .to_string();
    config.resource_dir = shoop_src_root_dir
        .join("resources")
        .to_str()
        .unwrap()
        .to_string();
    config.schemas_dir = shoop_src_root_dir
        .join("src/session_schemas")
        .to_str()
        .unwrap()
        .to_string();
    config.qt_plugins_dir = qt_plugins;
    config.dynlibpaths = dynlib_paths;
    config.additional_qml_dirs = vec![qt_qml];

    Ok(config)
}

fn main_impl() -> Result<(), anyhow::Error> {
    let write_config =
        |filename: &PathBuf, config: &config::ShoopConfig| -> Result<(), anyhow::Error> {
            if filename.exists() {
                std::fs::remove_file(filename)?;
            }
            let content = config.serialize_toml_values()?;
            println!("CONFIG! {content}");
            std::fs::write(filename, content)?;
            Ok(())
        };
    // Write config files
    let dev_config = dev_config_path();
    let dev_config_str = dev_config.to_string_lossy();

    write_config(&dev_config, &generate_dev_config()?)?;

    println!("cargo:rustc-env=SHOOP_DEV_CONFIG_PATH={dev_config_str}");

    Ok(())
}

fn main() {
    match main_impl() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {:?}\nBacktrace: {:?}", e, e.backtrace());
            std::process::exit(1);
        }
    }
}
