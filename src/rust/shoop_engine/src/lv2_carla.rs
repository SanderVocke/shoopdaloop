//! Carla LV2 plugin discovery and static port/UI metadata.
//!
//! This is the first layer of the Rust Carla host.  It deliberately keeps Lilv-owned
//! objects inside the short-lived discovery function and stores only plain Rust data
//! afterwards; realtime processing/state/UI instantiation can build on this without
//! making frontend code depend on Lilv lifetimes.

use crate::FXChainType;
use anyhow::{anyhow, Result};
use base64::Engine;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_uint, c_void};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use lv2_raw::atom::{
    LV2Atom, LV2AtomEvent, LV2AtomSequence, LV2AtomSequenceBody, LV2_ATOM__SEQUENCE,
};
use lv2_raw::atomutils::lv2_atom_pad_size;
use lv2_raw::core::{LV2Feature, LV2Handle};
use lv2_raw::midi::LV2_MIDI__MIDIEVENT;
use lv2_raw::ui::{LV2UIDescriptorRaw, LV2UIExternalUIHost, LV2UIExternalUIWidget, LV2UIWidget};
use lv2_raw::urid::{LV2Urid, LV2UridMap, LV2UridMapHandle, LV2_URID__MAP, LV2_URID__UNMAP};

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
    pub uri: String,
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
const LV2_EXTERNAL_UI_HOST_URI: &str = "http://kxstudio.sf.net/ns/lv2ext/external-ui#Host";
const LV2_INSTANCE_ACCESS_URI: &str = "http://lv2plug.in/ns/ext/instance-access";
const LV2_OPTIONS_OPTIONS_URI: &str = "http://lv2plug.in/ns/ext/options#options";
const LV2_BUF_SIZE_MAX_BLOCK_LENGTH_URI: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
const LV2_BUF_SIZE_MIN_BLOCK_LENGTH_URI: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
const LV2_BUF_SIZE_NOMINAL_BLOCK_LENGTH_URI: &str =
    "http://lv2plug.in/ns/ext/buf-size#nominalBlockLength";
const LV2_ATOM_INT_URI: &str = "http://lv2plug.in/ns/ext/atom#Int";
const LV2_OPTIONS_INSTANCE: u32 = 0;
const LV2_STATE_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/state#interface";
const LV2_STATE_IS_POD: u32 = 1 << 0;
const LV2_STATE_IS_PORTABLE: u32 = 1 << 1;
const LV2_STATE_SUCCESS: u32 = 0;
const LV2_STATE_ERR_BAD_FLAGS: u32 = 3;
const LV2_STATE_ERR_NO_PROPERTY: u32 = 5;

#[repr(C)]
struct Lv2UridUnmap {
    handle: LV2UridMapHandle,
    unmap: extern "C" fn(handle: LV2UridMapHandle, urid: LV2Urid) -> *const c_char,
}

#[repr(C)]
struct Lv2OptionsOption {
    context: u32,
    subject: u32,
    key: LV2Urid,
    size: u32,
    value_type: LV2Urid,
    value: *const c_void,
}

type Lv2StateStoreFunction = unsafe extern "C" fn(
    handle: *mut c_void,
    key: u32,
    value: *const c_void,
    size: usize,
    value_type: u32,
    flags: u32,
) -> u32;
type Lv2StateRetrieveFunction = unsafe extern "C" fn(
    handle: *mut c_void,
    key: u32,
    size: *mut usize,
    value_type: *mut u32,
    flags: *mut u32,
) -> *const c_void;

#[repr(C)]
struct Lv2StateInterface {
    save: unsafe extern "C" fn(
        instance: LV2Handle,
        store: Lv2StateStoreFunction,
        handle: *mut c_void,
        flags: u32,
        features: *const *const LV2Feature,
    ) -> u32,
    restore: unsafe extern "C" fn(
        instance: LV2Handle,
        retrieve: Lv2StateRetrieveFunction,
        handle: *mut c_void,
        flags: u32,
        features: *const *const LV2Feature,
    ) -> u32,
}

type Lv2UiDescriptorFn = unsafe extern "C" fn(index: u32) -> *const LV2UIDescriptorRaw;

