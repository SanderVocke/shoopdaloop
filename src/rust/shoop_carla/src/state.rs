//! Validate the plugin inventory and local resources on the owning Carla host.

use anyhow::{bail, Context, Result};
use quick_xml::{events::Event, Reader};
use serde::Deserialize;
pub use shoop_plugin_protocol::state::{decode_state, encode_state};
use shoop_plugin_protocol::CarlaChainType;

const MAX_PROJECT_DEPTH: usize = 128;
const MAX_PROJECT_PLUGINS: usize = 512;

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
