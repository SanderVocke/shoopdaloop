use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::anyhow;
use omnilua::{Function, Lua, Table};
use shoop_script_resources::{
    register_resource_provider, NormalizedRelativePath, RegisteredResourceProvider, ResourceOrigin,
    ScriptResourceBundle,
};

use crate::api_version::ApiVersionState;
use crate::{install_compatibility_value, runtime_error};

#[derive(Clone, Debug, Default)]
pub enum ScriptResourceProvider {
    #[default]
    None,
    Filesystem(PathBuf),
    Bundle(Arc<ScriptResourceBundle>),
}

#[derive(Default)]
struct ReaderState {
    provider: ScriptResourceProvider,
    origin: Option<ResourceOrigin>,
}

#[derive(Default)]
pub struct ScriptFileReader {
    state: RefCell<ReaderState>,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use shoop_script_resources::{ResourceKind, ResourceLimits, ScriptResource};
    use std::collections::BTreeMap;
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

    #[shoop_wasm_test_support::shoop_test]
    fn bundle_and_filesystem_providers_have_matching_reads() {
        let root = fixture();
        let filesystem = ScriptFileReader::default();
        filesystem.set_script_path(root.join("script.lua").to_str().unwrap());
        let path = NormalizedRelativePath::parse("content/data.bin").unwrap();
        let bundle = ScriptResourceBundle::new(
            NormalizedRelativePath::parse("script.lua").unwrap(),
            BTreeMap::from([
                (
                    NormalizedRelativePath::parse("script.lua").unwrap(),
                    ScriptResource::new(ResourceKind::Lua, Arc::<[u8]>::from(&b""[..])),
                ),
                (
                    path,
                    ScriptResource::new(
                        ResourceKind::Image,
                        Arc::<[u8]>::from(&[0_u8, 255, 42][..]),
                    ),
                ),
            ]),
            ResourceLimits::default(),
        );
        assert!(bundle.is_err());

        std::fs::write(root.join("content/data.png"), [0, 255, 42]).unwrap();
        let bundle = Arc::new(
            ScriptResourceBundle::new(
                NormalizedRelativePath::parse("script.lua").unwrap(),
                BTreeMap::from([
                    (
                        NormalizedRelativePath::parse("script.lua").unwrap(),
                        ScriptResource::new(ResourceKind::Lua, Arc::<[u8]>::from(&b""[..])),
                    ),
                    (
                        NormalizedRelativePath::parse("content/data.png").unwrap(),
                        ScriptResource::new(
                            ResourceKind::Image,
                            Arc::<[u8]>::from(&[0_u8, 255, 42][..]),
                        ),
                    ),
                ]),
                ResourceLimits::default(),
            )
            .unwrap(),
        );
        let memory = ScriptFileReader::default();
        memory
            .configure(ScriptResourceProvider::Bundle(bundle), None)
            .unwrap();
        assert_eq!(
            filesystem.read("content/data.png").unwrap(),
            memory.read("content/data.png").unwrap()
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}

impl ScriptFileReader {
    #[cfg(test)]
    pub fn set_script_path(&self, script_path: &str) {
        let provider = std::path::Path::new(script_path)
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| ScriptResourceProvider::Filesystem(path.to_owned()))
            .unwrap_or_default();
        let _ = self.configure(provider, None);
    }

    pub fn configure(
        &self,
        provider: ScriptResourceProvider,
        origin: Option<ResourceOrigin>,
    ) -> Result<(), String> {
        let provider = match provider {
            ScriptResourceProvider::Filesystem(root) => {
                let root = root
                    .canonicalize()
                    .map_err(|error| format!("could not resolve script directory: {error}"))?;
                ScriptResourceProvider::Filesystem(root)
            }
            provider => provider,
        };
        if let Some(origin) = &origin {
            let registered = match &provider {
                ScriptResourceProvider::Filesystem(root) => {
                    RegisteredResourceProvider::Filesystem(root.clone())
                }
                ScriptResourceProvider::Bundle(bundle) => {
                    RegisteredResourceProvider::Bundle(Arc::clone(bundle))
                }
                ScriptResourceProvider::None => {
                    return Err("cannot register an empty script resource provider".to_owned());
                }
            };
            register_resource_provider(origin, registered)?;
        }
        *self.state.borrow_mut() = ReaderState { provider, origin };
        Ok(())
    }

    fn read(&self, relative_path: &str) -> omnilua::Result<Vec<u8>> {
        let relative_path = NormalizedRelativePath::parse(relative_path)
            .map_err(|_| runtime_error("script file path must name a deeper relative location"))?;
        match &self.state.borrow().provider {
            ScriptResourceProvider::None => Err(runtime_error(
                "script file loading requires an attached resource provider",
            )),
            ScriptResourceProvider::Filesystem(root) => {
                let resolved =
                    root.join(relative_path.as_str())
                        .canonicalize()
                        .map_err(|error| {
                            runtime_error(format!("could not resolve script file: {error}"))
                        })?;
                if !resolved.starts_with(root) || !resolved.is_file() {
                    return Err(runtime_error(
                        "script file path must resolve below the script directory",
                    ));
                }
                std::fs::read(&resolved)
                    .map_err(|error| runtime_error(format!("could not read script file: {error}")))
            }
            ScriptResourceProvider::Bundle(bundle) => bundle
                .get(&relative_path)
                .map(|resource| resource.bytes.to_vec())
                .ok_or_else(|| {
                    runtime_error(format!("undeclared script resource {relative_path:?}"))
                }),
        }
    }

    pub fn read_utf8(&self, relative_path: &str) -> omnilua::Result<String> {
        String::from_utf8(self.read(relative_path)?)
            .map_err(|error| runtime_error(format!("script file is not UTF-8: {error}")))
    }

    pub fn base_uri(&self, relative_path: Option<&str>) -> omnilua::Result<Option<String>> {
        let state = self.state.borrow();
        let Some(origin) = &state.origin else {
            return Ok(None);
        };
        match relative_path {
            Some(path) => {
                let path = NormalizedRelativePath::parse(path).map_err(|_| {
                    runtime_error("script file path must name a deeper relative location")
                })?;
                Ok(Some(origin.base_uri_below(&path)))
            }
            None => Ok(Some(origin.base_uri())),
        }
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
