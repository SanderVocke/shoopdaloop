use anyhow;
use anyhow::Context;
use std::path::Path;

pub fn recursive_dir_cpy (src: &Path, dst: &Path) -> Result<(), anyhow::Error> {
    for entry in std::fs::read_dir(src)
                    .with_context(|| format!("Cannot read dir {src:?}"))? {
        let entry = entry.with_context(|| format!("Invalid entry"))?;
        let path = entry.path();
        let file_name = path.file_name().unwrap();
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