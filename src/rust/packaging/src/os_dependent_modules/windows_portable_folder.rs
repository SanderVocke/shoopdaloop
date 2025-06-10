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
    info!("Using source path {src_path:?}");

    let excludelist_path = src_path.join("distribution/windows/excludelist");
    let includelist_path = src_path.join("distribution/windows/includelist");

    crate::portable_folder_common::populate_portable_folder
       (folder, exe_path, src_path, &includelist_path, &excludelist_path)?;

    info!("Bundling additional assets...");
    for (src,dst) in [
        (launcher_path.to_str().expect("Failed to convert launcher path"), "shoopdaloop_launcher.exe"),
    ] {
        let from = src_path.join(src);
        let to = folder.join(dst);
        info!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
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

