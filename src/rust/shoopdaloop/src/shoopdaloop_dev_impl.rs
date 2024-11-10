use std::env;
use std::path::PathBuf;
use glob::glob;

use crate::shoopdaloop_main;
use crate::add_lib_search_path::add_lib_search_path;
use crate::shoop_app_info;

const SHOOP_BUILD_OUT_DIR : &str = env!("OUT_DIR");
const SRC_DIR : &str = env!("CARGO_MANIFEST_DIR");

pub fn main() {
    // Set up PYTHONPATH. This can deal with:
    // finding pyenv in Cargo build case, based on the remembered OUT_DIR
    let shoop_lib_dir = std::fs::canonicalize(PathBuf::from(SHOOP_BUILD_OUT_DIR).join("shoop_lib")).unwrap();
    let bundled_python_home = shoop_lib_dir.join("py");
    let bundled_python_site_packages : PathBuf;

    #[cfg(target_os = "windows")]
    {
        bundled_python_site_packages = bundled_python_home.join("Lib/site-packages");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let pattern = format!("{}/**/site-packages", bundled_python_home.to_str().unwrap());
        let mut sp_glob = glob(&pattern).unwrap();
        let bundled_python_site_packages = std::fs::canonicalize(
                sp_glob.next()
                .expect(format!("No site-packages dir found @ {}", pattern).as_str())
                .unwrap()
            ).unwrap();
    }

    let shoop_src_root_dir = std::fs::canonicalize(PathBuf::from(SRC_DIR).join("../../..")).unwrap();    
    let pythonpath_to_src = std::fs::canonicalize(
        shoop_src_root_dir.join("src/python")).unwrap();
    let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
    let pythonpath = format!("{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}{sep}{}",
                         pythonpath_to_src.to_str().unwrap(),
                         bundled_python_site_packages.to_str().unwrap(),
                         bundled_python_site_packages.parent().unwrap().to_str().unwrap(),
                         bundled_python_site_packages.parent().unwrap().join("lib-dynload").to_str().unwrap(),
                         bundled_python_site_packages.parent().unwrap().join("importlib").to_str().unwrap(),
                         bundled_python_site_packages.parent().unwrap().parent().unwrap().join("DLLs").to_str().unwrap(),
                         bundled_python_site_packages.join("win32").to_str().unwrap(),
                         bundled_python_site_packages.join("win32").join("lib").to_str().unwrap(),
                         shoop_lib_dir.to_str().unwrap(),
                         );
    println!("using PYTHONPATH: {}", pythonpath.as_str());
    env::set_var("PYTHONPATH", pythonpath.as_str());

    add_lib_search_path(&shoop_lib_dir);

    let mut app_info = shoop_app_info::ShoopAppInfo::default();
    app_info.version = env!("CARGO_PKG_VERSION").to_string();
    app_info.description = env!("CARGO_PKG_DESCRIPTION").to_string();
    app_info.install_info = format!("editable dev install in {}",
                    shoop_src_root_dir.to_str().unwrap());
    app_info.dynlib_dir = shoop_lib_dir.to_str().unwrap().to_string();
    app_info.qml_dir = shoop_src_root_dir.join("src/qml").to_str().unwrap().to_string();
    app_info.py_dir = shoop_src_root_dir.join("src/python/shoopdaloop").to_str().unwrap().to_string();
    app_info.lua_dir = shoop_src_root_dir.join("src/lua").to_str().unwrap().to_string();
    app_info.resource_dir = shoop_src_root_dir.join("resources").to_str().unwrap().to_string();
    app_info.schemas_dir = shoop_src_root_dir.join("src/session_schemas").to_str().unwrap().to_string();

    let errcode = shoopdaloop_main(app_info);
    std::process::exit(errcode);
}
