use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};
use crate::dependencies::get_dependency_libs;
use crate::fs_helpers::recursive_dir_cpy;

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
    info!("Using source path {src_path:?}");

    info!("Bundling executable...");
    std::fs::copy(exe_path, &final_exe_path)
        .with_context(|| format!("Failed to copy {exe_path:?} to {folder:?}"))?;

    // info!("Bundling runtime dependencies...");
    // let runtime_dir = folder.join("runtime");
    // std::fs::create_dir(&runtime_dir)
    //     .with_context(|| format!("Cannot create dir: {:?}", runtime_dir))?;
    // recursive_dir_cpy(
    //     &PathBuf::from(runtime_env_dir),
    //     &runtime_dir
    // )?;

    // let dynlib_dir = folder;
    // let excludelist_path = src_path.join("distribution/windows/excludelist");
    // let includelist_path = src_path.join("distribution/windows/includelist");
    // let final_exe_dir = final_exe_path.parent().ok_or(anyhow::anyhow!("Could not get executable directory"))?;
    // info!("Getting dependencies (this may take some time)...");
    // let libs = get_dependency_libs (&final_exe_path, &final_exe_dir, &excludelist_path, &includelist_path, false)?;

    // info!("Bundling dependencies...");
    // for lib in libs {
    //     info!("  Bundling {}", lib.to_str().unwrap());
    //     std::fs::copy(
    //         lib.clone(),
    //         dynlib_dir.join(lib.file_name().unwrap())
    //     )?;
    // }

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

