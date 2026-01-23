use anyhow::anyhow;
use anyhow::Context;
use std::path::Path;
use std::process::Command;

use common::logging::macros::*;
shoop_log_unit!("packaging");

pub fn build_appimage(
    appimagetool: &str,
    appdir: &Path,
    output_file: &Path,
) -> Result<(), anyhow::Error> {
    let src_dir = appdir.to_owned();

    info!("Creating AppImage...");
    Command::new(appimagetool)
        .args(&[
            src_dir.to_str().ok_or(anyhow!("Invalid unicode"))?,
            output_file.to_str().ok_or(anyhow!("Invalid unicode"))?,
        ])
        .status()
        .with_context(|| "Failed to run appimagetool")?;

    info!("AppImage created @ {output_file:?}");
    Ok(())
}
