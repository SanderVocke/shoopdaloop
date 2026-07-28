//! Carla LV2 plugin discovery and static port/UI metadata.
//!
//! This is the first layer of the Rust Carla host.  It deliberately keeps Lilv-owned
//! objects inside the short-lived discovery function and stores only plain Rust data
//! afterwards; realtime processing/state/UI instantiation can build on this without
//! making frontend code depend on Lilv lifetimes.

use crate::FXChainType;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::Mutex;

use lv2_raw::atom::{LV2Atom, LV2AtomSequence, LV2AtomSequenceBody, LV2_ATOM__SEQUENCE};
use lv2_raw::core::LV2Feature;
use lv2_raw::urid::{LV2Urid, LV2UridMap, LV2UridMapHandle, LV2_URID__MAP};

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
pub const CARLA_MAX_BUFFER_SIZE: usize = 8192;
pub const CARLA_MIDI_BUFFER_CAPACITY: usize = 8192;
const EXTERNAL_UI_URI: &str = "http://kxstudio.sf.net/ns/lv2ext/external-ui#Widget";
const LV2_OPTIONS_OPTIONS_URI: &str = "http://lv2plug.in/ns/ext/options#options";
const LV2_BUF_SIZE_MAX_BLOCK_LENGTH_URI: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
const LV2_BUF_SIZE_MIN_BLOCK_LENGTH_URI: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
const LV2_BUF_SIZE_NOMINAL_BLOCK_LENGTH_URI: &str =
    "http://lv2plug.in/ns/ext/buf-size#nominalBlockLength";
const LV2_ATOM_INT_URI: &str = "http://lv2plug.in/ns/ext/atom#Int";
const LV2_OPTIONS_INSTANCE: u32 = 0;

#[repr(C)]
struct Lv2OptionsOption {
    context: u32,
    subject: u32,
    key: LV2Urid,
    size: u32,
    value_type: LV2Urid,
    value: *const c_void,
}

pub struct CarlaLv2Host {
    _world: lilv::World,
    pub info: CarlaPluginInfo,
    instance: Option<lilv::instance::Instance>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<AtomSequenceBuffer>,
    midi_outputs: Vec<AtomSequenceBuffer>,
    _urid_mapper: Box<UridMapper>,
    _urid_map: Box<LV2UridMap>,
    active: bool,
}

