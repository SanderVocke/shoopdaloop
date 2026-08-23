use crate::document::{
    AudioPayload, ChannelModeDocument, CompositeKindDocument, CueOutputSelectionDocument,
    DataTypeDocument, FormatVersion, FxChainTypeDocument, LatencyCertaintyDocument, MediaPayload,
    PortDirectionDocument, SessionBundle, SessionDocument, TakeLatencyDocument, TrackDocument,
    TrackLatencyPolicyDocument, TrackTopologyDocument, AUDIO_FORMAT, DOCUMENT_VERSION,
    FORMAT_MAJOR, FORMAT_MINOR, MIDI_FORMAT, SESSION_DOCUMENT_VERSION, SESSION_FORMAT,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shoop_script_resources::{
    NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource, ScriptResourceBundle,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::sync::Arc;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MANIFEST_PATH: &str = "manifest.json";
const DEFAULT_MAX_ENTRIES: usize = 1_000_000;
const DEFAULT_MAX_UNCOMPRESSED_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub max_entries: usize,
    pub max_uncompressed_bytes: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_MAX_ENTRIES,
            max_uncompressed_bytes: DEFAULT_MAX_UNCOMPRESSED_BYTES,
        }
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("unsupported file format; expected a Shoop ZIP64 container")]
    UnsupportedFormat,
    #[error("unsupported {format} format version {major}.{minor}")]
    UnsupportedVersion {
        format: String,
        major: u16,
        minor: u16,
    },
    #[error("archive is malformed: {0}")]
    Archive(String),
    #[error("manifest is malformed: {0}")]
    Manifest(String),
    #[error("session document is invalid: {0}")]
    Validation(String),
    #[error("archive exceeds resource limits: {0}")]
    ResourceLimit(String),
    #[error("media payload {id} is missing")]
    MissingMedia { id: String },
    #[error("media payload {id} failed its SHA-256 check")]
    HashMismatch { id: String },
    #[error("media payload {id} has an invalid shape")]
    InvalidMediaShape { id: String },
}

impl From<zip::result::ZipError> for SessionError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Archive(value.to_string())
    }
}

impl From<std::io::Error> for SessionError {
    fn from(value: std::io::Error) -> Self {
        Self::Archive(value.to_string())
    }
}

