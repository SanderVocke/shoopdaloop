use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;
use common::fs::copy_dir_merge;
use regex::Regex;
use copy_dir::copy_dir;
use glob::glob;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_folder(
    folder : &Path,
    exe_path : &Path,
    launcher_path : &Path,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(6).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    let final_exe_path = folder.join("shoopdaloop.exe");
    let final_exe_dir = final_exe_path.parent().ok_or(anyhow::anyhow!("Could not get executable directory"))?;
    info!("Using source path {src_path:?}");

    let lib_dir = folder.join("lib");
    std::fs::create_dir(&lib_dir)
        .with_context(|| format!("Cannot create dir: {:?}", lib_dir))?;

    let py_lib_dir = lib_dir.join("python");
    std::fs::create_dir(&py_lib_dir)
        .with_context(|| format!("Cannot create dir: {:?}", py_lib_dir))?;

    info!("Bundling executable...");
    std::fs::copy(exe_path, &final_exe_path)
        .with_context(|| format!("Failed to copy {exe_path:?} to {folder:?}"))?;

    info!("Bundling development environment Python packages from PYTHONPATH...");
    let python_lib_paths = crate::remove_subpaths::remove_subpaths(&py_env::dev_env_pythonpath_entries());
    let py_folder_regex = Regex::new(r"python[0-9]+\.[0-9]+").unwrap();
    for path in python_lib_paths {
        if path.ends_with("site-packages") {
            debug!("--> {} -> site-packages", path);
            copy_dir_merge(&path, &py_lib_dir.join("site-packages"))?;
        } else if PathBuf::from(path.clone()).is_dir() {
            debug!("--> {} -> python", path);
            copy_dir_merge(&path, &py_lib_dir)?;
        } else {
            debug!("--> {} -> ignored (not a directory)", path);
        }
    }

    // Copy filesets into our output lib dir
    let to_copy = [
        ("src/lua", "lib/lua"),
        ("src/qml", "lib/qml"),
        ("src/session_schemas", "lib/session_schemas"),
        ("resources", "resources"),
    ];
    info!("Bundling source assets...");
    for (from, to) in to_copy {
        let src = src_path.join(from);
        let dst = folder.join(to);
        debug!("--> {:?} -> {:?}", src, dst);
        copy_dir(&src, &dst)?;
    }

    // FIXME: this is ugly to have to do explicitly. But since these are lazy-loaded,
    // we cannot autodetect them.
    info!("Bundling Qt plugins...");
    let zita_dir = backend::zita_link_implib_dir();
    let pkg_base_dir = zita_dir.parent().unwrap();
    let pattern = format!("{}/Qt6/plugins", pkg_base_dir.to_string_lossy());
    let plugins_dir = lib_dir.join("qt_plugins");
    let mut entries = glob(&pattern)?;
    let entry = entries.next().unwrap();
    debug!("--> {:?} -> {:?}", entry, plugins_dir);
    copy_dir(entry?, &plugins_dir)?;

    // FIXME: this is ugly to have to do explicitly. But since these are lazy-loaded,
    // we cannot autodetect them.
    info!("Bundling Qt QML modules...");
    let zita_dir = backend::zita_link_implib_dir();
    let pkg_base_dir = zita_dir.parent().unwrap();
    let pattern = format!("{}/Qt6/qml", pkg_base_dir.to_string_lossy());
    let qml_dir = folder.join("lib/qml");
    let mut entries = glob(&pattern)?;
    let entry = entries.next().unwrap();
    debug!("--> {:?} -> {:?}", entry, qml_dir);
    copy_dir_merge(entry?, &qml_dir)?;

    info!("Getting dependencies (this may take some time)...");
    let excludelist_path = src_path.join("distribution/windows/excludelist");
    let includelist_path = src_path.join("distribution/windows/includelist");
    for path in backend::all_link_search_paths() {
        debug!("--> extra search path: {:?}", path);
        common::env::add_lib_search_path(&path);
    }
    // Also include search paths to all of Qt's plugin directories
    let plugin_subdirs = std::fs::read_dir(plugins_dir)?;
    for entry in plugin_subdirs {
        let entry = entry?;
        let path = entry.path();
        debug!("--> extra search path: {:?}", path);
        common::env::add_lib_search_path(&path);
    }
    let libs = get_dependency_libs (&final_exe_path, &final_exe_dir, &excludelist_path, &includelist_path, false)?;

    info!("Bundling dependencies...");
    for lib in libs {
        info!("  Bundling {}", lib.to_str().unwrap());
        std::fs::copy(
            lib.clone(),
            final_exe_dir.join(lib.file_name().unwrap())
        )?;
    }

    info!("Bundling additional assets...");
    for (src,dst) in [
        ("distribution/windows/shoop.dllpaths", "shoop.dllpaths"),
        ("distribution/windows/shoop.executable", "shoop.executable"),
        (launcher_path.to_str().expect("Failed to convert launcher path"), "shoopdaloop_launcher.exe"),
    ] {
        let from = src_path.join(src);
        let to = folder.join(dst);
        info!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    // info!("Slimming down folder...");
    // for file in [
    //     "shoop_lib/test_runner.exe"
    // ] {
    //     let path = folder.join(file);
    //     info!("  remove {:?}", path);
    //     std::fs::remove_file(&path)?;
    // }

    info!("Portable folder produced in {}", folder.to_str().unwrap());

    Ok(())
}

pub fn build_portable_folder(
    exe_path : &Path,
    _dev_exe_path : &Path,
    launcher_path : &Path,
    output_dir : &Path,
    _release : bool,
) -> Result<(), anyhow::Error> {
    if std::fs::exists(output_dir)? {
        return Err(anyhow::anyhow!("Output directory {:?} already exists", output_dir));
    }
    if !std::fs::exists(output_dir.parent().unwrap())? {
        return Err(anyhow::anyhow!("Output directory {:?}: parent doesn't exist", output_dir));
    }
    info!("Creating portable directory...");
    std::fs::create_dir(output_dir)?;

    populate_folder(output_dir,
                            exe_path,
                            launcher_path)?;

    info!("Portable folder created @ {output_dir:?}");
    Ok(())
}

