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

    println!("Creating AppImage...");
    Command::new(appimagetool)
        .args(&[src_dir.to_str().unwrap(), output_file.to_str().unwrap()])
        .status()
        .with_context(|| "Failed to run appimagetool")?;

    println!("AppImage created @ {output_file:?}");
    Ok(())
}