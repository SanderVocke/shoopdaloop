use anyhow::anyhow;
use anyhow::Context;
use std::path::{Path, PathBuf};

/// Locate the repository root from this source file's compile-time path.
///
/// The OS-specific packaging modules each do this by hand with
/// `PathBuf::from(file!())` followed by `.ancestors().nth(6)`, which silently
/// breaks if a file moves to a different directory depth. Searching upwards for
/// a directory that has both `Cargo.toml` and `distribution/` is robust against
/// that.
pub fn source_root() -> Result<PathBuf, anyhow::Error> {
    let this_file = std::fs::canonicalize(PathBuf::from(file!()))
        .with_context(|| format!("Cannot canonicalize {:?}", file!()))?;
    this_file
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file() && dir.join("distribution").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow!(
                "Cannot find the source root above {:?} (looking for a dir with \
                 both Cargo.toml and distribution/)",
                this_file
            )
        })
}

#[tracing::instrument(name = "tool.packaging.copy_directory", skip_all)]
pub fn recursive_dir_cpy(src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    for entry in std::fs::read_dir(src).with_context(|| format!("Cannot read dir {src:?}"))? {
        let entry = entry.with_context(|| format!("Invalid entry"))?;
        let path = entry.path();
        let file_name = path
            .file_name()
            .ok_or(anyhow!("Invalid file name: {:?}", path))?;
        if path.is_dir() {
            std::fs::create_dir(dst.join(file_name))
                .with_context(|| format!("Cannot create {:?}", dst.join(file_name)))?;
            recursive_dir_cpy(&path, &dst.join(file_name))?;
        } else {
            std::fs::copy(&path, &dst.join(file_name))
                .with_context(|| format!("Cannot copy {:?} to {:?}", path, dst.join(file_name)))?;
        }
    }
    Ok(())
}