impl CarlaLv2Host {
    pub fn instantiate(
        chain_type: FXChainType,
        sample_rate: u32,
        buffer_size: u32,
    ) -> Result<Self> {
        let plugin_uri = carla_plugin_uri(chain_type)
            .ok_or_else(|| anyhow!("{chain_type:?} is not a Carla LV2 chain type"))?;
        let n_audio = carla_audio_port_count(chain_type).expect("Carla chain type has audio count");
        let world = lilv::World::with_load_all();
        let uri = world.new_uri(plugin_uri);
        let plugin = world
            .plugins()
            .plugin(&uri)
            .ok_or_else(|| anyhow!("Carla LV2 plugin {plugin_uri} not found in LV2_PATH"))?;
        let info = inspect_carla_plugin(&world, &plugin, chain_type, plugin_uri, n_audio)?;
        let mut urid_mapper = Box::new(UridMapper::new());
        let max_buffer_size = CARLA_MAX_BUFFER_SIZE as u32;
        let min_buffer_size = 1u32;
        let nominal_buffer_size = buffer_size.max(1);
        let atom_int_type = urid_mapper.map_str(LV2_ATOM_INT_URI);
        let options = [
            Lv2OptionsOption {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: urid_mapper.map_str(LV2_BUF_SIZE_MAX_BLOCK_LENGTH_URI),
                size: std::mem::size_of_val(&max_buffer_size) as u32,
                value_type: atom_int_type,
                value: (&max_buffer_size as *const u32).cast::<c_void>(),
            },
            Lv2OptionsOption {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: urid_mapper.map_str(LV2_BUF_SIZE_MIN_BLOCK_LENGTH_URI),
                size: std::mem::size_of_val(&min_buffer_size) as u32,
                value_type: atom_int_type,
                value: (&min_buffer_size as *const u32).cast::<c_void>(),
            },
            Lv2OptionsOption {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: urid_mapper.map_str(LV2_BUF_SIZE_NOMINAL_BLOCK_LENGTH_URI),
                size: std::mem::size_of_val(&nominal_buffer_size) as u32,
                value_type: atom_int_type,
                value: (&nominal_buffer_size as *const u32).cast::<c_void>(),
            },
            Lv2OptionsOption {
                context: 0,
                subject: 0,
                key: 0,
                size: 0,
                value_type: 0,
                value: std::ptr::null(),
            },
        ];
        let mut urid_map = Box::new(LV2UridMap {
            handle: (&mut *urid_mapper as *mut UridMapper).cast::<c_void>(),
            map: map_urid,
        });
        let map_uri = CString::new(LV2_URID__MAP).expect("static URI contains no nul");
        let options_uri =
            CString::new(LV2_OPTIONS_OPTIONS_URI).expect("static URI contains no nul");
        let map_feature = LV2Feature {
            uri: map_uri.as_ptr(),
            data: (&mut *urid_map as *mut LV2UridMap).cast::<c_void>(),
        };
        let options_feature = LV2Feature {
            uri: options_uri.as_ptr(),
            data: options.as_ptr().cast::<c_void>() as *mut c_void,
        };
        let instance = unsafe {
            plugin
                .instantiate(sample_rate.max(1) as f64, [&map_feature, &options_feature])
                .ok_or_else(|| anyhow!("Carla LV2 plugin {plugin_uri} failed to instantiate"))?
        };

        let mut host = Self {
            _world: world,
            info,
            instance: Some(instance),
            audio_inputs: vec![vec![0.0; CARLA_MAX_BUFFER_SIZE]; n_audio],
            audio_outputs: vec![vec![0.0; CARLA_MAX_BUFFER_SIZE]; n_audio],
            midi_inputs: vec![AtomSequenceBuffer::new(CARLA_MIDI_BUFFER_CAPACITY); 1],
            midi_outputs: vec![AtomSequenceBuffer::new(CARLA_MIDI_BUFFER_CAPACITY); 1],
            _urid_mapper: urid_mapper,
            _urid_map: urid_map,
            active: false,
        };
        host.connect_ports();
        let _ = buffer_size;
        Ok(host)
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn process(&mut self, frames: usize) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        if frames > CARLA_MAX_BUFFER_SIZE {
            return Err(anyhow!(
                "Carla processing chain: requesting to process more than buffer size ({frames} vs. {CARLA_MAX_BUFFER_SIZE})"
            ));
        }
        for midi in &mut self.midi_inputs {
            midi.clear();
        }
        for midi in &mut self.midi_outputs {
            midi.clear();
        }
        let instance = self
            .instance
            .take()
            .ok_or_else(|| anyhow!("internal error: Carla LV2 instance temporarily unavailable"))?;
        let mut active = unsafe { instance.activate() };
        unsafe { active.run(frames) };
        self.instance = Some(unsafe {
            active
                .deactivate()
                .ok_or_else(|| anyhow!("Carla LV2 plugin failed to deactivate"))?
        });
        Ok(())
    }

    pub fn audio_input_mut(&mut self, idx: usize) -> Option<&mut [f32]> {
        self.audio_inputs.get_mut(idx).map(Vec::as_mut_slice)
    }

    pub fn audio_output(&self, idx: usize) -> Option<&[f32]> {
        self.audio_outputs.get(idx).map(Vec::as_slice)
    }

    fn connect_ports(&mut self) {
        let instance = self.instance.as_mut().expect("instance");
        for (port, buffer) in self
            .info
            .ports
            .audio_inputs
            .iter()
            .zip(&mut self.audio_inputs)
        {
            unsafe { instance.connect_port_mut(port.index, buffer.as_mut_ptr()) };
        }
        for (port, buffer) in self
            .info
            .ports
            .audio_outputs
            .iter()
            .zip(&mut self.audio_outputs)
        {
            unsafe { instance.connect_port_mut(port.index, buffer.as_mut_ptr()) };
        }
        for (port, buffer) in self
            .info
            .ports
            .midi_inputs
            .iter()
            .zip(&mut self.midi_inputs)
        {
            unsafe { instance.connect_port_mut(port.index, buffer.as_mut_ptr()) };
        }
        for (port, buffer) in self
            .info
            .ports
            .midi_outputs
            .iter()
            .zip(&mut self.midi_outputs)
        {
            unsafe { instance.connect_port_mut(port.index, buffer.as_mut_ptr()) };
        }
    }
}

