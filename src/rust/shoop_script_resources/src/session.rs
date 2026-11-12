use alloc::{collections::BTreeSet, string::String, sync::Arc, vec::Vec};

use crate::{validate_resources, ResourceLimits, ScriptResourceBundle};

pub const MAX_SESSION_SCRIPTS: usize = 256;
pub const MAX_SESSION_RESOURCE_FILES: usize = 65_536;
pub const MAX_SCRIPT_METADATA_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SCRIPT_NAME_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionScriptData {
    pub document_id: u64,
    pub name: String,
    pub enabled: bool,
    pub bundle: Arc<ScriptResourceBundle>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionScripts(pub Vec<SessionScriptData>);

impl SessionScripts {
    pub fn validate(&self) -> Result<(), String> {
        if self.0.len() > MAX_SESSION_SCRIPTS {
            return Err("too many session scripts".into());
        }
        let limits = ResourceLimits::default();
        let mut ids = BTreeSet::new();
        let mut files = 0usize;
        let mut bytes = 0u64;
        let mut metadata = 0usize;
        for script in &self.0 {
            if script.document_id == 0 || !ids.insert(script.document_id) {
                return Err("invalid or duplicate session script identity".into());
            }
            if script.name.trim().is_empty() || script.name.len() > MAX_SCRIPT_NAME_BYTES {
                return Err("invalid session script name".into());
            }
            validate_resources(&script.bundle.entrypoint, &script.bundle.resources, limits)
                .map_err(|error| alloc::format!("{error}"))?;
            metadata = metadata
                .saturating_add(script.name.len() + script.bundle.entrypoint.as_str().len() + 64);
            files = files.saturating_add(script.bundle.resources.len());
            bytes = bytes.saturating_add(script.bundle.byte_count());
            for path in script.bundle.resources.keys() {
                if path.as_str().len() > MAX_SCRIPT_NAME_BYTES {
                    return Err("script resource path is too long".into());
                }
                metadata = metadata.saturating_add(path.as_str().len() + 64);
            }
        }
        if files > MAX_SESSION_RESOURCE_FILES
            || bytes > limits.max_aggregate_bytes
            || metadata > MAX_SCRIPT_METADATA_BYTES
        {
            return Err("session script resource or metadata budget exceeded".into());
        }
        Ok(())
    }
}
