use std::env;
use glob::glob;
use std::path::{Path, PathBuf};

use shoopdaloop::shoopdaloop_main;
use shoopdaloop::add_lib_search_path::add_lib_search_path;
use shoopdaloop::shoop_app_info;

pub fn main() {
    // For normalizing Windows paths
    let normalize_path = |path: &Path| -> PathBuf {
        PathBuf::from(std::fs::canonicalize(path).unwrap().to_str().unwrap().trim_start_matches(r"\\?\"))
    };

    // Set up PYTHONPATH. This can deal with:
    // - Finding embedded pyenv in installed case (shoop_lib/py)
    let executable_path = env::current_exe().unwrap();
    let installed_path = normalize_path(executable_path.parent().unwrap());
    let shoop_lib_path = normalize_path(&installed_path.join("shoop_lib"));
    let lib_path = installed_path.join("lib");
    let bundled_pythonpath_shoop_lib = &shoop_lib_path;
    let bundled_python_home = normalize_path(&shoop_lib_path.join("py"));
    let bundled_python_site_packages : PathBuf;
    #[cfg(target_os = "windows")]
    {
        bundled_python_site_packages = bundled_python_home.join("Lib/site-packages");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let pattern = format!("{}/**/site-packages", bundled_python_home.to_str().unwrap());
        let mut sp_glob = glob(&pattern).expect("Couldn't glob for site-packages");
        bundled_python_site_packages = sp_glob.next()
                .expect(format!("No site-packages dir found @ {}", pattern).as_str()).unwrap();
    }

    let bundled_python_lib_path = bundled_python_site_packages.parent().unwrap();
    let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
    if bundled_python_home.exists() &&
       bundled_pythonpath_shoop_lib.exists() &&
       bundled_python_site_packages.exists() &&
       bundled_python_lib_path.exists() {
        let pythonpath = format!("{}{sep}{}{sep}{}",
            bundled_python_lib_path.to_str().unwrap(),
            bundled_pythonpath_shoop_lib.to_str().unwrap(),
            bundled_python_site_packages.to_str().unwrap());
        // println!("using PYTHONPATH: {}", pythonpath.as_str());
        env::set_var("PYTHONPATH", pythonpath.as_str());
        // println!("using PYTHONHOME: {}", bundled_python_home.to_str().unwrap());
        env::set_var("PYTHONHOME", bundled_python_home.to_str().unwrap());
    } else {
        println!("Warning: could not find python paths for ShoopDaLoop. Attempting to run with default Python environment.");
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

    let errcode = shoopdaloop_main(app_info);
    std::process::exit(errcode);
}
