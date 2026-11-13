//! Bounded client-local resource streaming, independent of musical transports.
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use serde::{Deserialize, Serialize};

use crate::{
    session::*, NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource,
    ScriptResourceBundle,
};

pub const CHUNK_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Part {
    Script {
        id: u64,
        name: String,
        enabled: bool,
        entrypoint: NormalizedRelativePath,
    },
    Resource {
        path: NormalizedRelativePath,
        kind: ResourceKind,
        bytes: u64,
    },
    Chunk {
        offset: u64,
        bytes: Vec<u8>,
    },
    EndResource,
    EndScript,
    Finish,
}

pub struct Source {
    scripts: SessionScripts,
    script: usize,
    paths: Vec<NormalizedRelativePath>,
    resource: usize,
    offset: usize,
    phase: u8,
}
impl Source {
    pub fn new(scripts: SessionScripts) -> Result<Self, String> {
        scripts.validate()?;
        Ok(Self {
            scripts,
            script: 0,
            paths: Vec::new(),
            resource: 0,
            offset: 0,
            phase: 0,
        })
    }
    pub fn next_part(&mut self) -> Option<Part> {
        if self.phase == 5 {
            return None;
        }
        let Some(script) = self.scripts.0.get(self.script) else {
            self.phase = 5;
            return Some(Part::Finish);
        };
        match self.phase {
            0 => {
                self.paths = script.bundle.resources.keys().cloned().collect();
                self.resource = 0;
                self.phase = 1;
                Some(Part::Script {
                    id: script.document_id,
                    name: script.name.clone(),
                    enabled: script.enabled,
                    entrypoint: script.bundle.entrypoint.clone(),
                })
            }
            1 => {
                let Some(path) = self.paths.get(self.resource) else {
                    self.script += 1;
                    self.phase = 0;
                    return Some(Part::EndScript);
                };
                let resource = &script.bundle.resources[path];
                self.offset = 0;
                self.phase = 2;
                Some(Part::Resource {
                    path: path.clone(),
                    kind: resource.kind,
                    bytes: resource.bytes.len() as u64,
                })
            }
            _ => {
                let resource = &script.bundle.resources[&self.paths[self.resource]];
                if self.offset == resource.bytes.len() {
                    self.resource += 1;
                    self.phase = 1;
                    return Some(Part::EndResource);
                }
                let start = self.offset;
                self.offset = resource.bytes.len().min(start + CHUNK_BYTES);
                Some(Part::Chunk {
                    offset: start as u64,
                    bytes: resource.bytes[start..self.offset].to_vec(),
                })
            }
        }
    }
}

