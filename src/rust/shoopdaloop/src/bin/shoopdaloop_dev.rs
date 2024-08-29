use pyo3::prelude::*;
use std::env;
use std::path::PathBuf;
use glob::glob;

use shoopdaloop::shoopdaloop_main;
use shoopdaloop::add_lib_search_path::add_lib_search_path;
use shoopdaloop::shoop_app_info;

const SHOOP_BUILD_OUT_DIR : &str = env!("OUT_DIR");
const SRC_DIR : &str = env!("CARGO_MANIFEST_DIR");

fn main() -> PyResult<()> {
    // Set up PYTHONPATH. This can deal with:
    // finding pyenv in Cargo build case, based on the remembered OUT_DIR
    let base = PathBuf::from(SHOOP_BUILD_OUT_DIR).join("shoop_pyenv");
    let shoop_lib_dir = PathBuf::from(SHOOP_BUILD_OUT_DIR).join("shoop_lib");
    let shoop_src_root_dir = PathBuf::from(SRC_DIR).join('../../../../..');
    let pythonpath : PathBuf;
    let pattern = format!("{}/**/site-packages", base.to_str().unwrap());
    let mut sp_glob = glob(&pattern).unwrap();
    pythonpath = sp_glob.next()
            .expect(format!("No site-packages dir found @ {}", pattern).as_str())
            .unwrap();
    println!("using PYTHONPATH: {}", pythonpath.to_str().unwrap());
    env::set_var("PYTHONPATH", pythonpath.to_str().unwrap());

    add_lib_search_path(&shoop_lib_dir);

    let install_info = format!("editable dev install in {}",
                    shoop_src_root_dir);
    let shoop_app_info_module = 
                shoop_app_info::create_py_module
                    (py,
                        install_info.as_str(),
                        shoop_lib_dir,
                        shoop_src_root_dir.join("src/qml"),
                        shoop_src_root_dir.join("src/python/shoopdaloop"),
                        shoop_src_root_dir.join("src/lua"),
                        shoop_src_root_dir.join("resources"),
                        shoop_src_root_dir.join("src/session_schemas"))
                    .unwrap();

    shoopdaloop_main(shoop_app_info_module)
}
