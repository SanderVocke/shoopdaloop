use pyo3::prelude::*;
use std::env;

use shoopdaloop::shoopdaloop_main;
use shoopdaloop::add_lib_search_path::add_lib_search_path;
use shoopdaloop::shoop_app_info;

fn main() -> PyResult<()> {
    // Set up PYTHONPATH. This can deal with:
    // - Finding embedded pyenv in installed case (shoop_lib/py)
    let executable_path = env::current_exe().unwrap();
    let shoop_lib_path = executable_path.parent().unwrap().join("shoop_lib");
    let bundled_pythonpath = shoop_lib_path.join("py");
    if bundled_pythonpath.exists() {
        env::set_var("PYTHONPATH", bundled_pythonpath.to_str().unwrap());
    } else {
        println!("Warning: could not find PYTHONPATH for ShoopDaLoop. Attempting to run with default Python environment.");
    }

    add_lib_search_path(&shoop_lib_path);

    let install_info = format!("installed in {}", shoop_lib_path.join("..").to_str().unwrap());
    let shoop_app_info_module = 
                shoop_app_info::create_py_module
                    (py,
                     install_info.as_str(),
                     shoop_lib_path,
                     shoop_lib_path.join("qml"),
                     shoop_lib_path.join("py"),
                     shoop_lib_path.join("lua"),
                     shoop_lib_path.join("resources"),
                     shoop_lib_path.join("schemas"))
                    .unwrap();

    shoopdaloop_main(shoop_app_info_module)
}