struct CarlaUiRuntime {
    _library: libloading::Library,
    descriptor: *const LV2UIDescriptorRaw,
    handle: *mut c_void,
    widget: *const LV2UIExternalUIWidget,
    closed: Box<AtomicBool>,
    _host: Box<LV2UIExternalUIHost>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    _human_id: CString,
}

unsafe impl Send for CarlaUiRuntime {}

impl Drop for CarlaUiRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        unsafe {
            if !self.widget.is_null() {
                if let Some(hide) = (*self.widget).hide {
                    hide(self.widget);
                }
            }
            if !self.descriptor.is_null() && !self.handle.is_null() {
                ((*self.descriptor).cleanup)(self.handle);
            }
        }
    }
}

pub struct CarlaLv2Host {
    _world: lilv::World,
    pub info: CarlaPluginInfo,
    instance: Option<lilv::instance::Instance>,
    state_interface: Option<NonNull<Lv2StateInterface>>,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    midi_inputs: Vec<AtomSequenceBuffer>,
    midi_outputs: Vec<AtomSequenceBuffer>,
    _urid_mapper: Box<UridMapper>,
    _urid_map: Box<LV2UridMap>,
    _urid_unmap: Box<Lv2UridUnmap>,
    ui_runtime: Option<CarlaUiRuntime>,
    active: bool,
    visible: bool,
}

// Carla's LV2 instance is owned by this host object and only accessed through
// mutable methods; app/session users wrap it in a Mutex before sharing it with
// callback threads.
unsafe impl Send for CarlaLv2Host {}

