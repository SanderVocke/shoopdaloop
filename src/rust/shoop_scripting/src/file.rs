use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use anyhow::anyhow;
use omnilua::{Function, Lua, Table};

use crate::api_version::ApiVersionState;
use crate::{install_compatibility_value, runtime_error};

#[derive(Default)]
pub struct ScriptFileReader {
    script_directory: RefCell<Option<PathBuf>>,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "shoop-script-files-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(path.join("content")).unwrap();
        std::fs::write(path.join("script.lua"), "").unwrap();
        std::fs::write(path.join("content/data.bin"), [0, 255, 42]).unwrap();
        path
    }

    #[shoop_wasm_test_support::shoop_test]
    fn reads_binary_files_below_the_script_directory() {
        let root = fixture();
        let reader = ScriptFileReader::default();
        reader.set_script_path(root.join("script.lua").to_str().unwrap());

        assert_eq!(reader.read("content/data.bin").unwrap(), [0, 255, 42]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rejects_paths_that_are_not_strictly_deeper() {
        let root = fixture();
        let reader = ScriptFileReader::default();
        reader.set_script_path(root.join("script.lua").to_str().unwrap());

        for path in [
            "",
            ".",
            "content/../script.lua",
            "../outside",
            "/etc/passwd",
        ] {
            assert!(reader.read(path).is_err(), "accepted {path:?}");
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[shoop_wasm_test_support::shoop_test]
    fn rejects_symlinks_that_escape_the_script_directory() {
        let root = fixture();
        std::os::unix::fs::symlink("/etc/passwd", root.join("content/outside")).unwrap();
        let reader = ScriptFileReader::default();
        reader.set_script_path(root.join("script.lua").to_str().unwrap());

        assert!(reader.read("content/outside").is_err());

        std::fs::remove_dir_all(root).unwrap();
    }
}

impl ScriptFileReader {
    pub fn set_script_path(&self, script_path: &str) {
        let directory = Path::new(script_path)
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_owned);
        *self.script_directory.borrow_mut() = directory;
    }

    fn read(&self, relative_path: &str) -> omnilua::Result<Vec<u8>> {
        let relative_path = Path::new(relative_path);
        if relative_path.as_os_str().is_empty()
            || relative_path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(runtime_error(
                "script file path must name a deeper relative location",
            ));
        }
        let script_directory = self.script_directory.borrow();
        let script_directory = script_directory.as_ref().ok_or_else(|| {
            runtime_error("script file loading requires a script with a filesystem location")
        })?;
        let root = script_directory.canonicalize().map_err(|error| {
            runtime_error(format!("could not resolve script directory: {error}"))
        })?;
        let path = root.join(relative_path);
        let resolved = path
            .canonicalize()
            .map_err(|error| runtime_error(format!("could not resolve script file: {error}")))?;
        if !resolved.starts_with(&root) || resolved == root {
            return Err(runtime_error(
                "script file path must resolve below the script directory",
            ));
        }
        std::fs::read(&resolved)
            .map_err(|error| runtime_error(format!("could not read script file: {error}")))
    }

    pub fn read_utf8(&self, relative_path: &str) -> omnilua::Result<String> {
        String::from_utf8(self.read(relative_path)?)
            .map_err(|error| runtime_error(format!("script file is not UTF-8: {error}")))
    }
}

pub fn install_file_api(
    lua: &Lua,
    run_sandboxed: &Function,
    versions: Rc<ApiVersionState>,
    files: Rc<ScriptFileReader>,
) -> anyhow::Result<()> {
    let module = (|| -> omnilua::Result<Table> {
        let module = lua.create_table()?;
        module.set(
            "load",
            lua.create_function(move |lua, path: String| {
                versions.require_announced()?;
                lua.create_string(files.read(&path)?)
            })?,
        )?;
        Ok(module)
    })()
    .map_err(|error| anyhow!("could not install shoop_file API: {error}"))?;
    install_compatibility_value(run_sandboxed, "__shoop_file", module)
}
