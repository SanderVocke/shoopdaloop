use anyhow;
use anyhow::Context;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use crate::fs_helpers::recursive_dir_cpy;

pub fn build_appimage(
    appimagetool : &str,
    appdir : &Path,
    output_file : &Path,
    strip: bool,
) -> Result<(), anyhow::Error> {
    let mut src_dir = appdir.to_owned();


    println!("Creating temporary directory for stripping...");
    let tmp_dir : Option<TempDir> = match strip {
        true => Some(TempDir::new()?),
        false => None,
    };
    if strip {
        src_dir = tmp_dir.unwrap().path().to_owned().join("appdir");

        std::fs::create_dir_all(&src_dir)
           .with_context(|| format!("Failed to create {src_dir:?}"))?;
        recursive_dir_cpy(appdir, &src_dir)
           .with_context(|| format!("Failed to copy {appdir:?} to {src_dir:?}"))?;

        for file in [
            "nextest-archive.tar.zst",
            "cargo-nextest",
            "shoop_lib/test_runner",
        ] {
            let file = src_dir.join(file);
            std::fs::remove_file(&file)
                .with_context(|| format!("Failed to remove {file:?}"))?;
        }
    }

    println!("Creating AppImage...");
    Command::new(appimagetool)
        .args(&[src_dir.to_str().unwrap(), output_file.to_str().unwrap()])
        .status()
        .with_context(|| "Failed to run appimagetool")?;

    println!("AppImage created @ {output_file:?}");
    Ok(())
}