impl std::fmt::Debug for CarlaLv2Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CarlaLv2Host")
            .field("info", &self.info)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
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
        let sequence_type = urid_mapper.map_str(&cstr_bytes_to_string(LV2_ATOM__SEQUENCE));
        let midi_event_type = urid_mapper.map_str(&cstr_bytes_to_string(LV2_MIDI__MIDIEVENT));
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
        let mut urid_unmap = Box::new(Lv2UridUnmap {
            handle: (&mut *urid_mapper as *mut UridMapper).cast::<c_void>(),
            unmap: unmap_urid,
        });
        let map_uri = CString::new(LV2_URID__MAP).expect("static URI contains no nul");
        let unmap_uri = CString::new(LV2_URID__UNMAP).expect("static URI contains no nul");
        let options_uri =
            CString::new(LV2_OPTIONS_OPTIONS_URI).expect("static URI contains no nul");
        let map_feature = LV2Feature {
            uri: map_uri.as_ptr(),
            data: (&mut *urid_map as *mut LV2UridMap).cast::<c_void>(),
        };
        let unmap_feature = LV2Feature {
            uri: unmap_uri.as_ptr(),
            data: (&mut *urid_unmap as *mut Lv2UridUnmap).cast::<c_void>(),
        };
        let options_feature = LV2Feature {
            uri: options_uri.as_ptr(),
            data: options.as_ptr().cast::<c_void>() as *mut c_void,
        };
        let instance = unsafe {
            plugin
                .instantiate(
                    sample_rate.max(1) as f64,
                    [&map_feature, &unmap_feature, &options_feature],
                )
                .ok_or_else(|| anyhow!("Carla LV2 plugin {plugin_uri} failed to instantiate"))?
        };
        let state_interface =
            unsafe { instance.extension_data::<Lv2StateInterface>(LV2_STATE_INTERFACE_URI) };

        let mut host = Self {
            _world: world,
            info,
            instance: Some(instance),
            state_interface,
            audio_inputs: vec![vec![0.0; CARLA_MAX_BUFFER_SIZE]; n_audio],
            audio_outputs: vec![vec![0.0; CARLA_MAX_BUFFER_SIZE]; n_audio],
            midi_inputs: vec![
                AtomSequenceBuffer::new(
                    CARLA_MIDI_BUFFER_CAPACITY,
                    sequence_type,
                    midi_event_type,
                );
                1
            ],
            midi_outputs: vec![
                AtomSequenceBuffer::new(
                    CARLA_MIDI_BUFFER_CAPACITY,
                    sequence_type,
                    midi_event_type,
                );
                1
            ],
            _urid_mapper: urid_mapper,
            _urid_map: urid_map,
            _urid_unmap: urid_unmap,
            ui_runtime: None,
            active: false,
            visible: false,
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

    pub fn set_visible(&mut self, visible: bool) -> Result<()> {
        if visible {
            self.show_ui()
        } else {
            self.hide_ui();
            Ok(())
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn show_ui(&mut self) -> Result<()> {
        if self.visible {
            return Ok(());
        }
        if self.ui_runtime.is_none() {
            self.ui_runtime = Some(self.instantiate_ui()?);
        }
        let runtime = self.ui_runtime.as_mut().expect("runtime just instantiated");
        unsafe {
            if !runtime.widget.is_null() {
                if let Some(show) = (*runtime.widget).show {
                    show(runtime.widget);
                }
                let widget = runtime.widget as usize;
                let stop = runtime.stop.clone();
                let closed = (&*runtime.closed as *const AtomicBool) as usize;
                runtime.thread = Some(thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let widget = widget as *const LV2UIExternalUIWidget;
                        if widget.is_null() {
                            break;
                        }
                        if (*(closed as *const AtomicBool)).load(Ordering::Relaxed) {
                            break;
                        }
                        if let Some(run) = (*widget).run {
                            run(widget);
                        }
                        let next = Instant::now() + Duration::from_millis(30);
                        thread::sleep(next.saturating_duration_since(Instant::now()));
                    }
                }));
            }
        }
        self.visible = true;
        Ok(())
    }

    fn hide_ui(&mut self) {
        self.visible = false;
        self.ui_runtime.take();
    }

    fn instantiate_ui(&mut self) -> Result<CarlaUiRuntime> {
        let ui = self
            .info
            .ui
            .as_ref()
            .ok_or_else(|| anyhow!("Carla plugin has no external UI metadata"))?;
        let binary_path = ui
            .binary_path
            .as_ref()
            .ok_or_else(|| anyhow!("Carla external UI has no binary path"))?;
        let bundle_path = ui
            .bundle_path
            .as_ref()
            .ok_or_else(|| anyhow!("Carla external UI has no bundle path"))?;
        let instance = self
            .instance
            .as_ref()
            .ok_or_else(|| anyhow!("internal error: Carla LV2 instance temporarily unavailable"))?;
        let library = unsafe { libloading::Library::new(binary_path)? };
        let descriptor_fn: libloading::Symbol<Lv2UiDescriptorFn> =
            unsafe { library.get(b"lv2ui_descriptor\0")? };
        let mut descriptor = std::ptr::null();
        for idx in 0..1024u32 {
            let d = unsafe { descriptor_fn(idx) };
            if d.is_null() {
                break;
            }
            let uri = unsafe { CStr::from_ptr((*d).uri) }.to_string_lossy();
            if uri == ui.uri {
                descriptor = d;
                break;
            }
        }
        if descriptor.is_null() {
            return Err(anyhow!("Carla external UI descriptor {} not found", ui.uri));
        }
        let plugin_uri = CString::new(self.info.plugin_uri).expect("static URI contains no nul");
        let bundle_path = CString::new(bundle_path.as_str())?;
        let human_id = CString::new("shoopdaloop")?;
        let closed = Box::new(AtomicBool::new(false));
        let mut host = Box::new(LV2UIExternalUIHost {
            ui_closed: unsafe {
                std::mem::transmute::<_, extern "C" fn(_) -> c_void>(
                    external_ui_closed as extern "C" fn(_),
                )
            },
            plugin_human_id: human_id.as_ptr(),
        });
        let instance_uri =
            CString::new(LV2_INSTANCE_ACCESS_URI).expect("static URI contains no nul");
        let external_uri =
            CString::new(LV2_EXTERNAL_UI_HOST_URI).expect("static URI contains no nul");
        let instance_feature = LV2Feature {
            uri: instance_uri.as_ptr(),
            data: instance.handle(),
        };
        let external_feature = LV2Feature {
            uri: external_uri.as_ptr(),
            data: (&mut *host as *mut LV2UIExternalUIHost).cast::<c_void>(),
        };
        let features = [
            &instance_feature as *const LV2Feature,
            &external_feature as *const LV2Feature,
            std::ptr::null(),
        ];
        let mut widget: LV2UIWidget = std::ptr::null_mut();
        let handle = unsafe {
            ((*descriptor).instantiate_raw)(
                descriptor,
                plugin_uri.as_ptr(),
                bundle_path.as_ptr(),
                Some(ui_write_ignored),
                (&*closed as *const AtomicBool).cast::<c_void>(),
                &mut widget,
                features.as_ptr(),
            )
        };
        if handle.is_null() || widget.is_null() {
            return Err(anyhow!("Could not instantiate Carla external UI"));
        }
        Ok(CarlaUiRuntime {
            _library: library,
            descriptor,
            handle,
            widget: widget.cast::<LV2UIExternalUIWidget>(),
            closed,
            _host: host,
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            _human_id: human_id,
        })
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

    pub fn set_midi_input_events<'a>(
        &mut self,
        idx: usize,
        events: impl IntoIterator<Item = (u32, &'a [u8])>,
    ) -> Result<()> {
        let buffer = self
            .midi_inputs
            .get_mut(idx)
            .ok_or_else(|| anyhow!("No Carla MIDI input port {idx}"))?;
        buffer.clear();
        for (time, data) in events {
            buffer.append_midi_event(time, data)?;
        }
        Ok(())
    }

    pub fn midi_output_events(&mut self, idx: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        self.midi_outputs
            .get_mut(idx)
            .ok_or_else(|| anyhow!("No Carla MIDI output port {idx}"))?
            .midi_events()
    }

    pub fn save_state_string(&mut self) -> Result<String> {
        let state_interface = self
            .state_interface
            .ok_or_else(|| anyhow!("No state interface for Carla chain"))?;
        let instance = self
            .instance
            .as_ref()
            .ok_or_else(|| anyhow!("internal error: Carla LV2 instance temporarily unavailable"))?;
        let mut state = Lv2StateString::default();
        let features = [std::ptr::null::<LV2Feature>()];
        let status = unsafe {
            (state_interface.as_ref().save)(
                instance.handle(),
                lv2_state_store,
                (&mut state as *mut Lv2StateString).cast::<c_void>(),
                LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE,
                features.as_ptr(),
            )
        };
        if status != LV2_STATE_SUCCESS {
            return Err(anyhow!("Carla LV2 state save failed with status {status}"));
        }
        state.serialize(&self._urid_mapper)
    }

    pub fn restore_state_string(&mut self, s: &str) -> Result<()> {
        let state_interface = self
            .state_interface
            .ok_or_else(|| anyhow!("No state interface for Carla chain"))?;
        let instance = self
            .instance
            .as_ref()
            .ok_or_else(|| anyhow!("internal error: Carla LV2 instance temporarily unavailable"))?;
        let mut state = Lv2StateString::deserialize(s, &self._urid_mapper)?;
        let features = [std::ptr::null::<LV2Feature>()];
        let status = unsafe {
            (state_interface.as_ref().restore)(
                instance.handle(),
                lv2_state_retrieve,
                (&mut state as *mut Lv2StateString).cast::<c_void>(),
                LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE,
                features.as_ptr(),
            )
        };
        if status != LV2_STATE_SUCCESS {
            return Err(anyhow!(
                "Carla LV2 state restore failed with status {status}"
            ));
        }
        Ok(())
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
    midi_event_type: LV2Urid,
}

impl Clone for AtomSequenceBuffer {
    fn clone(&self) -> Self {
        Self::new(self.bytes.len(), self.sequence_type, self.midi_event_type)
    }
}

impl AtomSequenceBuffer {
    fn new(capacity: usize, sequence_type: LV2Urid, midi_event_type: LV2Urid) -> Self {
        let mut out = Self {
            bytes: vec![0; capacity.max(std::mem::size_of::<LV2AtomSequence>())],
            sequence_type,
            midi_event_type,
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
    fn append_midi_event(&mut self, time: u32, data: &[u8]) -> Result<()> {
        let event_header_size = std::mem::size_of::<LV2AtomEvent>();
        let total_size = event_header_size + data.len();
        let padded_size = lv2_atom_pad_size(total_size as u32) as usize;
        let atom_header_size = std::mem::size_of::<LV2Atom>();
        let write_at = atom_header_size + self.as_mut_sequence().atom.size as usize;
        if write_at + padded_size > self.bytes.len() {
            return Err(anyhow!("Carla MIDI atom sequence buffer overflow"));
        }
        unsafe {
            let ptr = self.bytes.as_mut_ptr().add(write_at);
            let event = &mut *(ptr.cast::<LV2AtomEvent>());
            event.time_in_frames = time as i64;
            event.body = LV2Atom {
                size: data.len() as u32,
                mytype: self.midi_event_type,
            };
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.add(event_header_size), data.len());
            for b in &mut self.bytes[write_at + total_size..write_at + padded_size] {
                *b = 0;
            }
        }
        self.as_mut_sequence().atom.size += padded_size as u32;
        Ok(())
    }

    fn midi_events(&mut self) -> Result<Vec<(u32, Vec<u8>)>> {
        let mut out = Vec::new();
        let seq = self.as_mut_sequence();
        let atom_size = seq.atom.size as usize;
        let mut offset = std::mem::size_of::<LV2AtomSequence>();
        let end = std::mem::size_of::<LV2Atom>() + atom_size;
        while offset + std::mem::size_of::<LV2AtomEvent>() <= end {
            let event = unsafe { &*(self.bytes.as_ptr().add(offset).cast::<LV2AtomEvent>()) };
            let data_len = event.body.size as usize;
            let data_at = offset + std::mem::size_of::<LV2AtomEvent>();
            if data_at + data_len > self.bytes.len() || data_at + data_len > end {
                return Err(anyhow!("Invalid Carla MIDI atom sequence event"));
            }
            if event.body.mytype == self.midi_event_type {
                out.push((
                    event.time_in_frames.max(0) as u32,
                    self.bytes[data_at..data_at + data_len].to_vec(),
                ));
            }
            offset +=
                lv2_atom_pad_size((std::mem::size_of::<LV2AtomEvent>() + data_len) as u32) as usize;
        }
        Ok(out)
    }

    fn as_mut_sequence(&mut self) -> &mut LV2AtomSequence {
        unsafe { &mut *(self.bytes.as_mut_ptr().cast::<LV2AtomSequence>()) }
    }
}

#[derive(Default)]
struct Lv2StateString {
    data: HashMap<String, (String, Vec<u8>)>,
    mapped_data: HashMap<LV2Urid, (LV2Urid, Vec<u8>)>,
}

impl Lv2StateString {
    fn serialize(&self, mapper: &UridMapper) -> Result<String> {
        let mut obj = serde_json::Map::new();
        for (key_uri, (type_uri, value)) in &self.data {
            obj.insert(
                key_uri.clone(),
                serde_json::json!({
                    "type": type_uri,
                    "value": base64::engine::general_purpose::STANDARD.encode(value),
                }),
            );
        }
        for (key, (value_type, value)) in &self.mapped_data {
            let key_uri = mapper
                .unmap(*key)
                .ok_or_else(|| anyhow!("No URI for saved LV2 state key URID {key}"))?;
            let type_uri = mapper
                .unmap(*value_type)
                .ok_or_else(|| anyhow!("No URI for saved LV2 state type URID {value_type}"))?;
            obj.insert(
                key_uri,
                serde_json::json!({
                    "type": type_uri,
                    "value": base64::engine::general_purpose::STANDARD.encode(value),
                }),
            );
        }
        Ok(serde_json::Value::Object(obj).to_string())
    }

    fn deserialize(s: &str, mapper: &UridMapper) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(s)?;
        let obj = value
            .as_object()
            .ok_or_else(|| anyhow!("LV2 state string must be a JSON object"))?;
        let mut mapped_data = HashMap::new();
        for (key_uri, entry) in obj {
            let type_uri = entry
                .get("type")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("LV2 state entry {key_uri} is missing type"))?;
            let encoded = entry
                .get("value")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("LV2 state entry {key_uri} is missing value"))?;
            let data = base64::engine::general_purpose::STANDARD.decode(encoded)?;
            mapped_data.insert(mapper.map_str(key_uri), (mapper.map_str(type_uri), data));
        }
        Ok(Self {
            data: HashMap::new(),
            mapped_data,
        })
    }
}

