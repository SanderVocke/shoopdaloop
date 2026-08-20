#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, OnceLock, RwLock};
use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ResourcePathError {
    #[error("path must name a non-empty relative location")]
    Empty,
    #[error("path must use slash separators")]
    Backslash,
    #[error("path component {0:?} is not allowed")]
    Component(String),
    #[error("absolute paths are not allowed")]
    Absolute,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NormalizedRelativePath(String);

impl NormalizedRelativePath {
    pub fn parse(path: impl AsRef<str>) -> Result<Self, ResourcePathError> {
        let path = path.as_ref();
        if path.is_empty() {
            return Err(ResourcePathError::Empty);
        }
        if path.contains('\\') {
            return Err(ResourcePathError::Backslash);
        }
        if path.starts_with('/') {
            return Err(ResourcePathError::Absolute);
        }
        let mut normalized = Vec::new();
        for component in path.split('/') {
            if component.is_empty() || component == "." || component == ".." {
                return Err(ResourcePathError::Component(component.to_owned()));
            }
            if component.contains('\0') || component.contains(':') {
                return Err(ResourcePathError::Component(component.to_owned()));
            }
            normalized.push(component);
        }
        Ok(Self(normalized.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    pub fn parent(&self) -> Option<Self> {
        self.0
            .rsplit_once('/')
            .map(|(parent, _)| Self(parent.to_owned()))
    }

    pub fn join(&self, path: &NormalizedRelativePath) -> Result<Self, ResourcePathError> {
        Self::parse(format!("{}/{}", self.0, path.0))
    }

    pub fn case_folded(&self) -> String {
        self.0.to_lowercase()
    }
}

impl fmt::Display for NormalizedRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TryFrom<String> for NormalizedRelativePath {
    type Error = ResourcePathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<NormalizedRelativePath> for String {
    fn from(value: NormalizedRelativePath) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Lua,
    Markdown,
    Image,
}

pub fn classify_resource(path: &NormalizedRelativePath) -> Option<ResourceKind> {
    let (_, extension) = path.file_name().rsplit_once('.')?;
    if extension.eq_ignore_ascii_case("lua") {
        Some(ResourceKind::Lua)
    } else if extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown") {
        Some(ResourceKind::Markdown)
    } else if extension.eq_ignore_ascii_case("png") {
        Some(ResourceKind::Image)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    pub max_file_bytes: u64,
    pub max_script_bytes: u64,
    pub max_aggregate_bytes: u64,
    pub max_files_per_script: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 16 * 1024 * 1024,
            max_script_bytes: 64 * 1024 * 1024,
            max_aggregate_bytes: 256 * 1024 * 1024,
            max_files_per_script: 10_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptResource {
    pub kind: ResourceKind,
    pub bytes: Arc<[u8]>,
}

impl ScriptResource {
    pub fn new(kind: ResourceKind, bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            kind,
            bytes: bytes.into(),
        }
    }

    pub fn sha256(&self) -> String {
        sha256(&self.bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptResourceBundle {
    pub entrypoint: NormalizedRelativePath,
    pub resources: Arc<BTreeMap<NormalizedRelativePath, ScriptResource>>,
}

impl ScriptResourceBundle {
    pub fn new(
        entrypoint: NormalizedRelativePath,
        resources: BTreeMap<NormalizedRelativePath, ScriptResource>,
        limits: ResourceLimits,
    ) -> Result<Self, BundleError> {
        validate_resources(&entrypoint, &resources, limits)?;
        Ok(Self {
            entrypoint,
            resources: Arc::new(resources),
        })
    }

    pub fn source_only(name: &str, source: impl Into<Arc<[u8]>>) -> Result<Self, BundleError> {
        let entrypoint = NormalizedRelativePath::parse(name)?;
        let mut resources = BTreeMap::new();
        resources.insert(
            entrypoint.clone(),
            ScriptResource::new(ResourceKind::Lua, source),
        );
        Self::new(entrypoint, resources, ResourceLimits::default())
    }

    pub fn entrypoint_resource(&self) -> &ScriptResource {
        &self.resources[&self.entrypoint]
    }

    pub fn get(&self, path: &NormalizedRelativePath) -> Option<&ScriptResource> {
        self.resources.get(path)
    }

    pub fn byte_count(&self) -> u64 {
        self.resources
            .values()
            .map(|resource| resource.bytes.len() as u64)
            .sum()
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BundleError {
    #[error(transparent)]
    Path(#[from] ResourcePathError),
    #[error("entrypoint {0:?} is missing")]
    MissingEntrypoint(String),
    #[error("entrypoint {0:?} is not Lua")]
    InvalidEntrypoint(String),
    #[error("resource {0:?} has a kind that does not match its extension")]
    KindMismatch(String),
    #[error("resource path differs only by case: {0:?}")]
    CaseCollision(String),
    #[error("resource {path:?} is {bytes} bytes; limit is {limit}")]
    FileLimit {
        path: String,
        bytes: u64,
        limit: u64,
    },
    #[error("script has {files} files; limit is {limit}")]
    FileCountLimit { files: usize, limit: usize },
    #[error("script resources are {bytes} bytes; limit is {limit}")]
    ScriptLimit { bytes: u64, limit: u64 },
}

pub fn validate_resources(
    entrypoint: &NormalizedRelativePath,
    resources: &BTreeMap<NormalizedRelativePath, ScriptResource>,
    limits: ResourceLimits,
) -> Result<(), BundleError> {
    if resources.len() > limits.max_files_per_script {
        return Err(BundleError::FileCountLimit {
            files: resources.len(),
            limit: limits.max_files_per_script,
        });
    }
    let mut case_folded = BTreeSet::new();
    let mut total = 0_u64;
    for (path, resource) in resources {
        if !case_folded.insert(path.case_folded()) {
            return Err(BundleError::CaseCollision(path.to_string()));
        }
        if classify_resource(path) != Some(resource.kind) {
            return Err(BundleError::KindMismatch(path.to_string()));
        }
        let bytes = resource.bytes.len() as u64;
        if bytes > limits.max_file_bytes {
            return Err(BundleError::FileLimit {
                path: path.to_string(),
                bytes,
                limit: limits.max_file_bytes,
            });
        }
        total = total.saturating_add(bytes);
        if total > limits.max_script_bytes {
            return Err(BundleError::ScriptLimit {
                bytes: total,
                limit: limits.max_script_bytes,
            });
        }
    }
    let entrypoint_resource = resources
        .get(entrypoint)
        .ok_or_else(|| BundleError::MissingEntrypoint(entrypoint.to_string()))?;
    if entrypoint_resource.kind != ResourceKind::Lua {
        return Err(BundleError::InvalidEntrypoint(entrypoint.to_string()));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceOrigin {
    pub scope: String,
    pub generation: u64,
}

impl ResourceOrigin {
    pub fn base_uri(&self) -> String {
        format!(
            "shoop-script-resource://{}/{}/",
            self.scope, self.generation
        )
    }

    pub fn base_uri_below(&self, path: &NormalizedRelativePath) -> String {
        match path.parent() {
            Some(parent) => format!("{}{}/", self.base_uri(), percent_encode_path(&parent)),
            None => self.base_uri(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum RegisteredResourceProvider {
    Filesystem(std::path::PathBuf),
    Bundle(Arc<ScriptResourceBundle>),
}

static RESOURCE_REGISTRY: OnceLock<RwLock<BTreeMap<(String, u64), RegisteredResourceProvider>>> =
    OnceLock::new();

fn resource_registry() -> &'static RwLock<BTreeMap<(String, u64), RegisteredResourceProvider>> {
    RESOURCE_REGISTRY.get_or_init(|| RwLock::new(BTreeMap::new()))
}

pub fn register_resource_provider(
    origin: &ResourceOrigin,
    provider: RegisteredResourceProvider,
) -> Result<(), String> {
    if origin.scope.is_empty()
        || !origin
            .scope
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("resource scope contains unsupported characters".to_owned());
    }
    if let RegisteredResourceProvider::Filesystem(root) = &provider {
        let canonical = root
            .canonicalize()
            .map_err(|error| format!("could not resolve script resource root: {error}"))?;
        if !canonical.is_dir() {
            return Err("script resource root is not a directory".to_owned());
        }
        let provider = RegisteredResourceProvider::Filesystem(canonical);
        let mut registry = resource_registry()
            .write()
            .unwrap_or_else(|error| error.into_inner());
        registry.retain(|(scope, _), _| scope != &origin.scope);
        registry.insert((origin.scope.clone(), origin.generation), provider);
    } else {
        let mut registry = resource_registry()
            .write()
            .unwrap_or_else(|error| error.into_inner());
        registry.retain(|(scope, _), _| scope != &origin.scope);
        registry.insert((origin.scope.clone(), origin.generation), provider);
    }
    Ok(())
}

pub fn unregister_resource_scope(scope: &str) {
    resource_registry()
        .write()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|(registered, _), _| registered != scope);
}

pub fn read_resource_uri(uri: &str) -> Result<Option<Arc<[u8]>>, String> {
    let Some(rest) = uri.strip_prefix("shoop-script-resource://") else {
        return Ok(None);
    };
    let (scope, rest) = rest
        .split_once('/')
        .ok_or_else(|| "script resource URI has no generation".to_owned())?;
    if scope.is_empty()
        || !scope
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("script resource URI has an invalid scope".to_owned());
    }
    let (generation, encoded_path) = rest
        .split_once('/')
        .ok_or_else(|| "script resource URI has no path".to_owned())?;
    let generation = generation
        .parse::<u64>()
        .map_err(|_| "script resource URI has an invalid generation".to_owned())?;
    let decoded = percent_decode_path(encoded_path)?;
    let path = NormalizedRelativePath::parse(decoded).map_err(|error| error.to_string())?;
    let provider = resource_registry()
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .get(&(scope.to_owned(), generation))
        .cloned()
        .ok_or_else(|| "script resource scope is stale or unknown".to_owned())?;
    match provider {
        RegisteredResourceProvider::Bundle(bundle) => bundle
            .get(&path)
            .map(|resource| Arc::clone(&resource.bytes))
            .ok_or_else(|| format!("undeclared script resource {path:?}"))
            .map(Some),
        RegisteredResourceProvider::Filesystem(root) => {
            let resolved = root
                .join(path.as_str())
                .canonicalize()
                .map_err(|error| format!("could not resolve script resource {path:?}: {error}"))?;
            if !resolved.starts_with(&root) || !resolved.is_file() {
                return Err(format!("script resource {path:?} escapes its scope"));
            }
            std::fs::read(&resolved)
                .map(Arc::<[u8]>::from)
                .map(Some)
                .map_err(|error| format!("could not read script resource {path:?}: {error}"))
        }
    }
}

fn percent_encode_path(path: &NormalizedRelativePath) -> String {
    let mut encoded = String::new();
    for byte in path.as_str().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn percent_decode_path(encoded: &str) -> Result<String, String> {
    if encoded.is_empty() {
        return Err("script resource URI path is empty".to_owned());
    }
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("script resource URI has truncated percent encoding".to_owned());
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| "script resource URI has malformed percent encoding".to_owned())?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| "script resource URI has malformed percent encoding".to_owned())?;
            let byte = (high << 4) | low;
            if matches!(byte, b'/' | b'\\' | 0) {
                return Err("script resource URI encodes a forbidden separator".to_owned());
            }
            decoded.push(byte);
            index += 3;
        } else {
            if bytes[index] == b'\\' {
                return Err("script resource URI uses a backslash".to_owned());
            }
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "script resource URI path is not UTF-8".to_owned())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltinCatalogEntry {
    pub identity: NormalizedRelativePath,
    pub source: Arc<str>,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanDiagnostic {
    pub path: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BuiltinCatalog {
    pub generation: u64,
    pub entries: Vec<BuiltinCatalogEntry>,
    pub diagnostics: Vec<ScanDiagnostic>,
    pub deletions_safe: bool,
}

pub fn sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Error)]
pub enum ScanError {
    #[error("could not resolve script root {path}: {source}")]
    Root {
        path: String,
        source: std::io::Error,
    },
    #[error("could not scan {path}: {message}")]
    Path { path: String, message: String },
    #[error(transparent)]
    Bundle(#[from] BundleError),
}

#[cfg(not(target_arch = "wasm32"))]
pub fn scan_builtin_directory(
    root: &std::path::Path,
    generation: u64,
    limits: ResourceLimits,
) -> Result<BuiltinCatalog, ScanError> {
    let canonical_root = root.canonicalize().map_err(|source| ScanError::Root {
        path: root.display().to_string(),
        source,
    })?;
    if !canonical_root.is_dir() {
        return Err(ScanError::Path {
            path: root.display().to_string(),
            message: "built-ins root is not a directory".to_owned(),
        });
    }
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    collect_regular_files(
        &canonical_root,
        &canonical_root,
        &mut files,
        &mut diagnostics,
        false,
    )?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut entries = Vec::new();
    let mut identities = BTreeSet::new();
    let mut total = 0_u64;
    for (identity, path) in files {
        if classify_resource(&identity) != Some(ResourceKind::Lua) {
            continue;
        }
        if !identities.insert(identity.case_folded()) {
            diagnostics.push(ScanDiagnostic {
                path: Some(identity.to_string()),
                message: "script identity differs only by case from another script".to_owned(),
            });
            continue;
        }
        let metadata = match path.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(ScanDiagnostic {
                    path: Some(identity.to_string()),
                    message: format!("could not inspect script: {error}"),
                });
                continue;
            }
        };
        if metadata.len() > limits.max_file_bytes {
            diagnostics.push(ScanDiagnostic {
                path: Some(identity.to_string()),
                message: format!(
                    "script is {} bytes; per-file limit is {}",
                    metadata.len(),
                    limits.max_file_bytes
                ),
            });
            continue;
        }
        if total
            .checked_add(metadata.len())
            .is_none_or(|bytes| bytes > limits.max_aggregate_bytes)
        {
            diagnostics.push(ScanDiagnostic {
                path: Some(identity.to_string()),
                message: format!(
                    "built-in scan exceeds aggregate limit {}",
                    limits.max_aggregate_bytes
                ),
            });
            break;
        }
        let remaining = limits.max_aggregate_bytes - total;
        let bytes =
            match read_file_limited(&path, metadata.len(), limits.max_file_bytes.min(remaining)) {
                Ok(bytes) => bytes,
                Err(error) => {
                    diagnostics.push(ScanDiagnostic {
                        path: Some(identity.to_string()),
                        message: format!("could not read script: {error}"),
                    });
                    continue;
                }
            };
        total += bytes.len() as u64;
        let hash = sha256(&bytes);
        match String::from_utf8(bytes) {
            Ok(source) => entries.push(BuiltinCatalogEntry {
                identity,
                source: Arc::from(source),
                sha256: hash,
            }),
            Err(error) => diagnostics.push(ScanDiagnostic {
                path: Some(identity.to_string()),
                message: format!("script is not UTF-8: {error}"),
            }),
        }
    }
    let deletions_safe = diagnostics.iter().all(|diagnostic| {
        diagnostic
            .path
            .as_deref()
            .and_then(|path| NormalizedRelativePath::parse(path).ok())
            .is_some_and(|path| classify_resource(&path) == Some(ResourceKind::Lua))
            && !diagnostic.message.contains("differs only by case")
    });
    Ok(BuiltinCatalog {
        generation,
        entries,
        diagnostics,
        deletions_safe,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn capture_filesystem_bundle(
    script_path: &std::path::Path,
    current_source: impl Into<Arc<[u8]>>,
    limits: ResourceLimits,
) -> Result<ScriptResourceBundle, ScanError> {
    let file_name = script_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ScanError::Path {
            path: script_path.display().to_string(),
            message: "script filename is not valid UTF-8".to_owned(),
        })?;
    let entrypoint = NormalizedRelativePath::parse(file_name).map_err(BundleError::from)?;
    if classify_resource(&entrypoint) != Some(ResourceKind::Lua) {
        return Err(ScanError::Path {
            path: script_path.display().to_string(),
            message: "script entrypoint must have a .lua extension".to_owned(),
        });
    }
    let parent = script_path.parent().ok_or_else(|| ScanError::Path {
        path: script_path.display().to_string(),
        message: "script has no parent directory".to_owned(),
    })?;
    let root = parent.canonicalize().map_err(|source| ScanError::Root {
        path: parent.display().to_string(),
        source,
    })?;
    let resolved_script = script_path
        .canonicalize()
        .map_err(|source| ScanError::Root {
            path: script_path.display().to_string(),
            source,
        })?;
    if !resolved_script.starts_with(&root) || !resolved_script.is_file() {
        return Err(ScanError::Path {
            path: script_path.display().to_string(),
            message: "script resolves outside its parent directory or is not a file".to_owned(),
        });
    }
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    collect_regular_files(&root, &root, &mut files, &mut diagnostics, true)?;
    if let Some(diagnostic) = diagnostics.into_iter().next() {
        return Err(ScanError::Path {
            path: diagnostic
                .path
                .unwrap_or_else(|| root.display().to_string()),
            message: diagnostic.message,
        });
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let current_source = current_source.into();
    let source_bytes = current_source.len() as u64;
    if source_bytes > limits.max_file_bytes {
        return Err(ScanError::Bundle(BundleError::FileLimit {
            path: entrypoint.to_string(),
            bytes: source_bytes,
            limit: limits.max_file_bytes,
        }));
    }
    if limits.max_files_per_script == 0 {
        return Err(ScanError::Bundle(BundleError::FileCountLimit {
            files: 1,
            limit: 0,
        }));
    }
    if source_bytes > limits.max_script_bytes {
        return Err(ScanError::Bundle(BundleError::ScriptLimit {
            bytes: source_bytes,
            limit: limits.max_script_bytes,
        }));
    }
    let mut resource_count = 1_usize;
    let mut total = source_bytes;
    let mut case_folded = BTreeSet::from([entrypoint.case_folded()]);
    let mut resources = BTreeMap::new();
    resources.insert(
        entrypoint.clone(),
        ScriptResource::new(ResourceKind::Lua, current_source),
    );
    for (relative, path) in files {
        let Some(kind) = classify_resource(&relative) else {
            continue;
        };
        if kind == ResourceKind::Lua {
            continue;
        }
        resource_count = resource_count.saturating_add(1);
        if resource_count > limits.max_files_per_script {
            return Err(ScanError::Bundle(BundleError::FileCountLimit {
                files: resource_count,
                limit: limits.max_files_per_script,
            }));
        }
        if !case_folded.insert(relative.case_folded()) {
            return Err(ScanError::Bundle(BundleError::CaseCollision(
                relative.to_string(),
            )));
        }
        let metadata = path.metadata().map_err(|error| ScanError::Path {
            path: relative.to_string(),
            message: format!("could not inspect resource: {error}"),
        })?;
        if metadata.len() > limits.max_file_bytes {
            return Err(ScanError::Bundle(BundleError::FileLimit {
                path: relative.to_string(),
                bytes: metadata.len(),
                limit: limits.max_file_bytes,
            }));
        }
        if total
            .checked_add(metadata.len())
            .is_none_or(|bytes| bytes > limits.max_script_bytes)
        {
            return Err(ScanError::Bundle(BundleError::ScriptLimit {
                bytes: total.saturating_add(metadata.len()),
                limit: limits.max_script_bytes,
            }));
        }
        let bytes = read_file_limited(
            &path,
            metadata.len(),
            limits.max_file_bytes.min(limits.max_script_bytes - total),
        )
        .map_err(|error| ScanError::Path {
            path: relative.to_string(),
            message: format!("could not read resource: {error}"),
        })?;
        total += bytes.len() as u64;
        if resources
            .insert(
                relative.clone(),
                ScriptResource::new(kind, Arc::<[u8]>::from(bytes)),
            )
            .is_some()
        {
            return Err(ScanError::Path {
                path: relative.to_string(),
                message: "duplicate normalized resource path".to_owned(),
            });
        }
    }
    ScriptResourceBundle::new(entrypoint, resources, limits).map_err(ScanError::from)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file_limited(
    path: &std::path::Path,
    declared_bytes: u64,
    limit: u64,
) -> Result<Vec<u8>, String> {
    use std::io::Read as _;

    if declared_bytes > limit {
        return Err(format!(
            "declared size {declared_bytes} exceeds limit {limit}"
        ));
    }
    let capacity = usize::try_from(declared_bytes)
        .map_err(|_| "declared size does not fit memory".to_owned())?;
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("file expanded beyond limit {limit}"));
    }
    Ok(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_regular_files(
    root: &std::path::Path,
    directory: &std::path::Path,
    files: &mut Vec<(NormalizedRelativePath, std::path::PathBuf)>,
    diagnostics: &mut Vec<ScanDiagnostic>,
    strict: bool,
) -> Result<(), ScanError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if !strict && directory != root => {
            diagnostics.push(ScanDiagnostic {
                path: Some(directory.display().to_string()),
                message: format!("could not read directory: {error}"),
            });
            return Ok(());
        }
        Err(error) => {
            return Err(ScanError::Path {
                path: directory.display().to_string(),
                message: error.to_string(),
            });
        }
    };
    let mut entries = match entries.collect::<Result<Vec<_>, _>>() {
        Ok(entries) => entries,
        Err(error) if !strict => {
            diagnostics.push(ScanDiagnostic {
                path: Some(directory.display().to_string()),
                message: format!("could not enumerate directory: {error}"),
            });
            return Ok(());
        }
        Err(error) => {
            return Err(ScanError::Path {
                path: directory.display().to_string(),
                message: error.to_string(),
            });
        }
    };
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative_os = path.strip_prefix(root).map_err(|error| ScanError::Path {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
        let relative_text = relative_os
            .components()
            .map(|component| component.as_os_str().to_str())
            .collect::<Option<Vec<_>>>()
            .map(|components| components.join("/"));
        let Some(relative_text) = relative_text else {
            let diagnostic = ScanDiagnostic {
                path: Some(path.display().to_string()),
                message: "path is not valid UTF-8".to_owned(),
            };
            diagnostics.push(diagnostic);
            continue;
        };
        let relative = match NormalizedRelativePath::parse(&relative_text) {
            Ok(relative) => relative,
            Err(error) => {
                diagnostics.push(ScanDiagnostic {
                    path: Some(relative_text),
                    message: error.to_string(),
                });
                continue;
            }
        };
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(ScanDiagnostic {
                    path: Some(relative.to_string()),
                    message: format!("could not inspect path: {error}"),
                });
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            let resolved = match path.canonicalize() {
                Ok(resolved) => resolved,
                Err(error) => {
                    diagnostics.push(ScanDiagnostic {
                        path: Some(relative.to_string()),
                        message: format!("could not resolve symlink: {error}"),
                    });
                    continue;
                }
            };
            if !resolved.starts_with(root) {
                diagnostics.push(ScanDiagnostic {
                    path: Some(relative.to_string()),
                    message: "symlink target escapes the selected root".to_owned(),
                });
            } else if resolved.is_dir() {
                diagnostics.push(ScanDiagnostic {
                    path: Some(relative.to_string()),
                    message: "directory symlinks are not followed".to_owned(),
                });
            } else if resolved.is_file() {
                files.push((relative, resolved));
            }
        } else if metadata.is_dir() {
            collect_regular_files(root, &path, files, diagnostics, strict)?;
        } else if metadata.is_file() {
            let resolved = path.canonicalize().map_err(|error| ScanError::Path {
                path: relative.to_string(),
                message: format!("could not resolve file: {error}"),
            })?;
            if !resolved.starts_with(root) {
                diagnostics.push(ScanDiagnostic {
                    path: Some(relative.to_string()),
                    message: "file target escapes the selected root".to_owned(),
                });
            } else {
                files.push((relative, resolved));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn normalizes_only_strict_relative_paths() {
        for valid in ["keyboard.lua", "controllers/MK1.LUA", "help/a.png"] {
            assert_eq!(
                NormalizedRelativePath::parse(valid).unwrap().as_str(),
                valid
            );
        }
        for invalid in [
            "", ".", "..", "/x", "a//b", "a/./b", "a/../b", "a\\b", "C:/x",
        ] {
            assert!(
                NormalizedRelativePath::parse(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn classifies_extensions_case_insensitively() {
        let classify = |path| classify_resource(&NormalizedRelativePath::parse(path).unwrap());
        assert_eq!(classify("a.LuA"), Some(ResourceKind::Lua));
        assert_eq!(classify("a.MarkDown"), Some(ResourceKind::Markdown));
        assert_eq!(classify("a.PNG"), Some(ResourceKind::Image));
        assert_eq!(classify("a.jpg"), None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bundle_rejects_case_collisions_and_wrong_kinds() {
        let entrypoint = NormalizedRelativePath::parse("main.lua").unwrap();
        let mut resources = BTreeMap::from([(
            entrypoint.clone(),
            ScriptResource::new(ResourceKind::Lua, Arc::<[u8]>::from(&b"return"[..])),
        )]);
        resources.insert(
            NormalizedRelativePath::parse("MAIN.LUA").unwrap(),
            ScriptResource::new(ResourceKind::Lua, Arc::<[u8]>::from(&b"return"[..])),
        );
        assert!(matches!(
            ScriptResourceBundle::new(entrypoint.clone(), resources, ResourceLimits::default()),
            Err(BundleError::CaseCollision(_))
        ));

        let resources = BTreeMap::from([(
            entrypoint.clone(),
            ScriptResource::new(ResourceKind::Image, Arc::<[u8]>::from(&b"return"[..])),
        )]);
        assert!(matches!(
            ScriptResourceBundle::new(entrypoint, resources, ResourceLimits::default()),
            Err(BundleError::KindMismatch(_))
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn registry_is_generation_scoped_and_rejects_malformed_uris() {
        let bundle = Arc::new(
            ScriptResourceBundle::source_only("main.lua", Arc::<[u8]>::from(&b"one"[..])).unwrap(),
        );
        let first = ResourceOrigin {
            scope: "script-1".to_owned(),
            generation: 1,
        };
        register_resource_provider(
            &first,
            RegisteredResourceProvider::Bundle(Arc::clone(&bundle)),
        )
        .unwrap();
        assert_eq!(
            read_resource_uri("shoop-script-resource://script-1/1/main.lua")
                .unwrap()
                .unwrap()
                .as_ref(),
            b"one"
        );

        let second = ResourceOrigin {
            scope: "script-1".to_owned(),
            generation: 2,
        };
        register_resource_provider(&second, RegisteredResourceProvider::Bundle(bundle)).unwrap();
        assert!(read_resource_uri("shoop-script-resource://script-1/1/main.lua").is_err());
        for uri in [
            "shoop-script-resource://script-1/2/../main.lua",
            "shoop-script-resource://script-1/2/%2Fmain.lua",
            "shoop-script-resource://script-1/2/%GG",
            "shoop-script-resource://script-2/2/main.lua",
        ] {
            assert!(read_resource_uri(uri).is_err(), "accepted {uri:?}");
        }
        unregister_resource_scope("script-1");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn native_scan_is_recursive_deterministic_and_isolates_bad_utf8() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("z")).unwrap();
        std::fs::write(temporary.path().join("z/second.LUA"), "return 2").unwrap();
        std::fs::write(temporary.path().join("first.lua"), "return 1").unwrap();
        std::fs::write(temporary.path().join("bad.lua"), [0xff]).unwrap();
        std::fs::write(temporary.path().join("ignored.txt"), "ignored").unwrap();

        let catalog =
            scan_builtin_directory(temporary.path(), 7, ResourceLimits::default()).unwrap();
        assert_eq!(catalog.generation, 7);
        assert_eq!(
            catalog
                .entries
                .iter()
                .map(|entry| entry.identity.as_str())
                .collect::<Vec<_>>(),
            ["first.lua", "z/second.LUA"]
        );
        assert_eq!(catalog.diagnostics.len(), 1);
        assert_eq!(catalog.diagnostics[0].path.as_deref(), Some("bad.lua"));
        assert!(catalog.deletions_safe);
    }

    #[cfg(all(not(target_arch = "wasm32"), unix))]
    #[shoop_wasm_test_support::shoop_test]
    fn scan_marks_incomplete_directory_enumeration_as_unsafe_for_deletions() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("real");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("script.lua"), "return").unwrap();
        std::os::unix::fs::symlink(&target, temporary.path().join("linked")).unwrap();

        let catalog =
            scan_builtin_directory(temporary.path(), 1, ResourceLimits::default()).unwrap();
        assert!(!catalog.deletions_safe);
        assert!(catalog
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("directory symlinks")));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn conversion_captures_nested_supported_companions_and_current_source() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temporary.path().join("help/images")).unwrap();
        let script = temporary.path().join("controller.lua");
        std::fs::write(&script, "old source").unwrap();
        std::fs::write(temporary.path().join("help/readme.md"), "markdown").unwrap();
        std::fs::write(temporary.path().join("help/images/a.png"), [1, 2, 3]).unwrap();
        std::fs::write(temporary.path().join("help/ignored.jpg"), [4]).unwrap();

        let bundle = capture_filesystem_bundle(
            &script,
            Arc::<[u8]>::from(&b"current source"[..]),
            ResourceLimits::default(),
        )
        .unwrap();
        assert_eq!(
            bundle.entrypoint_resource().bytes.as_ref(),
            b"current source"
        );
        assert_eq!(
            bundle
                .resources
                .keys()
                .map(NormalizedRelativePath::as_str)
                .collect::<Vec<_>>(),
            ["controller.lua", "help/images/a.png", "help/readme.md"]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn conversion_enforces_count_and_aggregate_limits_before_retaining_files() {
        let temporary = tempfile::tempdir().unwrap();
        let script = temporary.path().join("controller.lua");
        std::fs::write(&script, "return").unwrap();
        std::fs::write(temporary.path().join("one.md"), [1, 2, 3]).unwrap();
        std::fs::write(temporary.path().join("two.md"), [4, 5, 6]).unwrap();

        let count_error = capture_filesystem_bundle(
            &script,
            Arc::<[u8]>::from(&b"return"[..]),
            ResourceLimits {
                max_files_per_script: 2,
                ..ResourceLimits::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            count_error,
            ScanError::Bundle(BundleError::FileCountLimit { .. })
        ));

        let size_error = capture_filesystem_bundle(
            &script,
            Arc::<[u8]>::from(&b"return"[..]),
            ResourceLimits {
                max_script_bytes: 8,
                ..ResourceLimits::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            size_error,
            ScanError::Bundle(BundleError::ScriptLimit { .. })
        ));
    }

    #[cfg(all(not(target_arch = "wasm32"), unix))]
    #[shoop_wasm_test_support::shoop_test]
    fn conversion_rejects_an_escaping_symlink_atomically() {
        let temporary = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let script = temporary.path().join("controller.lua");
        std::fs::write(&script, "return").unwrap();
        std::os::unix::fs::symlink(outside.path(), temporary.path().join("outside.md")).unwrap();

        let error = capture_filesystem_bundle(
            &script,
            Arc::<[u8]>::from(&b"return"[..]),
            ResourceLimits::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("escapes"));
    }
}
