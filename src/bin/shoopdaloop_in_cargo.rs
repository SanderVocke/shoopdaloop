use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use std::env;
use std::path::PathBuf;
use glob::glob;

use shoopdaloop::shoopdaloop_main;
use shoopdaloop::add_lib_search_path::add_lib_search_path;

const SHOOP_BUILD_OUT_DIR : &str = env!("OUT_DIR");

fn main() -> PyResult<()> {
    // Set up PYTHONPATH. This can deal with:
    // finding pyenv in Cargo build case, based on the remembered OUT_DIR
    let base = PathBuf::from(SHOOP_BUILD_OUT_DIR).join("shoop_pyenv");
    let shoop_lib_dir = PathBuf::from(SHOOP_BUILD_OUT_DIR).join("shoop_lib");
    let pythonpath : PathBuf;
    let pattern = format!("{}/**/site-packages", base.to_str().unwrap());
    let mut sp_glob = glob(&pattern).unwrap();
    pythonpath = sp_glob.next()
            .expect(format!("No site-packages dir found @ {}", pattern).as_str())
            .unwrap();
    println!("using PYTHONPATH: {}", pythonpath.to_str().unwrap());
    env::set_var("PYTHONPATH", pythonpath.to_str().unwrap());

    add_lib_search_path(&shoop_lib_dir);

    shoopdaloop_main(&shoop_lib_dir)
}
