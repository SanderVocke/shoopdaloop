use std::path::{Path, PathBuf};
use std::fs;
use walkdir::{WalkDir, Error};
use anyhow::{Result, Context};

pub fn copy_dir_merge(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), anyhow::Error> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    for entry in WalkDir::new(src) {
        let entry = entry?;
        let rel_path = entry.path().strip_prefix(src)
           .with_context(|| format!("Failed to strip prefix from path"))?;
        let dest_path = dst.join(rel_path);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest_path)
               .with_context(|| format!("Failed to create directory for {}", dest_path.display()))?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                  .with_context(|| format!("Failed to create directory for {}", parent.display()))?;
            }
            fs::copy(entry.path(), &dest_path)
               .with_context(|| format!("Failed to copy file for {}", dest_path.display()))?;
        }
    }
    Ok(())
}