#[derive(Deserialize)]
struct SessionManifestHeader {
    format: String,
    format_version: FormatVersion,
    document_version: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionManifest {
    format: String,
    format_version: FormatVersion,
    document_version: u16,
    writer_app_version: String,
    sample_rate: u32,
    document: SessionDocument,
    media: Vec<MediaRecord>,
    #[serde(default)]
    scripts: Vec<ScriptResourceRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ScriptResourceRecord {
    owner_script_id: u64,
    relative_path: String,
    path: String,
    kind: ResourceKind,
    uncompressed_bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MediaRecord {
    id: String,
    path: String,
    kind: MediaKind,
    uncompressed_bytes: u64,
    sha256: String,
    frames: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MediaKind {
    AudioF32Le,
    ExactMidiJson,
}

pub fn encode_session(
    bundle: &SessionBundle,
    writer_app_version: &str,
) -> Result<Vec<u8>, SessionError> {
    validate_bundle(bundle)?;
    let mut payloads = BTreeMap::<String, Vec<u8>>::new();
    let mut records = Vec::with_capacity(bundle.media.len());
    for (id, payload) in &bundle.media {
        validate_media_id(id)?;
        let (kind, extension, frames, bytes) = match payload {
            MediaPayload::Audio(audio) => (
                MediaKind::AudioF32Le,
                "f32le",
                audio.samples.len() as u64,
                encode_audio_samples(&audio.samples),
            ),
            MediaPayload::Midi(midi) => (
                MediaKind::ExactMidiJson,
                "json",
                midi.length_frames,
                serde_json::to_vec(midi)
                    .map_err(|error| SessionError::Manifest(error.to_string()))?,
            ),
        };
        let folder = match kind {
            MediaKind::AudioF32Le => "audio",
            MediaKind::ExactMidiJson => "midi",
        };
        let path = format!("media/{folder}/{id}.{extension}");
        records.push(MediaRecord {
            id: id.clone(),
            path: path.clone(),
            kind,
            uncompressed_bytes: bytes.len() as u64,
            sha256: sha256(&bytes),
            frames,
        });
        payloads.insert(path, bytes);
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let mut script_records = Vec::new();
    for (owner_script_id, bundle) in &bundle.scripts {
        for (relative_path, resource) in bundle.resources.iter() {
            let path = format!("scripts/{owner_script_id}/{}", relative_path.as_str());
            script_records.push(ScriptResourceRecord {
                owner_script_id: *owner_script_id,
                relative_path: relative_path.to_string(),
                path: path.clone(),
                kind: resource.kind,
                uncompressed_bytes: resource.bytes.len() as u64,
                sha256: resource.sha256(),
            });
            payloads.insert(path, resource.bytes.to_vec());
        }
    }
    script_records.sort_by(|left, right| {
        (left.owner_script_id, &left.relative_path)
            .cmp(&(right.owner_script_id, &right.relative_path))
    });
    let manifest = SessionManifest {
        format: SESSION_FORMAT.to_owned(),
        format_version: FormatVersion::default(),
        document_version: SESSION_DOCUMENT_VERSION,
        writer_app_version: writer_app_version.to_owned(),
        sample_rate: bundle.document.sample_rate,
        document: bundle.document.clone(),
        media: records,
        scripts: script_records,
    };
    encode_zip(&manifest, payloads)
}

pub fn decode_session(bytes: &[u8]) -> Result<SessionBundle, SessionError> {
    decode_session_with_limits(bytes, DecodeLimits::default())
}

pub fn decode_session_with_limits(
    bytes: &[u8],
    limits: DecodeLimits,
) -> Result<SessionBundle, SessionError> {
    if !bytes.starts_with(b"PK") {
        return Err(SessionError::UnsupportedFormat);
    }
    inspect_central_directory(bytes, limits)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let archive_paths = inspect_archive(&mut archive, limits)?;
    let manifest_bytes = read_entry(&mut archive, MANIFEST_PATH, limits.max_uncompressed_bytes)?;
    let manifest_value: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| SessionError::Manifest(error.to_string()))?;
    let header: SessionManifestHeader = serde_json::from_value(manifest_value.clone())
        .map_err(|error| SessionError::Manifest(error.to_string()))?;
    if header.format_version.major != FORMAT_MAJOR
        || header.format_version.minor > FORMAT_MINOR
        || !(6..=SESSION_DOCUMENT_VERSION).contains(&header.document_version)
    {
        return Err(SessionError::UnsupportedVersion {
            format: header.format,
            major: header.format_version.major,
            minor: header.format_version.minor,
        });
    }
    let manifest: SessionManifest = serde_json::from_value(manifest_value.clone())
        .map_err(|error| SessionError::Manifest(error.to_string()))?;
    if manifest.format != SESSION_FORMAT {
        return Err(SessionError::UnsupportedFormat);
    }
    if manifest.sample_rate != manifest.document.sample_rate {
        return Err(SessionError::Validation(
            "manifest and document sample rates differ".to_owned(),
        ));
    }
    let mut declared_paths = BTreeSet::from([MANIFEST_PATH.to_owned()]);
    declared_paths.extend(manifest.media.iter().map(|record| record.path.clone()));
    declared_paths.extend(manifest.scripts.iter().map(|record| record.path.clone()));
    if archive_paths != declared_paths {
        return Err(SessionError::Archive(format!(
            "archive entries do not match the manifest; undeclared={:?}, missing={:?}",
            archive_paths
                .difference(&declared_paths)
                .collect::<Vec<_>>(),
            declared_paths
                .difference(&archive_paths)
                .collect::<Vec<_>>()
        )));
    }
    let script_bundles =
        decode_script_bundles(&mut archive, &manifest.document, manifest.scripts, limits)?;
    let mut media = BTreeMap::new();
    let mut record_ids = BTreeSet::new();
    let mut record_paths = BTreeSet::new();
    for record in manifest.media {
        validate_media_id(&record.id)?;
        if !record_ids.insert(record.id.clone()) || !record_paths.insert(record.path.clone()) {
            return Err(SessionError::Validation(
                "duplicate media ID or path".to_owned(),
            ));
        }
        let payload_bytes = read_entry(&mut archive, &record.path, record.uncompressed_bytes)?;
        if payload_bytes.len() as u64 != record.uncompressed_bytes {
            return Err(SessionError::InvalidMediaShape {
                id: record.id.clone(),
            });
        }
        if sha256(&payload_bytes) != record.sha256 {
            return Err(SessionError::HashMismatch {
                id: record.id.clone(),
            });
        }
        let payload = match record.kind {
            MediaKind::AudioF32Le => {
                let samples = decode_audio_samples(&record.id, &payload_bytes)?;
                if samples.len() as u64 != record.frames {
                    return Err(SessionError::InvalidMediaShape {
                        id: record.id.clone(),
                    });
                }
                MediaPayload::Audio(AudioPayload { samples })
            }
            MediaKind::ExactMidiJson => {
                let midi = serde_json::from_slice(&payload_bytes)
                    .map_err(|error| SessionError::Manifest(error.to_string()))?;
                MediaPayload::Midi(midi)
            }
        };
        media.insert(record.id, payload);
    }
    let bundle = SessionBundle {
        document: manifest.document,
        media,
        scripts: script_bundles,
    };
    validate_bundle(&bundle)?;
    Ok(bundle)
}

fn decode_script_bundles(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    document: &SessionDocument,
    records: Vec<ScriptResourceRecord>,
    limits: DecodeLimits,
) -> Result<BTreeMap<u64, Arc<ScriptResourceBundle>>, SessionError> {
    let resource_limits = ResourceLimits::default();
    let script_entrypoints = document
        .scripts
        .iter()
        .map(|script| {
            NormalizedRelativePath::parse(&script.entrypoint)
                .map(|entrypoint| (script.id, entrypoint))
                .map_err(|error| {
                    SessionError::Validation(format!(
                        "script {} entrypoint is invalid: {error}",
                        script.id
                    ))
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    if script_entrypoints.len() != document.scripts.len() {
        return Err(SessionError::Validation(
            "duplicate script IDs in document".to_owned(),
        ));
    }
    let mut grouped = BTreeMap::<u64, BTreeMap<NormalizedRelativePath, ScriptResource>>::new();
    let mut record_paths = BTreeSet::new();
    let mut aggregate = 0_u64;
    for record in records {
        if !script_entrypoints.contains_key(&record.owner_script_id) {
            return Err(SessionError::Validation(format!(
                "resource belongs to unknown script {}",
                record.owner_script_id
            )));
        }
        let relative = NormalizedRelativePath::parse(&record.relative_path).map_err(|error| {
            SessionError::Validation(format!(
                "script {} resource path is invalid: {error}",
                record.owner_script_id
            ))
        })?;
        let expected_path = format!("scripts/{}/{}", record.owner_script_id, relative.as_str());
        if record.path != expected_path || !record_paths.insert(record.path.clone()) {
            return Err(SessionError::Validation(format!(
                "script resource owner/path mismatch or duplicate: {:?}",
                record.path
            )));
        }
        if record.uncompressed_bytes > resource_limits.max_file_bytes {
            return Err(SessionError::ResourceLimit(format!(
                "script resource {:?} exceeds per-file limit {}",
                record.path, resource_limits.max_file_bytes
            )));
        }
        aggregate = aggregate.saturating_add(record.uncompressed_bytes);
        if aggregate > resource_limits.max_aggregate_bytes
            || aggregate > limits.max_uncompressed_bytes
        {
            return Err(SessionError::ResourceLimit(
                "script resources exceed the aggregate limit".to_owned(),
            ));
        }
        let bytes = read_entry(archive, &record.path, record.uncompressed_bytes)?;
        if bytes.len() as u64 != record.uncompressed_bytes || sha256(&bytes) != record.sha256 {
            return Err(SessionError::HashMismatch { id: record.path });
        }
        if grouped
            .entry(record.owner_script_id)
            .or_default()
            .insert(
                relative,
                ScriptResource::new(record.kind, Arc::<[u8]>::from(bytes)),
            )
            .is_some()
        {
            return Err(SessionError::Validation(
                "duplicate normalized script resource path".to_owned(),
            ));
        }
    }
    let mut bundles = BTreeMap::new();
    for (script_id, entrypoint) in script_entrypoints {
        let resources = grouped.remove(&script_id).ok_or_else(|| {
            SessionError::Validation(format!("script {script_id} has no resource records"))
        })?;
        let bundle = ScriptResourceBundle::new(entrypoint, resources, resource_limits)
            .map_err(|error| SessionError::Validation(format!("script {script_id}: {error}")))?;
        bundles.insert(script_id, Arc::new(bundle));
    }
    Ok(bundles)
}

fn encode_zip<T: Serialize>(
    manifest: &T,
    payloads: BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, SessionError> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    writer.start_file(MANIFEST_PATH, options)?;
    let manifest_bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| SessionError::Manifest(error.to_string()))?;
    writer.write_all(&manifest_bytes)?;
    for (path, payload) in payloads {
        writer.start_file(path, options)?;
        writer.write_all(&payload)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn inspect_central_directory(bytes: &[u8], limits: DecodeLimits) -> Result<(), SessionError> {
    const EOCD: &[u8; 4] = b"PK\x05\x06";
    const ZIP64_LOCATOR: &[u8; 4] = b"PK\x06\x07";
    const ZIP64_EOCD: &[u8; 4] = b"PK\x06\x06";
    const CENTRAL_FILE: &[u8; 4] = b"PK\x01\x02";

    let eocd = bytes
        .windows(EOCD.len())
        .rposition(|window| window == EOCD)
        .ok_or_else(|| {
            SessionError::Archive("end-of-central-directory record is missing".to_owned())
        })?;
    if eocd + 22 > bytes.len() {
        return Err(SessionError::Archive(
            "end-of-central-directory record is truncated".to_owned(),
        ));
    }
    let read_u16 = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    let read_u32 = |offset: usize| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    let read_u64 = |offset: usize| {
        u64::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ])
    };
    let mut entries = read_u16(eocd + 10) as u64;
    let mut central_offset = read_u32(eocd + 16) as u64;
    if entries == u16::MAX as u64 || central_offset == u32::MAX as u64 {
        if eocd < 20 || &bytes[eocd - 20..eocd - 16] != ZIP64_LOCATOR {
            return Err(SessionError::Archive(
                "ZIP64 central-directory locator is missing".to_owned(),
            ));
        }
        let zip64_offset = read_u64(eocd - 12) as usize;
        if zip64_offset + 56 > bytes.len() || &bytes[zip64_offset..zip64_offset + 4] != ZIP64_EOCD {
            return Err(SessionError::Archive(
                "ZIP64 end-of-central-directory record is invalid".to_owned(),
            ));
        }
        entries = read_u64(zip64_offset + 32);
        central_offset = read_u64(zip64_offset + 48);
    }
    if entries > limits.max_entries as u64 {
        return Err(SessionError::ResourceLimit(format!(
            "{entries} entries exceeds {}",
            limits.max_entries
        )));
    }
    let mut cursor = usize::try_from(central_offset)
        .map_err(|_| SessionError::Archive("central-directory offset is invalid".to_owned()))?;
    let mut names = BTreeSet::new();
    for _ in 0..entries {
        if cursor + 46 > bytes.len() || &bytes[cursor..cursor + 4] != CENTRAL_FILE {
            return Err(SessionError::Archive(
                "central-directory entry is invalid".to_owned(),
            ));
        }
        let name_len = read_u16(cursor + 28) as usize;
        let extra_len = read_u16(cursor + 30) as usize;
        let comment_len = read_u16(cursor + 32) as usize;
        let name_start = cursor + 46;
        let name_end = name_start
            .checked_add(name_len)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| {
                SessionError::Archive("central-directory filename is truncated".to_owned())
            })?;
        if !names.insert(bytes[name_start..name_end].to_vec()) {
            return Err(SessionError::Archive(
                "duplicate archive path in central directory".to_owned(),
            ));
        }
        cursor = name_end
            .checked_add(extra_len)
            .and_then(|next| next.checked_add(comment_len))
            .filter(|next| *next <= bytes.len())
            .ok_or_else(|| {
                SessionError::Archive("central-directory entry is truncated".to_owned())
            })?;
    }
    Ok(())
}

fn inspect_archive(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    limits: DecodeLimits,
) -> Result<BTreeSet<String>, SessionError> {
    if archive.len() > limits.max_entries {
        return Err(SessionError::ResourceLimit(format!(
            "{} entries exceeds {}",
            archive.len(),
            limits.max_entries
        )));
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        let name = file.name().to_owned();
        if file.enclosed_name().is_none() || name.starts_with('/') || name.contains('\\') {
            return Err(SessionError::Archive(format!(
                "unsafe archive path {name:?}"
            )));
        }
        if !names.insert(name.clone()) {
            return Err(SessionError::Archive(format!(
                "duplicate archive path {name:?}"
            )));
        }
        total = total
            .checked_add(file.size())
            .ok_or_else(|| SessionError::ResourceLimit("size overflow".to_owned()))?;
        if total > limits.max_uncompressed_bytes {
            return Err(SessionError::ResourceLimit(format!(
                "declared uncompressed size {total} exceeds {}",
                limits.max_uncompressed_bytes
            )));
        }
    }
    Ok(names)
}

fn read_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, SessionError> {
    let entry = archive
        .by_name(path)
        .map_err(|_| SessionError::MissingMedia {
            id: path.to_owned(),
        })?;
    if entry.size() > max_bytes {
        return Err(SessionError::ResourceLimit(format!(
            "entry {path:?} exceeds its declared/resource size"
        )));
    }
    let capacity = usize::try_from(entry.size())
        .map_err(|_| SessionError::ResourceLimit("entry does not fit memory".to_owned()))?;
    let mut bytes = Vec::with_capacity(capacity);
    entry
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(SessionError::ResourceLimit(format!(
            "entry {path:?} expanded beyond its limit"
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
fn check_version(
    format: &str,
    version: FormatVersion,
    document_version: u16,
    expected_document_version: u16,
) -> Result<(), SessionError> {
    if version.major != FORMAT_MAJOR
        || version.minor > FORMAT_MINOR
        || document_version != expected_document_version
    {
        return Err(SessionError::UnsupportedVersion {
            format: format.to_owned(),
            major: version.major,
            minor: version.minor,
        });
    }
    Ok(())
}

pub fn validate_bundle(bundle: &SessionBundle) -> Result<(), SessionError> {
    if bundle.document.sample_rate == 0 {
        return Err(SessionError::Validation(
            "sample rate must be non-zero".to_owned(),
        ));
    }
    if bundle.document.connection_model_version != crate::CONNECTION_MODEL_VERSION {
        return Err(SessionError::Validation(
            "unsupported connection model version".to_owned(),
        ));
    }
    let mut script_ids = BTreeSet::new();
    for script in &bundle.document.scripts {
        require_id(script.id, "script")?;
        if !script_ids.insert(script.id) {
            return Err(SessionError::Validation(format!(
                "duplicate script ID {}",
                script.id
            )));
        }
        if script.name.trim().is_empty() {
            return Err(SessionError::Validation(format!(
                "script {} has an empty name",
                script.id
            )));
        }
        let entrypoint = NormalizedRelativePath::parse(&script.entrypoint).map_err(|error| {
            SessionError::Validation(format!(
                "script {} entrypoint is invalid: {error}",
                script.id
            ))
        })?;
        let resources = bundle.scripts.get(&script.id).ok_or_else(|| {
            SessionError::Validation(format!("script {} resource bundle is missing", script.id))
        })?;
        if resources.entrypoint != entrypoint {
            return Err(SessionError::Validation(format!(
                "script {} entrypoint does not match its resource bundle",
                script.id
            )));
        }
    }
    if bundle.scripts.keys().copied().collect::<BTreeSet<_>>() != script_ids {
        return Err(SessionError::Validation(
            "script resource owners do not match the session document".to_owned(),
        ));
    }
    let mut track_ids = BTreeSet::new();
    let mut loop_ids = BTreeSet::new();
    let mut loop_lengths = BTreeMap::new();
    let mut sync_length = 1_u64;
    let mut port_ids = BTreeSet::new();
    let mut channel_ids = BTreeSet::new();
    let mut fx_chain_ids = BTreeSet::new();
    let mut fx_state_types = BTreeMap::new();
    for state in &bundle.document.fx_states {
        require_id(state.id, "FX state")?;
        if fx_state_types.insert(state.id, state.chain_type).is_some() {
            return Err(SessionError::Validation(format!(
                "duplicate FX state ID {}",
                state.id
            )));
        }
    }
    for group in &bundle.document.track_groups {
        for track in &group.tracks {
            require_id(track.id, "track")?;
            if !track_ids.insert(track.id) {
                return Err(SessionError::Validation(format!(
                    "duplicate track ID {}",
                    track.id
                )));
            }
            validate_finite(track.controls.output_gain_db, "track output gain")?;
            validate_finite(track.controls.output_balance, "track output balance")?;
            validate_finite(track.controls.input_gain_db, "track input gain")?;
            validate_finite(track.controls.input_balance, "track input balance")?;
            if let Some(chain) = &track.fx_chain {
                require_id(chain.id, "FX chain")?;
                if !fx_chain_ids.insert(chain.id) {
                    return Err(SessionError::Validation(format!(
                        "duplicate FX chain ID {}",
                        chain.id
                    )));
                }
                for port in &chain.ports {
                    require_id(port.id, "port")?;
                    if !port_ids.insert(port.id) {
                        return Err(SessionError::Validation(format!(
                            "duplicate port ID {}",
                            port.id
                        )));
                    }
                    validate_finite(port.gain, "port gain")?;
                }
            }
            validate_track_fx_shape(track)?;
            validate_track_latency_policy(&track.latency_policy)?;
            for port in &track.ports {
                require_id(port.id, "port")?;
                if !port_ids.insert(port.id) {
                    return Err(SessionError::Validation(format!(
                        "duplicate port ID {}",
                        port.id
                    )));
                }
                validate_finite(port.gain, "port gain")?;
            }
            validate_track_cue_output(track)?;
            for loop_ in &track.loops {
                require_id(loop_.id, "loop")?;
                if !loop_ids.insert(loop_.id) {
                    return Err(SessionError::Validation(format!(
                        "duplicate loop ID {}",
                        loop_.id
                    )));
                }
                loop_lengths.insert(loop_.id, loop_.length_frames);
                if loop_.is_sync {
                    sync_length = loop_.length_frames.max(1);
                }
                validate_finite(loop_.gain, "loop gain")?;
                validate_finite(loop_.balance, "loop balance")?;
                if loop_.composite.is_some() && !loop_.channels.is_empty() {
                    return Err(SessionError::Validation(format!(
                        "loop {} has primitive channels and a composite",
                        loop_.id
                    )));
                }
                if loop_.composite.is_none() {
                    validate_track_channel_shape(track, loop_.id, &loop_.channels)?;
                }
                for channel in &loop_.channels {
                    require_id(channel.id, "channel")?;
                    if !channel_ids.insert(channel.id) {
                        return Err(SessionError::Validation(format!(
                            "duplicate channel ID {}",
                            channel.id
                        )));
                    }
                    validate_finite(channel.gain, "channel gain")?;
                    validate_take_latency(
                        &channel.latency,
                        channel.data_length_frames,
                        bundle.document.sample_rate,
                    )?;
                    if let Some(state_id) = channel.recording_fx_state_id {
                        let state_type = fx_state_types.get(&state_id).ok_or_else(|| {
                            SessionError::Validation(format!(
                                "channel {} references missing FX state {}",
                                channel.id, state_id
                            ))
                        })?;
                        let chain_type = track.fx_chain.as_ref().map(|chain| chain.chain_type);
                        if chain_type != Some(*state_type) {
                            return Err(SessionError::Validation(format!(
                                "channel {} FX state type does not match its track",
                                channel.id
                            )));
                        }
                    }
                    if let Some(media_id) = &channel.media_id {
                        let payload = bundle.media.get(media_id).ok_or_else(|| {
                            SessionError::MissingMedia {
                                id: media_id.clone(),
                            }
                        })?;
                        match (channel.data_type, payload) {
                            (DataTypeDocument::Audio, MediaPayload::Audio(audio)) => {
                                if audio.samples.len() as u64 != channel.data_length_frames {
                                    return Err(SessionError::InvalidMediaShape {
                                        id: media_id.clone(),
                                    });
                                }
                            }
                            (DataTypeDocument::Midi, MediaPayload::Midi(midi)) => {
                                if midi.sample_rate != bundle.document.sample_rate
                                    || midi.length_frames != channel.data_length_frames
                                {
                                    return Err(SessionError::InvalidMediaShape {
                                        id: media_id.clone(),
                                    });
                                }
                                validate_midi(media_id, midi)?;
                            }
                            _ => {
                                return Err(SessionError::InvalidMediaShape {
                                    id: media_id.clone(),
                                });
                            }
                        }
                    } else if channel.data_length_frames != 0 {
                        return Err(SessionError::Validation(format!(
                            "non-empty channel {} has no media",
                            channel.id
                        )));
                    }
                }
            }
        }
    }
    for selected in &bundle.document.selected_loop_ids {
        if !loop_ids.contains(selected) {
            return Err(SessionError::Validation(format!(
                "selected loop ID {selected} is stale"
            )));
        }
    }
    if bundle
        .document
        .targeted_loop_id
        .is_some_and(|target| !loop_ids.contains(&target))
    {
        return Err(SessionError::Validation(
            "targeted loop ID is stale".to_owned(),
        ));
    }
    for group in &bundle.document.track_groups {
        for track in &group.tracks {
            for loop_ in &track.loops {
                for channel in &loop_.channels {
                    if channel
                        .connected_port_ids
                        .iter()
                        .any(|id| !port_ids.contains(id))
                    {
                        return Err(SessionError::Validation(format!(
                            "channel {} references a stale port",
                            channel.id
                        )));
                    }
                }
                if let Some(composite) = &loop_.composite {
                    let mut instance_ids = BTreeSet::new();
                    for event in &composite.instances {
                        if event.instance_id == 0 || !instance_ids.insert(event.instance_id) {
                            return Err(SessionError::Validation(format!(
                                "composite loop {} has invalid or duplicate instance ID {}",
                                loop_.id, event.instance_id
                            )));
                        }
                        if event.n_cycles == Some(0) {
                            return Err(SessionError::Validation(format!(
                                "composite loop {} has a zero-length instance",
                                loop_.id
                            )));
                        }
                        if event.start_cycle > u64::from(u32::MAX) {
                            return Err(SessionError::Validation(format!(
                                "composite loop {} has an out-of-range start cycle",
                                loop_.id
                            )));
                        }
                        if event.mode.as_deref().is_some_and(|mode| {
                            !matches!(
                                mode,
                                "stopped"
                                    | "playing"
                                    | "recording"
                                    | "replacing"
                                    | "playing_dry_through_wet"
                                    | "recording_dry_into_wet"
                            )
                        }) {
                            return Err(SessionError::Validation(format!(
                                "composite loop {} has an unsupported instance mode",
                                loop_.id
                            )));
                        }
                        if composite.kind == CompositeKindDocument::Regular && event.mode.is_some()
                        {
                            return Err(SessionError::Validation(format!(
                                "regular composite loop {} has an explicit instance mode",
                                loop_.id
                            )));
                        }
                        if !loop_ids.contains(&event.loop_id) {
                            return Err(SessionError::Validation(format!(
                                "composite loop {} references stale loop {}",
                                loop_.id, event.loop_id
                            )));
                        }
                        let duration = event.n_cycles.map(u64::from).unwrap_or_else(|| {
                            loop_lengths[&event.loop_id].div_ceil(sync_length).max(1)
                        });
                        if event
                            .start_cycle
                            .checked_add(duration)
                            .is_none_or(|end| end > u64::from(u32::MAX))
                        {
                            return Err(SessionError::Validation(format!(
                                "composite loop {} has an out-of-range instance end cycle",
                                loop_.id
                            )));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_track_fx_shape(track: &TrackDocument) -> Result<(), SessionError> {
    if let Some(chain) = &track.fx_chain {
        if chain.chain_type != FxChainTypeDocument::OxiSynth
            && !chain.midi_cc_assignments.is_empty()
        {
            return Err(SessionError::Validation(format!(
                "non-OxiSynth FX chain {} contains MIDI CC assignments",
                chain.id
            )));
        }
        let mut parameters = BTreeSet::new();
        let mut sources = BTreeSet::new();
        for assignment in &chain.midi_cc_assignments {
            if assignment.channel > 15
                || assignment.controller > 127
                || !parameters.insert(assignment.parameter)
                || !sources.insert((assignment.channel, assignment.controller))
            {
                return Err(SessionError::Validation(format!(
                    "FX chain {} contains invalid or duplicate MIDI CC assignments",
                    chain.id
                )));
            }
        }
    }
    match (&track.topology, &track.fx_chain) {
        (TrackTopologyDocument::DryWetExternal { .. }, Some(_)) => {
            Err(SessionError::Validation(format!(
                "external dry/wet track {} must not contain an FX chain",
                track.id
            )))
        }
        (TrackTopologyDocument::Carla { chain_type, .. }, Some(chain))
            if *chain_type != chain.chain_type =>
        {
            Err(SessionError::Validation(format!(
                "Carla track {} chain type does not match its topology",
                track.id
            )))
        }
        (TrackTopologyDocument::Carla { .. }, None) => Err(SessionError::Validation(format!(
            "Carla track {} is missing its FX chain",
            track.id
        ))),
        (TrackTopologyDocument::OxiSynth, Some(chain))
            if chain.chain_type != FxChainTypeDocument::OxiSynth
                || chain.internal_state.is_empty() =>
        {
            Err(SessionError::Validation(format!(
                "OxiSynth track {} contains mismatched or invalid processor state",
                track.id
            )))
        }
        (TrackTopologyDocument::OxiSynth, None) => Err(SessionError::Validation(format!(
            "OxiSynth track {} is missing its FX chain",
            track.id
        ))),
        (TrackTopologyDocument::Trigger, Some(_)) => Err(SessionError::Validation(format!(
            "trigger track {} must not contain an FX chain",
            track.id
        ))),
        _ => Ok(()),
    }
}

fn validate_track_channel_shape(
    track: &TrackDocument,
    loop_id: u64,
    channels: &[crate::document::ChannelDocument],
) -> Result<(), SessionError> {
    let count = |mode: ChannelModeDocument, data_type: DataTypeDocument| {
        channels
            .iter()
            .filter(|channel| channel.mode == mode && channel.data_type == data_type)
            .count() as u32
    };
    let disabled = channels
        .iter()
        .any(|channel| channel.mode == ChannelModeDocument::Disabled);
    let valid = match track.topology {
        TrackTopologyDocument::Direct {
            audio_channels,
            midi,
        } => {
            count(ChannelModeDocument::Direct, DataTypeDocument::Audio) == audio_channels
                && count(ChannelModeDocument::Direct, DataTypeDocument::Midi) == u32::from(midi)
                && channels
                    .iter()
                    .all(|channel| matches!(channel.mode, ChannelModeDocument::Direct))
        }
        TrackTopologyDocument::DryWetExternal {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } => {
            count(ChannelModeDocument::Dry, DataTypeDocument::Audio) == dry_audio_channels
                && count(ChannelModeDocument::Wet, DataTypeDocument::Audio) == wet_audio_channels
                && count(ChannelModeDocument::Dry, DataTypeDocument::Midi) == u32::from(dry_midi)
                && channels.iter().all(|channel| {
                    matches!(
                        channel.mode,
                        ChannelModeDocument::Dry | ChannelModeDocument::Wet
                    ) && !(channel.mode == ChannelModeDocument::Wet
                        && channel.data_type == DataTypeDocument::Midi)
                })
        }
        TrackTopologyDocument::Carla {
            audio_channels,
            midi,
            dry_audio_channels,
            wet_audio_channels,
            ..
        } => {
            let dry_audio_channels = dry_audio_channels.unwrap_or(audio_channels);
            let wet_audio_channels = wet_audio_channels.unwrap_or(audio_channels);
            count(ChannelModeDocument::Dry, DataTypeDocument::Audio) == dry_audio_channels
                && count(ChannelModeDocument::Wet, DataTypeDocument::Audio) == wet_audio_channels
                && count(ChannelModeDocument::Dry, DataTypeDocument::Midi) == u32::from(midi)
                && channels.iter().all(|channel| {
                    matches!(
                        channel.mode,
                        ChannelModeDocument::Dry | ChannelModeDocument::Wet
                    ) && !(channel.mode == ChannelModeDocument::Wet
                        && channel.data_type == DataTypeDocument::Midi)
                })
        }
        TrackTopologyDocument::OxiSynth => {
            count(ChannelModeDocument::Dry, DataTypeDocument::Audio) == 2
                && count(ChannelModeDocument::Wet, DataTypeDocument::Audio) == 2
                && count(ChannelModeDocument::Dry, DataTypeDocument::Midi) == 1
                && channels.iter().all(|channel| {
                    matches!(
                        channel.mode,
                        ChannelModeDocument::Dry | ChannelModeDocument::Wet
                    ) && !(channel.mode == ChannelModeDocument::Wet
                        && channel.data_type == DataTypeDocument::Midi)
                })
        }
        TrackTopologyDocument::Trigger => channels.is_empty(),
    };
    if valid && !disabled {
        Ok(())
    } else {
        Err(SessionError::Validation(format!(
            "loop {loop_id} channel shape does not match track {} topology",
            track.id
        )))
    }
}

fn validate_track_cue_output(track: &TrackDocument) -> Result<(), SessionError> {
    let Some(selection) = &track.latency_policy.cue_output else {
        return Ok(());
    };
    let ports = track
        .ports
        .iter()
        .chain(track.fx_chain.iter().flat_map(|chain| &chain.ports));
    let valid = match selection {
        CueOutputSelectionDocument::ApplicationPort { port_id } => ports
            .clone()
            .any(|port| port.id == *port_id && port.direction == PortDirectionDocument::Output),
        CueOutputSelectionDocument::HostPort { host_port_id } => {
            !host_port_id.is_empty()
                && host_port_id.len() <= shoop_latency::MAX_SOURCE_IDENTITY_BYTES
                && ports.clone().any(|port| {
                    port.direction == PortDirectionDocument::Output
                        && port
                            .external_connections
                            .iter()
                            .any(|candidate| candidate == host_port_id)
                })
        }
    };
    if !valid {
        return Err(SessionError::Validation(
            "track cue output does not identify a connected output path".to_owned(),
        ));
    }
    Ok(())
}

fn validate_track_latency_policy(policy: &TrackLatencyPolicyDocument) -> Result<(), SessionError> {
    if policy.components.len() > shoop_latency::MAX_RECIPE_COMPONENTS {
        return Err(SessionError::Validation(
            "track latency policy exceeds component capacity".to_owned(),
        ));
    }
    let mut kinds = BTreeSet::new();
    for component in &policy.components {
        if !kinds.insert(component.component) {
            return Err(SessionError::Validation(
                "track latency policy repeats a component".to_owned(),
            ));
        }
        match component.value {
            crate::document::LatencyValueDocument::Manual { frames }
                if frames > u64::from(shoop_latency::MAX_COMPENSATION_FRAMES) =>
            {
                return Err(SessionError::Validation(
                    "manual latency exceeds the supported bound".to_owned(),
                ));
            }
            crate::document::LatencyValueDocument::AutomaticPlusTrim { frames }
                if frames.unsigned_abs() > u64::from(shoop_latency::MAX_COMPENSATION_FRAMES) =>
            {
                return Err(SessionError::Validation(
                    "latency trim exceeds the supported bound".to_owned(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn validate_take_latency(
    latency: &TakeLatencyDocument,
    raw_length: u64,
    session_sample_rate: u32,
) -> Result<(), SessionError> {
    if latency.capture_alignment_frames.unsigned_abs()
        > u64::from(shoop_latency::MAX_COMPENSATION_FRAMES)
    {
        return Err(SessionError::Validation(
            "take latency alignment exceeds the supported bound".to_owned(),
        ));
    }
    if latency.retained_before_frames > u64::from(shoop_latency::MAX_RETAINED_MARGIN_FRAMES)
        || latency.retained_after_frames > u64::from(shoop_latency::MAX_RETAINED_MARGIN_FRAMES)
    {
        return Err(SessionError::Validation(
            "take latency retained margin exceeds the supported bound".to_owned(),
        ));
    }
    let observation = &latency.observation;
    match (
        observation.minimum_frames,
        observation.maximum_frames,
        observation.certainty,
    ) {
        (None, None, LatencyCertaintyDocument::Unknown | LatencyCertaintyDocument::ManualOnly) => {}
        (Some(minimum), Some(maximum), certainty)
            if minimum <= maximum
                && maximum <= u64::from(shoop_latency::MAX_COMPENSATION_FRAMES)
                && observation.sample_rate == session_sample_rate
                && match certainty {
                    LatencyCertaintyDocument::Exact => minimum == maximum,
                    LatencyCertaintyDocument::Range => minimum < maximum,
                    LatencyCertaintyDocument::Estimated => true,
                    LatencyCertaintyDocument::ManualOnly | LatencyCertaintyDocument::Unknown => {
                        false
                    }
                } => {}
        _ => {
            return Err(SessionError::Validation(
                "take latency observation is inconsistent".to_owned(),
            ));
        }
    }
    if latency.variable_history && latency.history_revisions < 2 {
        return Err(SessionError::Validation(
            "variable take latency has fewer than two revisions".to_owned(),
        ));
    }
    if latency.alignment_regions.len() > shoop_latency::MAX_OBSERVATION_HISTORY {
        return Err(SessionError::Validation(
            "take latency exceeds alignment-region capacity".to_owned(),
        ));
    }
    let mut previous_end = 0;
    for region in &latency.alignment_regions {
        if region.raw_start_frame >= region.raw_end_frame
            || region.raw_start_frame < previous_end
            || region.raw_end_frame > raw_length
            || region.capture_alignment_frames.unsigned_abs()
                > u64::from(shoop_latency::MAX_COMPENSATION_FRAMES)
        {
            return Err(SessionError::Validation(
                "take latency alignment region is invalid".to_owned(),
            ));
        }
        previous_end = region.raw_end_frame;
    }
    Ok(())
}

fn validate_midi(id: &str, midi: &crate::document::ExactMidi) -> Result<(), SessionError> {
    let mut previous = None;
    for event in &midi.events {
        if midi.length_frames == 0 || event.frame >= midi.length_frames {
            return Err(SessionError::InvalidMediaShape { id: id.to_owned() });
        }
        if previous.is_some_and(|key| key > (event.frame, event.order)) {
            return Err(SessionError::Validation(format!(
                "MIDI payload {id} is not ordered"
            )));
        }
        previous = Some((event.frame, event.order));
    }
    Ok(())
}

fn require_id(id: u64, kind: &str) -> Result<(), SessionError> {
    if id == 0 {
        Err(SessionError::Validation(format!(
            "{kind} ID must be non-zero"
        )))
    } else {
        Ok(())
    }
}

fn validate_finite(value: f32, field: &str) -> Result<(), SessionError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(SessionError::Validation(format!("{field} is not finite")))
    }
}

fn validate_media_id(id: &str) -> Result<(), SessionError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(SessionError::Validation(format!("invalid media ID {id:?}")));
    }
    Ok(())
}

fn encode_audio_samples(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len().saturating_mul(4));
    for sample in samples {
        bytes.extend_from_slice(&sample.to_bits().to_le_bytes());
    }
    bytes
}

fn decode_audio_samples(id: &str, bytes: &[u8]) -> Result<Vec<f32>, SessionError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(SessionError::InvalidMediaShape { id: id.to_owned() });
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_bits(u32::from_le_bytes(chunk.try_into().expect("four bytes"))))
        .collect())
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

pub(crate) fn encode_standalone_manifest<T: Serialize>(
    manifest: &T,
    payloads: BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, SessionError> {
    encode_zip(manifest, payloads)
}

pub(crate) fn check_standalone_version(
    format: &str,
    expected: &str,
    version: FormatVersion,
    document_version: u16,
) -> Result<(), SessionError> {
    if format != expected {
        return Err(SessionError::UnsupportedFormat);
    }
    if version != FormatVersion::default() || !(1..=DOCUMENT_VERSION).contains(&document_version) {
        return Err(SessionError::UnsupportedVersion {
            format: format.to_owned(),
            major: version.major,
            minor: version.minor,
        });
    }
    Ok(())
}

pub(crate) fn inspect_standalone_archive(
    bytes: &[u8],
) -> Result<ZipArchive<Cursor<&[u8]>>, SessionError> {
    if !bytes.starts_with(b"PK") {
        return Err(SessionError::UnsupportedFormat);
    }
    inspect_central_directory(bytes, DecodeLimits::default())?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    inspect_archive(&mut archive, DecodeLimits::default())?;
    Ok(archive)
}

pub(crate) fn read_standalone_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, SessionError> {
    read_entry(archive, path, max_bytes)
}

pub(crate) fn payload_hash(bytes: &[u8]) -> String {
    sha256(bytes)
}

pub(crate) fn audio_to_bytes(samples: &[f32]) -> Vec<u8> {
    encode_audio_samples(samples)
}

pub(crate) fn audio_from_bytes(id: &str, bytes: &[u8]) -> Result<Vec<f32>, SessionError> {
    decode_audio_samples(id, bytes)
}

pub(crate) const SESSION_MANIFEST_PATH: &str = MANIFEST_PATH;
pub(crate) const STANDALONE_AUDIO_FORMAT: &str = AUDIO_FORMAT;
pub(crate) const STANDALONE_MIDI_FORMAT: &str = MIDI_FORMAT;

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn version_dispatch_accepts_current_and_rejects_other_major_or_minor() {
        assert!(check_version(
            SESSION_FORMAT,
            FormatVersion {
                major: FORMAT_MAJOR,
                minor: FORMAT_MINOR,
            },
            SESSION_DOCUMENT_VERSION,
            SESSION_DOCUMENT_VERSION,
        )
        .is_ok());
        for version in [
            FormatVersion { major: 0, minor: 0 },
            FormatVersion { major: 2, minor: 0 },
            FormatVersion {
                major: FORMAT_MAJOR,
                minor: FORMAT_MINOR + 1,
            },
        ] {
            assert!(matches!(
                check_version(
                    SESSION_FORMAT,
                    version,
                    SESSION_DOCUMENT_VERSION,
                    SESSION_DOCUMENT_VERSION,
                ),
                Err(SessionError::UnsupportedVersion { .. })
            ));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn unsafe_archive_paths_are_rejected() {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        writer
            .start_file(
                "../manifest.json",
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(b"{}").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        assert!(matches!(
            inspect_archive(&mut archive, DecodeLimits::default()),
            Err(SessionError::Archive(_))
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn duplicate_archive_paths_are_rejected() {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("manifest1.json", options).unwrap();
        writer.write_all(b"first").unwrap();
        writer.start_file("manifest2.json", options).unwrap();
        writer.write_all(b"second").unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        for source in [b"manifest1.json", b"manifest2.json"] {
            let mut offset = 0;
            while let Some(index) = bytes[offset..]
                .windows(source.len())
                .position(|window| window == source)
            {
                let start = offset + index;
                bytes[start..start + source.len()].copy_from_slice(b"manifest0.json");
                offset = start + source.len();
            }
        }
        assert!(matches!(
            decode_session(&bytes),
            Err(SessionError::Archive(message)) if message.contains("duplicate archive path")
        ));
    }
}