extern "C" fn ui_write_ignored(
    _controller: lv2_raw::ui::LV2UIControllerRaw,
    _port_index: c_uint,
    _buffer_size: c_uint,
    _port_protocol: c_uint,
    _buffer: *const c_void,
) {
}

extern "C" fn external_ui_closed(controller: lv2_raw::ui::LV2UIControllerRaw) {
    if !controller.is_null() {
        let closed = unsafe { &*(controller.cast::<AtomicBool>()) };
        closed.store(true, Ordering::Relaxed);
    }
}

unsafe extern "C" fn lv2_state_store(
    handle: *mut c_void,
    key: u32,
    value: *const c_void,
    size: usize,
    value_type: u32,
    flags: u32,
) -> u32 {
    if handle.is_null() || value.is_null() {
        return LV2_STATE_ERR_NO_PROPERTY;
    }
    if (flags & LV2_STATE_IS_POD) == 0 || (flags & LV2_STATE_IS_PORTABLE) == 0 {
        return LV2_STATE_ERR_BAD_FLAGS;
    }
    let state = unsafe { &mut *(handle.cast::<Lv2StateString>()) };
    let data = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), size) }.to_vec();
    state.mapped_data.insert(key, (value_type, data));
    LV2_STATE_SUCCESS
}

unsafe extern "C" fn lv2_state_retrieve(
    handle: *mut c_void,
    key: u32,
    size: *mut usize,
    value_type: *mut u32,
    flags: *mut u32,
) -> *const c_void {
    if handle.is_null() {
        return std::ptr::null();
    }
    let state = unsafe { &mut *(handle.cast::<Lv2StateString>()) };
    let Some((ty, data)) = state.mapped_data.get(&key) else {
        return std::ptr::null();
    };
    if !size.is_null() {
        unsafe { *size = data.len() };
    }
    if !value_type.is_null() {
        unsafe { *value_type = *ty };
    }
    if !flags.is_null() {
        unsafe { *flags = LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE };
    }
    data.as_ptr().cast::<c_void>()
}