struct Script {
    id: u64,
    name: String,
    enabled: bool,
    entrypoint: NormalizedRelativePath,
    resources: BTreeMap<NormalizedRelativePath, ScriptResource>,
    bytes: u64,
}
struct Resource {
    path: NormalizedRelativePath,
    kind: ResourceKind,
    total: u64,
    bytes: Vec<u8>,
}
#[derive(Default)]
pub struct Assembly {
    scripts: SessionScripts,
    script: Option<Script>,
    resource: Option<Resource>,
    bytes: u64,
    files: usize,
    metadata: usize,
    finished: bool,
}
impl Assembly {
    pub fn apply(&mut self, part: Part) -> Result<(), String> {
        if self.finished {
            return Err("script transfer is already complete".into());
        }
        let limits = ResourceLimits::default();
        match part {
            Part::Script {
                id,
                name,
                enabled,
                entrypoint,
            } => {
                if self.script.is_some()
                    || self.scripts.0.len() >= MAX_SESSION_SCRIPTS
                    || id == 0
                    || self.scripts.0.iter().any(|script| script.document_id == id)
                    || name.trim().is_empty()
                    || name.len() > MAX_SCRIPT_NAME_BYTES
                    || entrypoint.as_str().len() > MAX_SCRIPT_NAME_BYTES
                {
                    return Err("invalid script declaration".into());
                }
                self.metadata = self
                    .metadata
                    .saturating_add(name.len() + entrypoint.as_str().len() + 64);
                self.script = Some(Script {
                    id,
                    name,
                    enabled,
                    entrypoint,
                    resources: BTreeMap::new(),
                    bytes: 0,
                });
            }
            Part::Resource { path, kind, bytes } => {
                let script = self.script.as_mut().ok_or("no script declaration")?;
                if self.resource.is_some()
                    || script.resources.contains_key(&path)
                    || script.resources.len() >= limits.max_files_per_script
                    || path.as_str().len() > MAX_SCRIPT_NAME_BYTES
                    || bytes > limits.max_file_bytes
                    || crate::classify_resource(&path) != Some(kind)
                {
                    return Err("invalid resource declaration".into());
                }
                self.files = self.files.saturating_add(1);
                self.bytes = self.bytes.saturating_add(bytes);
                script.bytes = script.bytes.saturating_add(bytes);
                self.metadata = self.metadata.saturating_add(path.as_str().len() + 64);
                if self.files > MAX_SESSION_RESOURCE_FILES
                    || self.bytes > limits.max_aggregate_bytes
                    || script.bytes > limits.max_script_bytes
                    || self.metadata > MAX_SCRIPT_METADATA_BYTES
                {
                    return Err("script transfer budget exceeded".into());
                }
                let mut data = Vec::new();
                data.try_reserve_exact(bytes as usize)
                    .map_err(|_| String::from("resource allocation failed"))?;
                self.resource = Some(Resource {
                    path,
                    kind,
                    total: bytes,
                    bytes: data,
                });
            }
            Part::Chunk { offset, bytes } => {
                let resource = self.resource.as_mut().ok_or("no resource declaration")?;
                if bytes.len() > CHUNK_BYTES
                    || offset != resource.bytes.len() as u64
                    || offset.saturating_add(bytes.len() as u64) > resource.total
                {
                    return Err("invalid resource chunk".into());
                }
                resource.bytes.extend_from_slice(&bytes);
            }
            Part::EndResource => {
                let resource = self.resource.take().ok_or("no resource declaration")?;
                if resource.bytes.len() as u64 != resource.total {
                    return Err("incomplete script resource".into());
                }
                self.script
                    .as_mut()
                    .ok_or("no script declaration")?
                    .resources
                    .insert(
                        resource.path,
                        ScriptResource::new(resource.kind, resource.bytes),
                    );
            }
            Part::EndScript => {
                if self.resource.is_some() {
                    return Err("incomplete script resource".into());
                }
                let script = self.script.take().ok_or("no script declaration")?;
                let bundle = ScriptResourceBundle::new(script.entrypoint, script.resources, limits)
                    .map_err(|error| alloc::format!("{error}"))?;
                self.scripts.0.push(SessionScriptData {
                    document_id: script.id,
                    name: script.name,
                    enabled: script.enabled,
                    bundle: Arc::new(bundle),
                });
            }
            Part::Finish => {
                if self.script.is_some() || self.resource.is_some() {
                    return Err("incomplete script transfer".into());
                }
                self.scripts.validate()?;
                self.finished = true;
            }
        }
        if self.metadata > MAX_SCRIPT_METADATA_BYTES {
            return Err("script metadata budget exceeded".into());
        }
        Ok(())
    }
    pub fn finish(self) -> Result<SessionScripts, String> {
        if !self.finished {
            return Err("incomplete script transfer".into());
        }
        Ok(self.scripts)
    }
    pub fn ready(&self) -> bool {
        self.finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn exact_resources_survive_streaming_and_malformed_chunks_refuse() {
        let bytes: Vec<_> = (0..CHUNK_BYTES * 3 + 7).map(|n| (n % 251) as u8).collect();
        let bundle = ScriptResourceBundle::new(
            NormalizedRelativePath::parse("main.lua").unwrap(),
            BTreeMap::from([
                (
                    NormalizedRelativePath::parse("main.lua").unwrap(),
                    ScriptResource::new(ResourceKind::Lua, &b"return 1"[..]),
                ),
                (
                    NormalizedRelativePath::parse("help/icon.png").unwrap(),
                    ScriptResource::new(ResourceKind::Image, bytes.clone()),
                ),
            ]),
            ResourceLimits::default(),
        )
        .unwrap();
        let expected = SessionScripts(alloc::vec![SessionScriptData {
            document_id: 12,
            name: "portable.lua".into(),
            enabled: true,
            bundle: Arc::new(bundle)
        }]);
        let mut source = Source::new(expected.clone()).unwrap();
        let mut assembly = Assembly::default();
        while let Some(part) = source.next_part() {
            assembly.apply(part).unwrap();
        }
        assert_eq!(assembly.finish().unwrap(), expected);
        let mut assembly = Assembly::default();
        assembly
            .apply(Part::Script {
                id: 1,
                name: "one".into(),
                enabled: true,
                entrypoint: NormalizedRelativePath::parse("main.lua").unwrap(),
            })
            .unwrap();
        assert!(assembly
            .apply(Part::Resource {
                path: NormalizedRelativePath::parse("main.lua").unwrap(),
                kind: ResourceKind::Lua,
                bytes: ResourceLimits::default().max_file_bytes + 1
            })
            .is_err());
        assert!(Assembly::default().finish().is_err());
        let mut assembly = Assembly::default();
        assembly
            .apply(Part::Script {
                id: 1,
                name: "one".into(),
                enabled: true,
                entrypoint: NormalizedRelativePath::parse("main.lua").unwrap(),
            })
            .unwrap();
        assembly
            .apply(Part::Resource {
                path: NormalizedRelativePath::parse("main.lua").unwrap(),
                kind: ResourceKind::Lua,
                bytes: 8,
            })
            .unwrap();
        assert!(assembly
            .apply(Part::Chunk {
                offset: 1,
                bytes: alloc::vec![1]
            })
            .is_err());
        assert!(assembly.apply(Part::EndResource).is_err());
    }
}
