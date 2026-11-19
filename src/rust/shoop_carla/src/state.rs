//! Carla Native state envelopes shared by native hosts.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use quick_xml::{events::Event, Reader};
use serde::Deserialize;
use shoop_plugin_protocol::{CarlaChainType, MAX_STATE_BYTES};

const STATE_PREFIX_V1: &str = "shoop-carla-native-state:1:";
const STATE_PREFIX_V2: &str = "shoop-carla-native-state:2:";
const LEGACY_CHUNK_URI: &str = "http://kxstudio.sf.net/ns/carla/chunk";
const LEGACY_ATOM_STRING_URI: &str = "http://lv2plug.in/ns/ext/atom#String";
const MAX_PROJECT_DEPTH: usize = 128;
const MAX_PROJECT_PLUGINS: usize = 512;

fn state_chain_label(chain_type: CarlaChainType) -> &'static str {
    match chain_type {
        CarlaChainType::Rack => "rack",
        CarlaChainType::Patchbay => "patchbay",
        CarlaChainType::Patchbay16x => "patchbay16",
    }
}

pub fn encode_state(chain_type: CarlaChainType, bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_STATE_BYTES {
        bail!("Carla state exceeds {MAX_STATE_BYTES} bytes");
    }
    if bytes.contains(&0) {
        bail!("Carla state contains an interior NUL byte");
    }
    Ok(format!(
        "{STATE_PREFIX_V2}{}:{}",
        state_chain_label(chain_type),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn decode_native_state(encoded: &str) -> Result<Vec<u8>> {
    decode_payload(encoded, MAX_STATE_BYTES).context("invalid Carla Native state base64")
}

fn decode_payload(encoded: &str, max_bytes: usize) -> Result<Vec<u8>> {
    if encoded.len() > max_bytes.div_ceil(3) * 4 {
        bail!("Carla state exceeds {MAX_STATE_BYTES} bytes");
    }
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(Into::into)
}

pub fn decode_state(state: &str, expected_chain_type: CarlaChainType) -> Result<Vec<u8>> {
    let decoded = if let Some(tagged) = state.strip_prefix(STATE_PREFIX_V2) {
        let (chain, encoded) = tagged
            .split_once(':')
            .ok_or_else(|| anyhow!("Carla Native state has no chain tag"))?;
        let expected = state_chain_label(expected_chain_type);
        if chain != expected {
            bail!("Carla Native state is for {chain}, not {expected}");
        }
        decode_native_state(encoded)?
    } else if let Some(encoded) = state.strip_prefix(STATE_PREFIX_V1) {
        // Version 1 was emitted briefly during direct-host development before
        // states carried a chain tag. Keep it readable, but only v2 is written.
        decode_native_state(encoded)?
    } else {
        let value: serde_json::Value =
            serde_json::from_str(state).context("unsupported Carla state format")?;
        let entry = value
            .as_object()
            .and_then(|object| object.get(LEGACY_CHUNK_URI))
            .ok_or_else(|| anyhow!("legacy Carla state has no chunk"))?;
        if entry.get("type").and_then(|value| value.as_str()) != Some(LEGACY_ATOM_STRING_URI) {
            bail!("legacy Carla state chunk has the wrong type");
        }
        let encoded = entry
            .get("value")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("legacy Carla state chunk has no value"))?;
        let mut bytes = decode_payload(encoded, MAX_STATE_BYTES + 1)
            .context("invalid legacy Carla state base64")?;
        if bytes.last() != Some(&0) {
            bail!("legacy Carla state chunk has no trailing NUL byte");
        }
        bytes.pop();
        bytes
    };
    if decoded.len() > MAX_STATE_BYTES {
        bail!("Carla state exceeds {MAX_STATE_BYTES} bytes");
    }
    if decoded.contains(&0) {
        bail!("Carla state contains an interior NUL byte");
    }
    Ok(decoded)
}

#[derive(Deserialize)]
struct Project {
    #[serde(rename = "Plugin", default)]
    plugins: Vec<ProjectPlugin>,
}

#[derive(Deserialize)]
struct ProjectPlugin {
    #[serde(rename = "Info")]
    identity: PluginIdentity,
    #[serde(rename = "Data", default)]
    data: PluginData,
}

#[derive(Default, Deserialize)]
struct PluginData {
    #[serde(rename = "CustomData", default)]
    custom: Vec<PluginCustomData>,
}

#[derive(Deserialize)]
struct PluginCustomData {
    #[serde(rename = "Key", default)]
    key: String,
    #[serde(rename = "Value", default)]
    value: String,
}

#[derive(Eq, PartialEq)]
struct ProjectEntry {
    identity: PluginIdentity,
    source_file: Option<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct PluginIdentity {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(
        rename = "Label",
        alias = "URI",
        alias = "Identifier",
        alias = "Setup",
        default
    )]
    label: String,
    #[serde(rename = "Binary", alias = "Filename", default)]
    binary: String,
    #[serde(rename = "UniqueID", default)]
    unique_id: Option<i64>,
}

impl PluginIdentity {
    fn normalize(&mut self) -> Result<()> {
        self.kind.make_ascii_uppercase();
        if let Some(canonical) = match self.kind.as_str() {
            "NATIVE" => Some("INTERNAL"),
            "VST" => Some("VST2"),
            "AUDIOUNIT" => Some("AU"),
            "SF3" => Some("SF2"),
            _ => None,
        } {
            self.kind = canonical.to_owned();
        }
        let label = !self.label.is_empty();
        let binary = !self.binary.is_empty();
        let valid = match self.kind.as_str() {
            "INTERNAL" | "LV2" | "AU" => label,
            "LADSPA" => binary && label && self.unique_id.is_some(),
            "VST2" => binary && self.unique_id.is_some(),
            "DSSI" | "VST3" => binary && label,
            "DLS" | "GIG" | "SF2" | "SFZ" | "JSFX" | "JACK" => binary,
            _ => false,
        };
        anyhow::ensure!(
            valid,
            "missing or unsupported Carla plugin identity ({})",
            self.kind
        );
        Ok(())
    }
}

fn project_entries(chain: CarlaChainType, state: &str) -> Result<Vec<ProjectEntry>> {
    let xml = decode_state(state, chain)?;
    let mut reader = Reader::from_reader(xml.as_slice());
    reader.config_mut().trim_text(true);
    reader.config_mut().expand_empty_elements = true;
    let mut depth = 0usize;
    let mut root = false;
    let mut plugins = 0;
    loop {
        match reader.read_event()? {
            Event::Start(element) => {
                if depth == 0 {
                    anyhow::ensure!(
                        !root && element.name().as_ref() == b"CARLA-PROJECT",
                        "invalid Carla project root"
                    );
                    root = true;
                } else if depth == 1 && element.name().as_ref() == b"Plugin" {
                    plugins += 1;
                    anyhow::ensure!(
                        plugins <= MAX_PROJECT_PLUGINS,
                        "too many plugins in Carla project"
                    );
                }
                depth += 1;
                anyhow::ensure!(
                    depth <= MAX_PROJECT_DEPTH,
                    "Carla project exceeds the XML nesting limit"
                );
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .context("unmatched Carla project close tag")?
            }
            Event::DocType(declaration) if &declaration[..] == b"CARLA-PROJECT" && !root => {}
            Event::DocType(_) => bail!("unsupported Carla project document type"),
            Event::Text(body) if depth == 0 && !body.is_empty() => {
                bail!("unexpected text outside Carla project")
            }
            Event::CData(_) | Event::GeneralRef(_) if depth == 0 => {
                bail!("unexpected content outside Carla project")
            }
            Event::Eof => break,
            _ => {}
        }
    }
    anyhow::ensure!(root && depth == 0, "incomplete Carla project");
    let project: Project =
        quick_xml::de::from_reader(xml.as_slice()).context("reading Carla project inventory")?;
    anyhow::ensure!(
        project.plugins.len() == plugins,
        "incomplete Carla plugin inventory"
    );
    project
        .plugins
        .into_iter()
        .map(|ProjectPlugin { mut identity, data }| {
            identity.normalize()?;
            let mut source_file = None;
            if identity.kind == "INTERNAL"
                && matches!(identity.label.as_str(), "audiofile" | "midifile")
            {
                let mut files = data.custom.into_iter().filter(|entry| entry.key == "file");
                source_file = files
                    .next()
                    .map(|entry| entry.value)
                    .filter(|value| !value.is_empty());
                anyhow::ensure!(
                    files.next().is_none(),
                    "duplicate Carla source file declaration"
                );
            }
            Ok(ProjectEntry {
                identity,
                source_file,
            })
        })
        .collect()
}

/// Verify a disposable candidate before publishing it. The Native ABI setter
/// returns void even when it skips a plugin or its source file.
pub(crate) fn restore_candidate(
    processor: &mut dyn crate::processor::CarlaProcessor,
    state: &str,
) -> Result<()> {
    let chain = processor.info().chain_type;
    let expected = project_entries(chain, state)?;
    for path in expected
        .iter()
        .filter_map(|entry| entry.source_file.as_ref())
    {
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("Carla source file is unavailable: {path}"))?;
        anyhow::ensure!(
            metadata.is_file(),
            "Carla source path is not a file: {path}"
        );
    }
    processor.restore_state(state)?;
    let actual = project_entries(chain, &processor.save_state()?)?;
    anyhow::ensure!(
        actual
            .iter()
            .map(|entry| &entry.identity)
            .eq(expected.iter().map(|entry| &entry.identity)),
        "Carla restored a different plugin inventory (expected {} plugins, got {})",
        expected.len(),
        actual.len()
    );
    anyhow::ensure!(actual == expected, "Carla restored different source files");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn direct_state_codec_round_trips_and_rejects_invalid_input() {
        let encoded = encode_state(CarlaChainType::Rack, b"<CARLA-PROJECT />").unwrap();
        assert_eq!(
            decode_state(&encoded, CarlaChainType::Rack).unwrap(),
            b"<CARLA-PROJECT />"
        );
        let version_one = format!(
            "{STATE_PREFIX_V1}{}",
            base64::engine::general_purpose::STANDARD.encode(b"old direct state")
        );
        assert_eq!(
            decode_state(&version_one, CarlaChainType::Rack).unwrap(),
            b"old direct state"
        );
        assert!(encode_state(CarlaChainType::Rack, b"bad\0state").is_err());
        assert!(decode_state(&encoded, CarlaChainType::Patchbay).is_err());
        assert!(decode_state(
            "shoop-carla-native-state:2:rack:not-base64!",
            CarlaChainType::Rack
        )
        .is_err());
        let oversized = format!(
            "{STATE_PREFIX_V2}rack:{}",
            base64::engine::general_purpose::STANDARD.encode(vec![0_u8; MAX_STATE_BYTES + 1])
        );
        assert!(decode_state(&oversized, CarlaChainType::Rack).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn legacy_lv2_chunks_for_every_descriptor_decode_to_native_state() {
        for (state, project, chain_type) in [
            (
                include_str!("../../shoop_engine/test_data/carla_legacy_rack_loaded_state.json"),
                include_bytes!("../../shoop_engine/test_data/carla_legacy_rack_loaded_project.xml")
                    .as_slice(),
                CarlaChainType::Rack,
            ),
            (
                include_str!(
                    "../../shoop_engine/test_data/carla_legacy_patchbay_loaded_state.json"
                ),
                include_bytes!(
                    "../../shoop_engine/test_data/carla_legacy_patchbay_loaded_project.xml"
                )
                .as_slice(),
                CarlaChainType::Patchbay,
            ),
            (
                include_str!(
                    "../../shoop_engine/test_data/carla_legacy_patchbay16_loaded_state.json"
                ),
                include_bytes!(
                    "../../shoop_engine/test_data/carla_legacy_patchbay16_loaded_project.xml"
                )
                .as_slice(),
                CarlaChainType::Patchbay16x,
            ),
        ] {
            assert_eq!(decode_state(state, chain_type).unwrap(), project);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn legacy_state_codec_rejects_malformed_wrong_type_and_nul_contracts() {
        let wrong_type =
            r#"{"http://kxstudio.sf.net/ns/carla/chunk":{"type":"wrong","value":"AA=="}}"#;
        assert!(decode_state(wrong_type, CarlaChainType::Rack).is_err());
        let no_trailing_nul = format!(
            r#"{{"{LEGACY_CHUNK_URI}":{{"type":"{LEGACY_ATOM_STRING_URI}","value":"{}"}}}}"#,
            base64::engine::general_purpose::STANDARD.encode(b"state")
        );
        assert!(decode_state(&no_trailing_nul, CarlaChainType::Rack).is_err());
        let interior_nul = format!(
            r#"{{"{LEGACY_CHUNK_URI}":{{"type":"{LEGACY_ATOM_STRING_URI}","value":"{}"}}}}"#,
            base64::engine::general_purpose::STANDARD.encode(b"bad\0state\0")
        );
        assert!(decode_state(&interior_nul, CarlaChainType::Rack).is_err());
    }
}
