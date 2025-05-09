use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use glob::glob;
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;
use crate::deduplicate_libraries::deduplicate_libraries;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_appdir(
    runtime_env_dir : &Path,
    appdir : &Path,
    exe_path : &Path,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(6).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    info!("Bundling executable...");
    let final_exe_path = appdir.join("shoopdaloop_bin");
    std::fs::copy(exe_path, &final_exe_path)?;

    info!("Bundling runtime dependencies...");
    let runtime_dir = appdir.join("runtime");
    std::fs::create_dir(&runtime_dir)
        .with_context(|| format!("Cannot create dir: {:?}", runtime_dir))?;
    recursive_dir_cpy(
        &PathBuf::from(runtime_env_dir),
        &runtime_dir
    )?;

    // info!("Getting dependencies (this may take some time)...");
    // let dynlib_dir = appdir.join("lib");
    // let excludelist_path = src_path.join("distribution/linux/excludelist");
    // let includelist_path = src_path.join("distribution/linux/includelist");
    // let exe_dir = final_exe_path.parent().ok_or(anyhow::anyhow!("Could not get executable directory"))?;
    // let libs = get_dependency_libs (&final_exe_path, &exe_dir, &excludelist_path, &includelist_path, false)?;

    // info!("Bundling {} dependencies...", libs.len());
    // std::fs::create_dir(&dynlib_dir)?;
    // for lib in libs {
    //     info!("  Bundling {}", lib.to_str().unwrap());
    //     std::fs::copy(
    //         lib.clone(),
    //         dynlib_dir.clone().join(lib.file_name().unwrap())
    //     )?;
    // }

    info!("Bundling additional assets...");
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

    // info!("Slimming down AppDir...");
    // for pattern in [
    //     "shoop_lib/test_runner",
    //     "shoop_lib/py/lib/python*/test",
    //     "shoop_lib/py/lib/python*/*/test",
    //     "shoop_lib/py/lib/python*/site-packages/PySide6/**/*Qt6Sql*",
    //     "shoop_lib/py/lib/python*/site-packages/PySide6/Qt/plugins/sqldrivers",
    //     "shoop_lib/py/lib/python*/site-packages/PySide6/Qt/plugins/designer",
    //     "shoop_lib/py/lib/python*/site-packages/PySide6/Qt/plugins/qmltooling",
    //     "shoop_lib/py/lib/python*/site-packages/PySide6/Qt/qml/QtQuick/VirtualKeyboard",
    //     "shoop_lib/py/lib/python*/site-packages/PySide6/Qt/qml/QtQuick/Scene2D",
    //     "shoop_lib/py/lib/python*/site-packages/PySide6/Qt/qml/Qt5Compat/GraphicalEffects",
    //     "**/*.a",
    // ] {
    //     let pattern = format!("{}/{pattern}", appdir.to_str().unwrap());
    //     for f in glob(&pattern)? {
    //         let f = f?;
    //         info!("  remove {f:?}");
    //         match std::fs::metadata(&f)?.is_dir() {
    //             true => { std::fs::remove_dir_all(&f)?; }
    //             false => { std::fs::remove_file(&f)?; }
    //         }
    //     }
    // }

    // Deduplicate identical libraries as symlinked files
    deduplicate_libraries(&appdir)?;

    info!("AppDir produced in {}", appdir.to_str().unwrap());

    Ok(())
}

pub fn build_appdir(
    runtime_env_dir : &Path,
    exe_path : &Path,
    _dev_exe_path : &Path,
    output_dir : &Path,
    _release : bool,
) -> Result<(), anyhow::Error> {
    info!("Runtime env directory: {:?}", runtime_env_dir);

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

    populate_appdir(runtime_env_dir,
                    output_dir,
                    exe_path)?;

    info!("AppDir created @ {output_dir:?}");
    Ok(())
}