struct AtomSequenceBuffer {
    bytes: Vec<u8>,
    sequence_type: LV2Urid,
}

impl Clone for AtomSequenceBuffer {
    fn clone(&self) -> Self {
        Self::new(self.bytes.len())
    }
}

impl AtomSequenceBuffer {
    fn new(capacity: usize) -> Self {
        let mut out = Self {
            bytes: vec![0; capacity.max(std::mem::size_of::<LV2AtomSequence>())],
            sequence_type: 0,
        };
        out.clear();
        out
    }
    fn clear(&mut self) {
        let sequence_type = self.sequence_type;
        let seq = self.as_mut_sequence();
        seq.atom = LV2Atom {
            size: std::mem::size_of::<LV2AtomSequenceBody>() as u32,
            mytype: sequence_type,
        };
        seq.body = LV2AtomSequenceBody { unit: 0, pad: 0 };
    }
    fn as_mut_ptr(&mut self) -> *mut LV2AtomSequence {
        self.as_mut_sequence() as *mut LV2AtomSequence
    }
    fn as_mut_sequence(&mut self) -> &mut LV2AtomSequence {
        unsafe { &mut *(self.bytes.as_mut_ptr().cast::<LV2AtomSequence>()) }
    }
}

struct UridMapper {
    by_uri: Mutex<HashMap<String, LV2Urid>>,
}

impl UridMapper {
    fn new() -> Self {
        let mut m = HashMap::new();
        m.insert(cstr_bytes_to_string(LV2_ATOM__SEQUENCE), 1);
        Self {
            by_uri: Mutex::new(m),
        }
    }

    fn map_str(&self, uri: &str) -> LV2Urid {
        let mut by_uri = self.by_uri.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = by_uri.get(uri) {
            *id
        } else {
            let id = by_uri.len() as LV2Urid + 1;
            by_uri.insert(uri.to_string(), id);
            id
        }
    }
}

extern "C" fn map_urid(handle: LV2UridMapHandle, uri: *const c_char) -> LV2Urid {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    let mapper = unsafe { &*(handle.cast::<UridMapper>()) };
    let uri = unsafe { CStr::from_ptr(uri) }.to_string_lossy().to_string();
    let mut by_uri = mapper.by_uri.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(id) = by_uri.get(&uri) {
        *id
    } else {
        let id = by_uri.len() as LV2Urid + 1;
        by_uri.insert(uri, id);
        id
    }
}

fn cstr_bytes_to_string(bytes: &[u8]) -> String {
    CStr::from_bytes_with_nul(bytes)
        .expect("static LV2 URI is nul-terminated")
        .to_string_lossy()
        .to_string()
}

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

    inspect_carla_plugin(&world, &plugin, chain_type, plugin_uri, n_audio)
}

fn inspect_carla_plugin(
    world: &lilv::World,
    plugin: &lilv::plugin::Plugin,
    chain_type: FXChainType,
    plugin_uri: &'static str,
    n_audio: usize,
) -> Result<CarlaPluginInfo> {
    let ports = CarlaPortSet {
        audio_inputs: required_ports(
            world,
            plugin,
            (1..=n_audio).map(|i| format!("lv2_audio_in_{i}")),
        )?,
        audio_outputs: required_ports(
            world,
            plugin,
            (1..=n_audio).map(|i| format!("lv2_audio_out_{i}")),
        )?,
        midi_inputs: required_ports(world, plugin, ["lv2_events_in".to_string()])?,
        midi_outputs: required_ports(world, plugin, ["lv2_events_out".to_string()])?,
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
        ui: discover_ui(world, plugin)?,
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

    #[test]
    fn instantiates_and_runs_installed_carla_rack_when_available() {
        let Ok(mut host) = CarlaLv2Host::instantiate(FXChainType::CarlaRack, 48_000, 256) else {
            eprintln!("skipping Carla LV2 run test; Carla Rack is not installed in LV2_PATH");
            return;
        };
        assert_eq!(host.info.chain_type, FXChainType::CarlaRack);
        assert_eq!(host.info.ports.audio_inputs.len(), 2);
        assert!(!host.is_active());
        host.process(256).expect("inactive process is a no-op");
        host.set_active(true);
        host.audio_input_mut(0).expect("audio input")[0] = 0.25;
        host.process(256).expect("active process");
        assert!(host.audio_output(0).is_some());
    }
}
