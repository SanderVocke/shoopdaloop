//! Carla LV2 plugin discovery and static port/UI metadata.
//!
//! This is the first layer of the Rust Carla host.  It deliberately keeps Lilv-owned
//! objects inside the short-lived discovery function and stores only plain Rust data
//! afterwards; realtime processing/state/UI instantiation can build on this without
//! making frontend code depend on Lilv lifetimes.

use crate::FXChainType;
use anyhow::{anyhow, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarlaPortSet {
    pub audio_inputs: Vec<CarlaPort>,
    pub audio_outputs: Vec<CarlaPort>,
    pub midi_inputs: Vec<CarlaPort>,
    pub midi_outputs: Vec<CarlaPort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarlaPort {
    pub symbol: String,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarlaUiInfo {
    pub binary_path: Option<String>,
    pub bundle_path: Option<String>,
    pub is_external_ui: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarlaPluginInfo {
    pub chain_type: FXChainType,
    pub plugin_uri: &'static str,
    pub ports: CarlaPortSet,
    pub required_features: Vec<String>,
    pub optional_features: Vec<String>,
    pub ui: Option<CarlaUiInfo>,
}

pub const CARLA_RACK_URI: &str = "http://kxstudio.sf.net/carla/plugins/carlarack";
pub const CARLA_PATCHBAY_URI: &str = "http://kxstudio.sf.net/carla/plugins/carlapatchbay";
pub const CARLA_PATCHBAY_16_URI: &str = "http://kxstudio.sf.net/carla/plugins/carlapatchbay16";
const EXTERNAL_UI_URI: &str = "http://kxstudio.sf.net/ns/lv2ext/external-ui#Widget";

pub fn carla_plugin_uri(chain_type: FXChainType) -> Option<&'static str> {
    match chain_type {
        FXChainType::CarlaRack => Some(CARLA_RACK_URI),
        FXChainType::CarlaPatchbay => Some(CARLA_PATCHBAY_URI),
        FXChainType::CarlaPatchbay16x => Some(CARLA_PATCHBAY_16_URI),
        FXChainType::Test2x2x1 => None,
    }
}

pub fn carla_audio_port_count(chain_type: FXChainType) -> Option<usize> {
    match chain_type {
        FXChainType::CarlaRack | FXChainType::CarlaPatchbay => Some(2),
        FXChainType::CarlaPatchbay16x => Some(16),
        FXChainType::Test2x2x1 => None,
    }
}

pub fn discover_carla_plugin(chain_type: FXChainType) -> Result<CarlaPluginInfo> {
    let plugin_uri = carla_plugin_uri(chain_type)
        .ok_or_else(|| anyhow!("{chain_type:?} is not a Carla LV2 chain type"))?;
    let n_audio = carla_audio_port_count(chain_type).expect("Carla chain type has audio count");

    let world = lilv::World::with_load_all();
    let uri = world.new_uri(plugin_uri);
    let plugin = world
        .plugins()
        .plugin(&uri)
        .ok_or_else(|| anyhow!("Carla LV2 plugin {plugin_uri} not found in LV2_PATH"))?;

    let ports = CarlaPortSet {
        audio_inputs: required_ports(
            &world,
            &plugin,
            (1..=n_audio).map(|i| format!("lv2_audio_in_{i}")),
        )?,
        audio_outputs: required_ports(
            &world,
            &plugin,
            (1..=n_audio).map(|i| format!("lv2_audio_out_{i}")),
        )?,
        midi_inputs: required_ports(&world, &plugin, ["lv2_events_in".to_string()])?,
        midi_outputs: required_ports(&world, &plugin, ["lv2_events_out".to_string()])?,
    };

    Ok(CarlaPluginInfo {
        chain_type,
        plugin_uri,
        ports,
        required_features: plugin
            .required_features()
            .iter()
            .filter_map(|n| n.as_uri().map(ToString::to_string))
            .collect(),
        optional_features: plugin
            .optional_features()
            .iter()
            .filter_map(|n| n.as_uri().map(ToString::to_string))
            .collect(),
        ui: discover_ui(&world, &plugin)?,
    })
}

fn required_ports(
    world: &lilv::World,
    plugin: &lilv::plugin::Plugin,
    symbols: impl IntoIterator<Item = String>,
) -> Result<Vec<CarlaPort>> {
    symbols
        .into_iter()
        .map(|symbol| {
            let node = world.new_string(&symbol);
            let port = plugin
                .port_by_symbol(&node)
                .ok_or_else(|| anyhow!("Carla LV2 plugin is missing required port {symbol}"))?;
            Ok(CarlaPort {
                symbol,
                index: port.index(),
            })
        })
        .collect()
}

fn discover_ui(world: &lilv::World, plugin: &lilv::plugin::Plugin) -> Result<Option<CarlaUiInfo>> {
    let Some(uis) = plugin.uis() else {
        return Ok(None);
    };
    let external_ui = world.new_uri(EXTERNAL_UI_URI);
    let mut iter = uis.iter();
    let Some(ui) = iter.next() else {
        return Ok(None);
    };
    if iter.next().is_some() {
        return Err(anyhow!(
            "expected at most one Carla LV2 UI for {}, found more",
            plugin.uri().as_uri().unwrap_or("unknown")
        ));
    }
    Ok(Some(CarlaUiInfo {
        binary_path: ui.binary_uri().and_then(|n| n.path().map(|(p, _)| p)),
        bundle_path: ui.bundle_uri().and_then(|n| n.path().map(|(p, _)| p)),
        is_external_ui: ui.is_a(&external_ui),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carla_type_metadata_matches_the_legacy_host() {
        assert_eq!(
            carla_plugin_uri(FXChainType::CarlaRack),
            Some(CARLA_RACK_URI)
        );
        assert_eq!(
            carla_plugin_uri(FXChainType::CarlaPatchbay),
            Some(CARLA_PATCHBAY_URI)
        );
        assert_eq!(
            carla_plugin_uri(FXChainType::CarlaPatchbay16x),
            Some(CARLA_PATCHBAY_16_URI)
        );
        assert_eq!(carla_audio_port_count(FXChainType::CarlaRack), Some(2));
        assert_eq!(carla_audio_port_count(FXChainType::CarlaPatchbay), Some(2));
        assert_eq!(
            carla_audio_port_count(FXChainType::CarlaPatchbay16x),
            Some(16)
        );
        assert_eq!(carla_plugin_uri(FXChainType::Test2x2x1), None);
    }

    #[test]
    fn discovers_installed_carla_plugin_ports_when_available() {
        let Ok(info) = discover_carla_plugin(FXChainType::CarlaRack) else {
            eprintln!("skipping Carla LV2 discovery test; Carla Rack is not installed in LV2_PATH");
            return;
        };
        assert_eq!(info.plugin_uri, CARLA_RACK_URI);
        assert_eq!(info.ports.audio_inputs.len(), 2);
        assert_eq!(info.ports.audio_outputs.len(), 2);
        assert_eq!(info.ports.midi_inputs.len(), 1);
        assert_eq!(info.ports.midi_outputs.len(), 1);
        assert_eq!(info.ports.audio_inputs[0].symbol, "lv2_audio_in_1");
        assert_eq!(info.ports.audio_outputs[1].symbol, "lv2_audio_out_2");
        assert_eq!(info.ports.midi_inputs[0].symbol, "lv2_events_in");
        assert_eq!(info.ports.midi_outputs[0].symbol, "lv2_events_out");
        assert!(
            info.required_features
                .iter()
                .any(|f| f == "http://lv2plug.in/ns/ext/urid#map"),
            "Carla should declare the URID map feature as required: {:?}",
            info.required_features
        );
        assert!(info.ui.as_ref().is_none_or(|ui| ui.is_external_ui));
    }
}
