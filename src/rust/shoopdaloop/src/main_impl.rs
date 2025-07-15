use config::config::ShoopConfig;
use std::env;
use std::path::{Path, PathBuf};

use config;
use shoopdaloop::shoopdaloop_main;

use common::logging::macros::*;
shoop_log_unit!("Main");

fn crash_info_callback_impl() -> Result<Vec<crashhandling::AdditionalCrashAttachment>, anyhow::Error> {
    let maybe_qml_engine = frontend::cxx_qt_shoop::type_shoopqmlapplicationengine::get_registered_qml_engine()?;
    let mut qml_stack : String = "".to_string();
    if !maybe_qml_engine.is_null() {
        unsafe {
            let maybe_qml_engine = &*maybe_qml_engine;
            qml_stack = frontend::cxx_qt_shoop::type_shoopqmlapplicationengine::get_qml_engine_stack_trace(maybe_qml_engine);
        }
    }
    let info = crashhandling::AdditionalCrashAttachment {
        id: "qml_stack".to_string(),
        contents: qml_stack,
    };

    Ok(vec![info])
}

fn crash_info_callback() -> Vec<crashhandling::AdditionalCrashAttachment> {
    match crash_info_callback_impl() {
        Ok(r) => return r,
        Err(e) => {
            error!("Could not gather additional crash info: {e}");
        }
    }
    return Vec::new();
}

pub fn main() {
    common::init().unwrap();
    crashhandling::init_crashhandling(
        std::env::args().any(|arg| arg == "--crash-handling-server"),
        "--crash-handling-server",
        Some(crash_info_callback),
    );
    info!("Start client!");
    let args: Vec<String> = std::env::args().collect();
    crashhandling::set_crash_json_extra("cmdline", serde_json::json!(args.join(" ")));

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
