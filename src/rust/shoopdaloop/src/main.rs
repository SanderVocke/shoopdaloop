use pyo3::prelude::*;
use std::env;

use shoopdaloop::shoopdaloop_main;
use shoopdaloop::add_lib_search_path::add_lib_search_path;
use shoopdaloop::shoop_app_info;

fn main() -> PyResult<()> {
    // Set up PYTHONPATH. This can deal with:
    // - Finding embedded pyenv in installed case (shoop_lib/py)
    let executable_path = env::current_exe().unwrap();
    let installed_path = executable_path.parent().unwrap().canonicalize().unwrap();
    let shoop_lib_path = installed_path.join("shoop_lib");
    let lib_path = installed_path.join("lib");
    let bundled_pythonpath_shoop_lib = std::fs::canonicalize(&shoop_lib_path).unwrap();
    let bundled_pythonpath_root = std::fs::canonicalize(&shoop_lib_path.join("py")).unwrap();
    if bundled_pythonpath_root.exists() {
        let pythonpath = format!("{}:{}", bundled_pythonpath_shoop_lib.to_str().unwrap(), bundled_pythonpath_root.to_str().unwrap());
        println!("using PYTHONPATH: {}", pythonpath.as_str());
        env::set_var("PYTHONPATH", pythonpath.as_str());
    } else {
        println!("Warning: could not find PYTHONPATH for ShoopDaLoop. Attempting to run with default Python environment.");
    }

    add_lib_search_path(&shoop_lib_path);
    add_lib_search_path(&lib_path);

    let mut app_info = shoop_app_info::ShoopAppInfo::default();
    app_info.version = env!("CARGO_PKG_VERSION").to_string();
    app_info.description = env!("CARGO_PKG_DESCRIPTION").to_string();
    app_info.install_info = format!("installed in {}", installed_path.to_str().unwrap());
    app_info.dynlib_dir = shoop_lib_path.to_str().unwrap().to_string();
    app_info.qml_dir = shoop_lib_path.join("qml").to_str().unwrap().to_string();
    app_info.py_dir = shoop_lib_path.join("py").to_str().unwrap().to_string();
    app_info.lua_dir = shoop_lib_path.join("lua").to_str().unwrap().to_string();
    app_info.resource_dir = shoop_lib_path.join("resources").to_str().unwrap().to_string();
    app_info.schemas_dir = shoop_lib_path.join("session_schemas").to_str().unwrap().to_string();

    shoopdaloop_main(app_info)
}
