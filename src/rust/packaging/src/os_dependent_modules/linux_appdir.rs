use anyhow;
use anyhow::Context;
use std::path::{PathBuf, Path};

use common::logging::macros::*;
shoop_log_unit!("packaging");

const MAYBE_QMAKE : Option<&'static str> = option_env!("QMAKE");

fn populate_appdir(
    appdir : &Path,
    exe_path : &Path
) -> Result<(), anyhow::Error> {

    let file_path = PathBuf::from(file!());
    let src_path = std::fs::canonicalize(file_path)?;
    let src_path = src_path.ancestors().nth(6).ok_or(anyhow::anyhow!("cannot find src dir"))?;
    info!("Using source path {src_path:?}");

    let excludelist_path = src_path.join("distribution/linux/excludelist");
    let includelist_path = src_path.join("distribution/linux/includelist");

    crate::portable_folder_common::populate_portable_folder (appdir,
                                                             exe_path,
                                                             src_path,
                                                             &includelist_path,
                                                             &excludelist_path)?;

    info!("Bundling distribution assets...");
    for file in [
        "distribution/linux/shoopdaloop.desktop",
        "distribution/linux/shoopdaloop.png",
        "distribution/linux/AppRun",
        "distribution/linux/shoop-config.toml",
        "distribution/linux/shoopdaloop"
    ] {
        let from = src_path.join(file);
        let to = appdir.join(from.file_name().unwrap());
        info!("  {:?} -> {:?}", &from, &to);
        std::fs::copy(&from, &to)
            .with_context(|| format!("Failed to copy {:?} to {:?}", from, to))?;
    }

    // Explicitly bundle libraries not detected automatically
    let mut extra_assets : Vec<(String, String)> = vec![];
    for base in &["meshoptimizer*.so"] {
        for path in backend::runtime_link_dirs() {
            let pattern = (&path).join(base);
            println!("{pattern:?}");
            let g = glob(&pattern.to_string_lossy())?.filter_map(Result::ok);
            for extra_lib_path in g {
                let extra_lib_srcpath : String = extra_lib_path.to_string_lossy().to_string();
                let extra_lib_filename = &extra_lib_path.file_name().unwrap().to_string_lossy().to_string();
                let extra_lib_dstpath : String = format!("lib/{}", extra_lib_filename);
                extra_assets.push((extra_lib_srcpath, extra_lib_dstpath));
            }
        }
    }
    info!("Bundling additional assets...");
    for (src,dst) in extra_assets {
        let from = src_path.join(src);
        let to = folder.join(dst);
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
    let output_dir = std::path::absolute(output_dir)?;
    if output_dir.exists() {
        return Err(anyhow::anyhow!("Output directory {:?} already exists", output_dir));
    }
    if !output_dir.parent()
       .ok_or(anyhow::anyhow!("Cannot find parent of {output_dir:?}"))?
       .exists() {
        return Err(anyhow::anyhow!("Output directory {:?}: parent doesn't exist", output_dir));
    }
    info!("Creating app directory...");
    std::fs::create_dir(&output_dir)?;

    populate_appdir(&output_dir,
                    exe_path)?;

    info!("AppDir created @ {output_dir:?}");
    Ok(())
}

