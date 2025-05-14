use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use glob::glob;
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;
use common::fs::copy_dir_merge;
use regex::Regex;
use copy_dir::copy_dir;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_appdir(
    appdir : &Path,
    exe_path : &Path,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(6).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    let bin_dir = appdir.join("bin");
    std::fs::create_dir(&bin_dir)
        .with_context(|| format!("Cannot create dir: {:?}", bin_dir))?;

    let lib_dir = appdir.join("lib");
    std::fs::create_dir(&lib_dir)
        .with_context(|| format!("Cannot create dir: {:?}", lib_dir))?;

    let py_lib_dir = lib_dir.join("python");
    std::fs::create_dir(&py_lib_dir)
        .with_context(|| format!("Cannot create dir: {:?}", py_lib_dir))?;

    info!("Bundling executable...");
    let final_exe_path = bin_dir.join("shoopdaloop");
    std::fs::copy(exe_path, &final_exe_path)?;

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
    let to_copy = ["src/lua", "src/qml", "src/session_schemas", "resources"];
    info!("Bundling source assets...");
    for directory in to_copy {
        let src = src_path.join(directory);
        let dst = appdir.join(PathBuf::from(directory).file_name().unwrap());
        debug!("--> {:?} -> {:?}", src, dst);
        copy_dir(&src, &dst)
            .expect(&format!("Failed to copy {} to {}",
                            src.display(),
                            dst.display()));
    }

    info!("Getting dependencies (this may take some time)...");
    let dynlib_dir = appdir.join("lib");
    let excludelist_path = src_path.join("distribution/linux/excludelist");
    let includelist_path = src_path.join("distribution/linux/includelist");
    // let extra_search_path = vcpkg_installed_dir.join
    //                          (if cfg!(debug_assertions) { "debug/lib" } else { "lib" });
    for path in backend::all_link_search_paths() {
        debug!("--> extra search path: {:?}", path);
        common::env::add_lib_search_path(&path);
    }
    let libs = get_dependency_libs (&final_exe_path, &bin_dir, &excludelist_path, &includelist_path, false)?;

    info!("Bundling {} dependencies...", libs.len());
    for lib in libs {
        debug!("--> {}", lib.to_str().unwrap());
        std::fs::copy(
            lib.clone(),
            lib_dir.clone().join(lib.file_name().unwrap())
        )?;
    }

    // FIXME: this is ugly to have to do explicitly. But since these are lazy-loaded,
    // we cannot autodetect them.
    info!("Bundling Qt plugins...");
    let pkg_base_dir = backend::zita_link_dir().parent().unwrap();
    let dylib_extension = if cfg!(target_os = "windows") { "dll" }
                          else if cfg!(target_os = "macos") { "dylib" }
                          else if cfg!(target_os = "linux") { "so" }
                          else { "" };
    let pattern = format!("{}/Qt6/plugins/**/*.{}", pkg_base_dir.to_string_lossy(), dylib_extension);
    for entry in glob(&pattern)? {
        match entry {
            Ok(path) => {
                let src = path;
                let dst = lib_dir.join(path.file_name().unwrap());
                debug!("--> {} -> {}", src.to_str().unwrap(), dst.to_str().unwrap());
                std::fs::copy(&src, &dst)?
            }
            Err(e) => (),
        }
    }

    info!("Bundling distribution assets...");
    for file in [
        "distribution/linux/shoopdaloop.desktop",
        "distribution/linux/shoopdaloop.png",
        "distribution/linux/shoopdaloop",
        "distribution/linux/AppRun",
        "distribution/linux/backend_tests",
        "distribution/linux/python_tests",
        "distribution/linux/rust_tests",
        "distribution/linux/env.sh",
    ] {
        let from = src_path.join(file);
        let to = appdir.join(from.file_name().unwrap());
        info!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    info!("AppDir produced in {}", appdir.to_str().unwrap());

    Ok(())
}

pub fn build_appdir(
    exe_path : &Path,
    _dev_exe_path : &Path,
    output_dir : &Path,
    _release : bool,
) -> Result<(), anyhow::Error> {
    if output_dir.exists() {
        return Err(anyhow::anyhow!("Output directory {:?} already exists", output_dir));
    }
    if !output_dir.parent()
       .ok_or(anyhow::anyhow!("Cannot find parent of {output_dir:?}"))?
       .exists() {
        return Err(anyhow::anyhow!("Output directory {:?}: parent doesn't exist", output_dir));
    }
    info!("Creating app directory...");
    std::fs::create_dir(output_dir)?;

    populate_appdir(output_dir,
                    exe_path)?;

    info!("AppDir created @ {output_dir:?}");
    Ok(())
}

