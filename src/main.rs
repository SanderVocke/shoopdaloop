use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;
use std::path::PathBuf;
use glob::glob;

use shoopdaloop::shoopdaloop_main;
use shoopdaloop::add_lib_search_path::add_lib_search_path;

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

    shoopdaloop_main(&shoop_lib_path)
}
