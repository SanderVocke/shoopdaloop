use std::env;
use glob::glob;
use std::path::{Path, PathBuf};

use shoopdaloop::shoopdaloop_main;
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

    let executable_path = env::current_exe().unwrap();
    // Assumption is that we are in {root}/bin
    let installed_path = normalize_path(executable_path.parent().unwrap()
                                                       .parent().unwrap());

    // Set up PYTHONPATH.
    let python_lib = installed_path.join("lib").join("python");
    let site_packages = python_lib.join("site-packages");
    let pathsep = if cfg!(target_os = "windows") { ";" } else { ":" };
    let pythonpath = format!("{}{}{}", python_lib.to_str().unwrap(), pathsep, site_packages.to_str().unwrap());
    env::set_var("PYTHONPATH", py_env::dev_env_pythonpath().as_str());

    // let runtime_env_path = normalize_path(&installed_path.join("runtime"));
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

    let default_qml_dir = installed_path.join("lib/qml").to_str().unwrap().to_string();
    let default_lua_dir = installed_path.join("lib/lua").to_str().unwrap().to_string();
    let default_py_dir = installed_path.join("lib/python").to_str().unwrap().to_string();
    let default_resource_dir = installed_path.join("resources").to_str().unwrap().to_string();
    let default_schemas_dir = installed_path.join("lib/session_schemas").to_str().unwrap().to_string();

    let mut app_info = shoop_app_info::ShoopAppInfo::default();
    app_info.version = env!("CARGO_PKG_VERSION").to_string();
    app_info.description = env!("CARGO_PKG_DESCRIPTION").to_string();
    app_info.install_info = format!("installed in {}", installed_path.to_str().unwrap());
    app_info.qml_dir = env::var("SHOOP_OVERRIDE_QML_DIR").unwrap_or(default_qml_dir.clone());
    app_info.py_dir = env::var("SHOOP_OVERRIDE_PY_DIR").unwrap_or(default_py_dir.clone());
    app_info.lua_dir = env::var("SHOOP_OVERRIDE_LUA_DIR").unwrap_or(default_lua_dir.clone());
    app_info.resource_dir = env::var("SHOOP_OVERRIDE_RESOURCE_DIR").unwrap_or(default_resource_dir.clone());
    app_info.schemas_dir = env::var("SHOOP_OVERRIDE_SCHEMAS_DIR").unwrap_or(default_schemas_dir.clone());

    let errcode = shoopdaloop_main(app_info);
    std::process::exit(errcode);
}
