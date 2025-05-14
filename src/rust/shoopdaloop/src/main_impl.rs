use std::env;
use glob::glob;
use std::path::{Path, PathBuf};

use shoopdaloop::shoopdaloop_main;
use shoopdaloop::add_lib_search_path::add_lib_search_path;
use shoopdaloop::shoop_app_info;

use common::logging::macros::*;
shoop_log_unit!("Main");

// const SHOOP_ENV_DIR_TO_SITE_PACKAGES: &str = env!("SHOOP_ENV_DIR_TO_SITE_PACKAGES");
// const SHOOP_ENV_DIR_TO_LIBS: &str = env!("SHOOP_ENV_DIR_TO_RUNTIME_LIB");
// const SHOOP_ENV_DIR_TO_PYTHON_LIBS : &str = env!("SHOOP_ENV_DIR_TO_PYTHON_LIBS");

pub fn main() {
    common::init().unwrap();

    // For normalizing Windows paths
    let normalize_path = |path: &Path| -> PathBuf {
        PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
    };

    // Set up PYTHONPATH. This can deal with:
    // - Finding embedded pyenv in installed case (shoop_lib/py)
    let executable_path = env::current_exe().unwrap();
    let installed_path = normalize_path(executable_path.parent().unwrap());
    let runtime_env_path = normalize_path(&installed_path.join("runtime"));
    // let lib_path = normalize_path(&installed_path.join(SHOOP_ENV_DIR_TO_LIBS));
    // let bundled_pythonpath_shoop_lib = &runtime_env_path;
    // let bundled_python_site_packages = normalize_path(&runtime_env_path.join(SHOOP_ENV_DIR_TO_SITE_PACKAGES));
    // let bundled_python_libs = normalize_path(&runtime_env_path.join(SHOOP_ENV_DIR_TO_PYTHON_LIBS));

    // let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
    // if bundled_pythonpath_shoop_lib.exists() &&
    //    bundled_python_site_packages.exists() &&
    //    lib_path.exists() {
    //     let pythonpath = format!("{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}",
    //                      bundled_python_site_packages.to_str().unwrap(),
    //                      bundled_python_libs.to_str().unwrap(),
    //                      bundled_python_libs.join("lib-dynload").to_str().unwrap(),
    //                      bundled_python_libs.join("importlib").to_str().unwrap(),
    //                      bundled_python_libs.parent().unwrap().join("DLLs").to_str().unwrap(),
    //                      bundled_python_site_packages.join("win32").to_str().unwrap(),
    //                      bundled_python_site_packages.join("win32").join("lib").to_str().unwrap(),
    //                      lib_path.to_str().unwrap(),
    //         );
    //     debug!("using PYTHONPATH: {}", pythonpath.as_str());
    //     env::set_var("PYTHONPATH", pythonpath.as_str());
    //     debug!("using PYTHONHOME: {}", bundled_python_libs.to_str().unwrap());
    //     env::set_var("PYTHONHOME", bundled_python_libs.to_str().unwrap());
    // } else {
    //     println!("Warning: could not find python paths for ShoopDaLoop. Attempting to run with default Python environment.");
    // }

    // add_lib_search_path(&runtime_env_path);
    // add_lib_search_path(&lib_path);

    let mut app_info = shoop_app_info::ShoopAppInfo::default();
    app_info.version = env!("CARGO_PKG_VERSION").to_string();
    app_info.description = env!("CARGO_PKG_DESCRIPTION").to_string();
    app_info.install_info = format!("installed in {}", installed_path.to_str().unwrap());
    app_info.dynlib_dir = runtime_env_path.to_str().unwrap().to_string();
    app_info.qml_dir = runtime_env_path.join("qml").to_str().unwrap().to_string();
    app_info.py_dir = runtime_env_path.join("py").to_str().unwrap().to_string();
    app_info.lua_dir = runtime_env_path.join("lua").to_str().unwrap().to_string();
    app_info.resource_dir = runtime_env_path.join("resources").to_str().unwrap().to_string();
    app_info.schemas_dir = runtime_env_path.join("session_schemas").to_str().unwrap().to_string();

    let errcode = shoopdaloop_main(app_info);
    std::process::exit(errcode);
}
