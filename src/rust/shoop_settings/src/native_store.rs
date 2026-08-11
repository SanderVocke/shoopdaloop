use crate::SETTINGS_FILENAME;
use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub fn default_settings_path() -> Result<PathBuf> {
    let project = directories::ProjectDirs::from("org", "ShoopDaLoop", "ShoopDaLoop egui")
        .ok_or_else(|| anyhow!("could not determine application project directories"))?;
    Ok(project.config_dir().join(SETTINGS_FILENAME))
}

pub fn load_settings_file(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .with_context(|| {
            format!(
                "could not read application settings from {}",
                path.display()
            )
        })
        .map(Some)
}

pub fn save_settings_file(path: &Path, contents: &str) -> Result<()> {
    save_settings_file_with(path, contents, |temporary, target| {
        temporary
            .persist(target)
            .map(|file| file)
            .map_err(|error| error.error)
    })
}

fn save_settings_file_with(
    path: &Path,
    contents: &str,
    commit: impl FnOnce(NamedTempFile, &Path) -> std::io::Result<File>,
) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("could not create settings directory {}", parent.display()))?;
    let mut temporary = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "could not create temporary settings file in {}",
            parent.display()
        )
    })?;
    temporary
        .write_all(contents.as_bytes())
        .context("could not write temporary settings file")?;
    temporary
        .as_file()
        .sync_all()
        .context("could not flush temporary settings file")?;
    let committed = commit(temporary, path)
        .with_context(|| format!("could not replace settings at {}", path.display()))?;
    committed
        .sync_all()
        .context("could not flush committed settings file")?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("could not flush settings directory {}", parent.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_first_run_and_save_creates_parents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("settings.json");
        assert_eq!(load_settings_file(&path).unwrap(), None);
        save_settings_file(&path, "first\n").unwrap();
        assert_eq!(
            load_settings_file(&path).unwrap().as_deref(),
            Some("first\n")
        );
    }

    #[test]
    fn save_replaces_complete_prior_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "old content that is longer\n").unwrap();
        save_settings_file(&path, "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_commit_retains_prior_bytes_and_cleans_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "old\n").unwrap();
        let result = save_settings_file_with(&path, "new\n", |_temporary, _target| {
            Err(std::io::Error::other("injected commit failure"))
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn invalid_parent_is_reported_without_replacing_it() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("not-a-directory");
        std::fs::write(&parent, "keep").unwrap();
        assert!(save_settings_file(&parent.join("settings.json"), "new").is_err());
        assert_eq!(std::fs::read_to_string(parent).unwrap(), "keep");
    }

    #[test]
    fn default_path_uses_compatibility_identity() {
        let path = default_settings_path().unwrap();
        assert_eq!(path.file_name().unwrap(), SETTINGS_FILENAME);
        assert!(path.to_string_lossy().to_lowercase().contains("egui"));
    }
}