struct UridMapper {
    by_uri: Mutex<HashMap<String, LV2Urid>>,
    by_id: Mutex<HashMap<LV2Urid, CString>>,
}

impl UridMapper {
    fn new() -> Self {
        let uri = cstr_bytes_to_string(LV2_ATOM__SEQUENCE);
        let mut by_uri = HashMap::new();
        by_uri.insert(uri.clone(), 1);
        let mut by_id = HashMap::new();
        by_id.insert(1, CString::new(uri).expect("static URI contains no nul"));
        Self {
            by_uri: Mutex::new(by_uri),
            by_id: Mutex::new(by_id),
        }
    }

    fn map_str(&self, uri: &str) -> LV2Urid {
        let mut by_uri = self.by_uri.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = by_uri.get(uri) {
            *id
        } else {
            let id = by_uri.len() as LV2Urid + 1;
            by_uri.insert(uri.to_string(), id);
            drop(by_uri);
            self.by_id
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id, CString::new(uri).unwrap_or_default());
            id
        }
    }

    fn unmap(&self, urid: LV2Urid) -> Option<String> {
        self.by_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&urid)
            .map(|s| s.to_string_lossy().to_string())
    }
}

extern "C" fn map_urid(handle: LV2UridMapHandle, uri: *const c_char) -> LV2Urid {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    let mapper = unsafe { &*(handle.cast::<UridMapper>()) };
    let uri = unsafe { CStr::from_ptr(uri) }.to_string_lossy();
    mapper.map_str(&uri)
}

extern "C" fn unmap_urid(handle: LV2UridMapHandle, urid: LV2Urid) -> *const c_char {
    if handle.is_null() {
        return std::ptr::null();
    }
    let mapper = unsafe { &*(handle.cast::<UridMapper>()) };
    mapper
        .by_id
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&urid)
        .map(|s| s.as_ptr())
        .unwrap_or(std::ptr::null())
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
        uri: ui
            .uri()
            .as_uri()
            .ok_or_else(|| anyhow!("Carla LV2 UI has no URI"))?
            .to_string(),
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
        let state = host.save_state_string().expect("save Carla LV2 state");
        assert!(state.starts_with('{'));
        host.restore_state_string(&state)
            .expect("restore Carla LV2 state");
    }
}
