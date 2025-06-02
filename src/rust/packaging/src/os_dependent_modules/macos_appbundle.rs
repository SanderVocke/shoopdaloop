use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;
use regex::Regex;
use std::collections::HashSet;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_appbundle(
    appdir : &Path,
    exe_path : &Path,
) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(6).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    crate::portable_folder_common::populate_portable_folder (appdir,
                                                             exe_path,
                                                             src_path)?;

    info!("Creating directories...");
    for directory in [
        "Contents",
        "Contents/MacOS",
        "Contents/Resources",
    ] {
        std::fs::create_dir(appdir.join(directory))
            .with_context(|| format!("Failed to create {directory:?}"))?;
    }

    // info!("Bundling runtime dependencies...");
    // let runtime_dir = appdir.join("runtime");
    // std::fs::create_dir(&runtime_dir)
    //     .with_context(|| format!("Cannot create dir: {:?}", runtime_dir))?;
    // recursive_dir_cpy(
    //     &PathBuf::from(runtime_env_dir),
    //     &runtime_dir
    // )?;

    // info!("Getting dependencies (this may take some time)...");
    // let dynlib_dir = appdir.join("lib");
    // let excludelist_path = src_path.join("distribution/macos/excludelist");
    // let includelist_path = src_path.join("distribution/macos/includelist");
    // let final_exe_dir = final_exe_path.parent().ok_or(anyhow::anyhow!("Could not get executable directory"))?;
    // let libs = get_dependency_libs (&final_exe_path, &final_exe_dir, &excludelist_path, &includelist_path, true)?;

    // info!("Bundling dependencies...");
    // std::fs::create_dir(&dynlib_dir)?;
    // let re = Regex::new(r"(.*/.*.framework)/.*").unwrap();
    // let mut set: HashSet<PathBuf> = HashSet::new();
    // for lib in libs {
    //     // Detect libraries in framework folders and reduce the entries to the frameworks themselves
    //     let cap = re.captures(lib.to_str().unwrap());
    //     if !cap.is_none() {
    //         let p = PathBuf::from(cap.unwrap().get(1).unwrap().as_str());
    //         set.insert(p);
    //     } else {
    //         set.insert(lib.clone());
    //     }
    // }
    // let libs: Vec<_> = set.into_iter().collect(); // Convert back to Vec
    // for lib in libs {
    //     let from = lib.clone();
    //     let to = dynlib_dir.clone().join(lib.file_name().unwrap());

    //     if !from.exists() {
    //         info!("  Skipping nonexistent file/framework {from:?}");
    //     } else if std::fs::metadata(&from)?.is_dir() {
    //         info!("  Bundling {}", lib.to_str().unwrap());
    //         recursive_dir_cpy(&from, &to)
    //            .with_context(|| format!("Failed to copy dir {from:?} to {to:?}"))?;
    //     } else {
    //         info!("  Bundling {}", lib.to_str().unwrap());
    //         std::fs::copy(&from, &to)
    //            .with_context(|| format!("Failed to copy {from:?} to {to:?}"))?;
    //     }
    // }

    info!("Bundling additional assets...");
    for (src,dst) in [
        ("distribution/macos/Info.plist", "Contents/Info.plist"),
        ("distribution/macos/icon.icns", "Contents/Resources/icon.icns"),
        ("distribution/macos/shoopdaloop", "Contents/MacOS/shoopdaloop"),
    ] {
        let from = src_path.join(src);
        let to = appdir.join(dst);
        info!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    info!("App bundle produced in {}", appdir.to_str().unwrap());

    Ok(())
}

pub fn build_appbundle(
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
    info!("Creating app bundle directory...");
    std::fs::create_dir(output_dir)?;

    populate_appbundle(output_dir, exe_path)?;

    info!("App bundle created @ {output_dir:?}");
    Ok(())
}

