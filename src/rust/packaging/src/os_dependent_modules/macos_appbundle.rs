use crate::fs_helpers::recursive_dir_cpy;
use anyhow;
use anyhow::Context;
use glob::glob;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::logging::macros::*;
shoop_log_unit!("packaging");

fn populate_appbundle(appdir: &Path, exe_path: &Path) -> Result<(), anyhow::Error> {
    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path
        .ancestors()
        .nth(6)
        .ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    let excludelist_path = src_path.join("distribution/macos/excludelist");
    let includelist_path = src_path.join("distribution/macos/includelist");

    crate::portable_folder_common::populate_portable_folder(
        appdir,
        exe_path,
        src_path,
        &includelist_path,
        &excludelist_path,
    )?;

    info!("Adding rpaths...");
    let installed_exe = appdir.join("shoopdaloop_exe");
    Command::new("install_name_tool")
        .args(&[
            "-add_rpath",
            "@executable_path/lib",
            installed_exe.to_str().unwrap(),
        ])
        .status()?;

    info!("Creating directories...");
    for directory in ["Contents", "Contents/MacOS", "Contents/Resources"] {
        std::fs::create_dir(appdir.join(directory))
            .with_context(|| format!("Failed to create {directory:?}"))?;
    }

    info!("Bundling runtime dependencies...");
    let runtime_dir = appdir.join("runtime");
    std::fs::create_dir(&runtime_dir)
        .with_context(|| format!("Cannot create dir: {:?}", runtime_dir))?;
    recursive_dir_cpy(&PathBuf::from(&runtime_dir), &runtime_dir)?;

    let mut extra_assets: Vec<(String, String)> = vec![
        (
            "distribution/macos/Info.plist".to_string(),
            "Contents/Info.plist".to_string(),
        ),
        (
            "distribution/macos/icon.icns".to_string(),
            "Contents/Resources/icon.icns".to_string(),
        ),
        (
            "distribution/macos/shoopdaloop".to_string(),
            "Contents/MacOS/shoopdaloop".to_string(),
        ),
        (
            "distribution/macos/shoop-config.toml".to_string(),
            "shoop-config.toml".to_string(),
        ),
    ];

    // Explicitly bundle shiboken library
    for base in &["*shiboken6*dylib", "*pyside6*dylib", "*pyside6qml*dylib"] {
        for path in backend::runtime_link_dirs() {
            let pattern = &path.join(base);
            let g = glob(&pattern.to_string_lossy())?.filter_map(Result::ok);
            for extra_lib_path in g {
                let extra_lib_srcpath: String = extra_lib_path.to_string_lossy().to_string();
                let extra_lib_filename = &extra_lib_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let extra_lib_dstpath: String = format!("lib/{}", extra_lib_filename);
                extra_assets.push((extra_lib_srcpath, extra_lib_dstpath));
            }
        }
    }

    info!("Bundling additional assets...");
    for (src, dst) in extra_assets {
        let from = src_path.join(src);
        let to = appdir.join(dst);
        info!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    info!("App bundle produced in {}", appdir.to_str().unwrap());

    Ok(())
}

pub fn build_appbundle(exe_path: &Path, output_dir: &Path) -> Result<(), anyhow::Error> {
    if output_dir.exists() {
        return Err(anyhow::anyhow!(
            "Output directory {:?} already exists",
            output_dir
        ));
    }
    if !output_dir
        .parent()
        .ok_or(anyhow::anyhow!("Cannot find parent of {output_dir:?}"))?
        .exists()
    {
        return Err(anyhow::anyhow!(
            "Output directory {:?}: parent doesn't exist",
            output_dir
        ));
    }
    info!("Creating app bundle directory...");
    std::fs::create_dir(output_dir)?;

    populate_appbundle(output_dir, exe_path)?;

    info!("App bundle created @ {output_dir:?}");
    Ok(())
}
