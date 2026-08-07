use crate::document::{
    AudioPayload, DataTypeDocument, FormatVersion, MediaPayload, SessionBundle, SessionDocument,
    AUDIO_FORMAT, DOCUMENT_VERSION, FORMAT_MAJOR, FORMAT_MINOR, MIDI_FORMAT, SESSION_FORMAT,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionManifest {
    format: String,
    format_version: FormatVersion,
    document_version: u16,
    writer_app_version: String,
    sample_rate: u32,
    document: SessionDocument,
    media: Vec<MediaRecord>,
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
    let manifest = SessionManifest {
        format: SESSION_FORMAT.to_owned(),
        format_version: FormatVersion::default(),
        document_version: DOCUMENT_VERSION,
        writer_app_version: writer_app_version.to_owned(),
        sample_rate: bundle.document.sample_rate,
        document: bundle.document.clone(),
        media: records,
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
    inspect_archive(&mut archive, limits)?;
    let manifest_bytes = read_entry(&mut archive, MANIFEST_PATH, limits.max_uncompressed_bytes)?;
    let manifest: SessionManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| SessionError::Manifest(error.to_string()))?;
    check_version(
        &manifest.format,
        manifest.format_version,
        manifest.document_version,
    )?;
    if manifest.format != SESSION_FORMAT {
        return Err(SessionError::UnsupportedFormat);
    }
    if manifest.sample_rate != manifest.document.sample_rate {
        return Err(SessionError::Validation(
            "manifest and document sample rates differ".to_owned(),
        ));
    }
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
    };
    validate_bundle(&bundle)?;
    Ok(bundle)
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
) -> Result<(), SessionError> {
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
    Ok(())
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

fn check_version(
    format: &str,
    version: FormatVersion,
    document_version: u16,
) -> Result<(), SessionError> {
    if version.major != FORMAT_MAJOR
        || version.minor > FORMAT_MINOR
        || document_version != DOCUMENT_VERSION
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
    let mut track_ids = BTreeSet::new();
    let mut loop_ids = BTreeSet::new();
    let mut port_ids = BTreeSet::new();
    let mut channel_ids = BTreeSet::new();
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
            for loop_ in &track.loops {
                require_id(loop_.id, "loop")?;
                if !loop_ids.insert(loop_.id) {
                    return Err(SessionError::Validation(format!(
                        "duplicate loop ID {}",
                        loop_.id
                    )));
                }
                validate_finite(loop_.gain, "loop gain")?;
                validate_finite(loop_.balance, "loop balance")?;
                if loop_.composite.is_some() && !loop_.channels.is_empty() {
                    return Err(SessionError::Validation(format!(
                        "loop {} has primitive channels and a composite",
                        loop_.id
                    )));
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
                    for event in composite.playlists.iter().flatten().flatten() {
                        if !loop_ids.contains(&event.loop_id) {
                            return Err(SessionError::Validation(format!(
                                "composite loop {} references stale loop {}",
                                loop_.id, event.loop_id
                            )));
                        }
                    }
                }
            }
        }
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
    check_version(format, version, document_version)
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

    #[test]
    fn version_dispatch_accepts_current_and_rejects_other_major_or_minor() {
        assert!(check_version(
            SESSION_FORMAT,
            FormatVersion {
                major: FORMAT_MAJOR,
                minor: FORMAT_MINOR,
            },
            DOCUMENT_VERSION,
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
                check_version(SESSION_FORMAT, version, DOCUMENT_VERSION),
                Err(SessionError::UnsupportedVersion { .. })
            ));
        }
    }

    #[test]
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

    #[test]
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
