use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use tempfile::TempDir;
use std::process::Command;
use std::collections::HashMap;
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_folder(
    shoop_built_out_dir : &Path,
    folder : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    release : bool,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(6).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    let dynlib_dir = folder;
    let excludelist_path = src_path.join("distribution/windows/excludelist");
    let includelist_path = src_path.join("distribution/windows/includelist");
    let env : HashMap<&str, &str> = HashMap::new();
    let libs = get_dependency_libs (&[dev_exe_path], &env, &excludelist_path, &includelist_path, ".dll", false)?;

    info!("Bundling executable...");
    std::fs::copy(exe_path, folder.join("shoopdaloop.exe"))
        .with_context(|| format!("Failed to copy {exe_path:?} to {folder:?}"))?;

    info!("Bundling shoop_lib...");
    let lib_dir = folder.join("shoop_lib");
    std::fs::create_dir(&lib_dir)
        .with_context(|| format!("Cannot create dir: {:?}", lib_dir))?;
    recursive_dir_cpy(
        &PathBuf::from(shoop_built_out_dir).join("shoop_lib"),
        &lib_dir
    )?;

    info!("Bundling dependencies...");
    for lib in libs {
        info!("  Bundling {}", lib.to_str().unwrap());
        std::fs::copy(
            lib.clone(),
            dynlib_dir.join(lib.file_name().unwrap())
        )?;
    }

    // info!("Bundling additional assets...");
    // for file in [
    //     "distribution/linux/shoopdaloop.desktop",
    //     "distribution/linux/shoopdaloop.png",
    //     "distribution/linux/AppRun",
    //     "distribution/linux/backend_tests",
    //     "distribution/linux/python_tests",
    //     "distribution/linux/rust_tests",
    // ] {
    //     let from = src_path.join(file);
    //     let to = folder.join(from.file_name().unwrap());
    //     info!("  {:?} -> {:?}", &from, &to);
    //     std::fs::copy(&from, &to)
    //         .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    // }


    info!("Slimming down folder...");
    for file in [
        "shoop_lib/test_runner.exe"
    ] {
        let path = folder.join(file);
        info!("  remove {:?}", path);
        std::fs::remove_file(&path)?;
    }

    info!("Portable folder produced in {}", folder.to_str().unwrap());

    Ok(())
}

pub fn build_portable_folder(
    shoop_built_out_dir : &Path,
    exe_path : &Path,
    dev_exe_path : &Path,
    output_dir : &Path,
    release : bool,
) -> Result<(), anyhow::Error> {
    info!("Assets directory: {:?}", shoop_built_out_dir);

    if std::fs::exists(output_dir)? {
        return Err(anyhow::anyhow!("Output directory {:?} already exists", output_dir));
    }
    if !std::fs::exists(output_dir.parent().unwrap())? {
        return Err(anyhow::anyhow!("Output directory {:?}: parent doesn't exist", output_dir));
    }
    info!("Creating portable directory...");
    std::fs::create_dir(output_dir)?;

    populate_folder(shoop_built_out_dir,
                            output_dir,
                            exe_path,
                            dev_exe_path,
                            release)?;

    info!("Portable folder created @ {output_dir:?}");
    Ok(())
}

