//! Direct hosting of Carla Rack and Patchbay through the Carla Native C ABI.

use crate::carla_processor::{CarlaMidiBuffer, CarlaProcessor, CarlaProcessorInfo};
use crate::FXChainType;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

#[cfg(test)]
use crate::realtime_lock_guard::Mutex;

#[cfg(test)]
static CARLA_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn lock_carla_test() -> impl Drop {
    CARLA_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub const CARLA_MAX_BUFFER_SIZE: usize = shoop_plugin_protocol::MAX_BLOCK_FRAMES;
pub const CARLA_MIDI_BUFFER_CAPACITY: usize = shoop_plugin_protocol::MAX_MIDI_EVENTS_PER_BLOCK;
const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
const STATE_PREFIX_V1: &str = "shoop-carla-native-state:1:";
const STATE_PREFIX_V2: &str = "shoop-carla-native-state:2:";
const LEGACY_CHUNK_URI: &str = "http://kxstudio.sf.net/ns/carla/chunk";
const LEGACY_ATOM_STRING_URI: &str = "http://lv2plug.in/ns/ext/atom#String";
const NATIVE_PLUGIN_HAS_UI: c_int = 1 << 2;
const NATIVE_PLUGIN_USES_STATE: c_int = 1 << 9;
const NATIVE_HOST_OPCODE_UI_UNAVAILABLE: c_int = 6;
const NATIVE_HOST_OPCODE_INTERNAL_PLUGIN: c_int = 8;
const NATIVE_PLUGIN_OPCODE_HOST_OPTION: c_int = 9;
const ENGINE_OPTION_PATH_BINARIES: i32 = 20;

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeParameterScalePoint {
    label: *const c_char,
    value: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeParameterRanges {
    default_value: f32,
    min: f32,
    max: f32,
    step: f32,
    step_small: f32,
    step_large: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeParameter {
    hints: c_int,
    name: *const c_char,
    unit: *const c_char,
    ranges: NativeParameterRanges,
    scale_point_count: u32,
    scale_points: *const NativeParameterScalePoint,
    comment: *const c_char,
    group_name: *const c_char,
    designation: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeMidiProgram {
    bank: u32,
    program: u32,
    name: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct NativeMidiEvent {
    time: u32,
    port: u8,
    size: u8,
    data: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeTimeInfoBbt {
    valid: bool,
    bar: i32,
    beat: i32,
    tick: f64,
    bar_start_tick: f64,
    beats_per_bar: f32,
    beat_type: f32,
    ticks_per_beat: f64,
    beats_per_minute: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeTimeInfo {
    playing: bool,
    frame: u64,
    usecs: u64,
    bbt: NativeTimeInfoBbt,
}

#[repr(C)]
struct NativeInlineDisplayImageSurface {
    data: *mut u8,
    width: c_int,
    height: c_int,
    stride: c_int,
}

#[repr(C)]
struct NativePortRange {
    minimum: f32,
    maximum: f32,
}

type NativeHostHandle = *mut c_void;
type NativePluginHandle = *mut c_void;

#[repr(C)]
struct NativeHostDescriptor {
    handle: NativeHostHandle,
    resource_dir: *const c_char,
    ui_name: *const c_char,
    ui_parent_id: usize,
    get_buffer_size: Option<unsafe extern "C" fn(NativeHostHandle) -> u32>,
    get_sample_rate: Option<unsafe extern "C" fn(NativeHostHandle) -> f64>,
    is_offline: Option<unsafe extern "C" fn(NativeHostHandle) -> bool>,
    get_time_info: Option<unsafe extern "C" fn(NativeHostHandle) -> *const NativeTimeInfo>,
    write_midi_event:
        Option<unsafe extern "C" fn(NativeHostHandle, *const NativeMidiEvent) -> bool>,
    ui_parameter_changed: Option<unsafe extern "C" fn(NativeHostHandle, u32, f32)>,
    ui_midi_program_changed: Option<unsafe extern "C" fn(NativeHostHandle, u8, u32, u32)>,
    ui_custom_data_changed:
        Option<unsafe extern "C" fn(NativeHostHandle, *const c_char, *const c_char)>,
    ui_closed: Option<unsafe extern "C" fn(NativeHostHandle)>,
    ui_open_file: Option<
        unsafe extern "C" fn(NativeHostHandle, bool, *const c_char, *const c_char) -> *const c_char,
    >,
    ui_save_file: Option<
        unsafe extern "C" fn(NativeHostHandle, bool, *const c_char, *const c_char) -> *const c_char,
    >,
    dispatcher: Option<
        unsafe extern "C" fn(NativeHostHandle, c_int, i32, isize, *mut c_void, f32) -> isize,
    >,
}

#[repr(C)]
struct NativePluginDescriptor {
    category: c_int,
    hints: c_int,
    supports: c_int,
    audio_ins: u32,
    audio_outs: u32,
    midi_ins: u32,
    midi_outs: u32,
    param_ins: u32,
    param_outs: u32,
    name: *const c_char,
    label: *const c_char,
    maker: *const c_char,
    copyright: *const c_char,
    instantiate: Option<unsafe extern "C" fn(*const NativeHostDescriptor) -> NativePluginHandle>,
    cleanup: Option<unsafe extern "C" fn(NativePluginHandle)>,
    get_parameter_count: Option<unsafe extern "C" fn(NativePluginHandle) -> u32>,
    get_parameter_info:
        Option<unsafe extern "C" fn(NativePluginHandle, u32) -> *const NativeParameter>,
    get_parameter_value: Option<unsafe extern "C" fn(NativePluginHandle, u32) -> f32>,
    get_midi_program_count: Option<unsafe extern "C" fn(NativePluginHandle) -> u32>,
    get_midi_program_info:
        Option<unsafe extern "C" fn(NativePluginHandle, u32) -> *const NativeMidiProgram>,
    set_parameter_value: Option<unsafe extern "C" fn(NativePluginHandle, u32, f32)>,
    set_midi_program: Option<unsafe extern "C" fn(NativePluginHandle, u8, u32, u32)>,
    set_custom_data: Option<unsafe extern "C" fn(NativePluginHandle, *const c_char, *const c_char)>,
    ui_show: Option<unsafe extern "C" fn(NativePluginHandle, bool)>,
    ui_idle: Option<unsafe extern "C" fn(NativePluginHandle)>,
    ui_set_parameter_value: Option<unsafe extern "C" fn(NativePluginHandle, u32, f32)>,
    ui_set_midi_program: Option<unsafe extern "C" fn(NativePluginHandle, u8, u32, u32)>,
    ui_set_custom_data:
        Option<unsafe extern "C" fn(NativePluginHandle, *const c_char, *const c_char)>,
    activate: Option<unsafe extern "C" fn(NativePluginHandle)>,
    deactivate: Option<unsafe extern "C" fn(NativePluginHandle)>,
    process: Option<
        unsafe extern "C" fn(
            NativePluginHandle,
            *mut *mut f32,
            *mut *mut f32,
            u32,
            *const NativeMidiEvent,
            u32,
        ),
    >,
    get_state: Option<unsafe extern "C" fn(NativePluginHandle) -> *mut c_char>,
    set_state: Option<unsafe extern "C" fn(NativePluginHandle, *const c_char)>,
    dispatcher: Option<
        unsafe extern "C" fn(NativePluginHandle, c_int, i32, isize, *mut c_void, f32) -> isize,
    >,
    render_inline_display: Option<
        unsafe extern "C" fn(
            NativePluginHandle,
            u32,
            u32,
        ) -> *const NativeInlineDisplayImageSurface,
    >,
    cv_ins: u32,
    cv_outs: u32,
    get_buffer_port_name:
        Option<unsafe extern "C" fn(NativePluginHandle, u32, bool) -> *const c_char>,
    get_buffer_port_range:
        Option<unsafe extern "C" fn(NativePluginHandle, u32, bool) -> *const NativePortRange>,
    ui_width: u16,
    ui_height: u16,
}

type GetDescriptor = unsafe extern "C" fn() -> *const NativePluginDescriptor;
type StateFree = unsafe extern "C" fn(*mut c_void);

#[cfg(not(target_os = "windows"))]
unsafe extern "C" fn process_state_free(pointer: *mut c_void) {
    libc::free(pointer);
}

struct CarlaRuntime {
    library_path: PathBuf,
    _library: Library,
    _state_allocator_library: Option<Library>,
    state_free: StateFree,
    rack: NonNull<NativePluginDescriptor>,
    patchbay: NonNull<NativePluginDescriptor>,
    patchbay16: NonNull<NativePluginDescriptor>,
    resource_dir: PathBuf,
    binary_dir: PathBuf,
}

unsafe impl Send for CarlaRuntime {}
unsafe impl Sync for CarlaRuntime {}

impl std::fmt::Debug for CarlaRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CarlaRuntime")
            .field("library_path", &self.library_path)
            .field("resource_dir", &self.resource_dir)
            .field("binary_dir", &self.binary_dir)
            .finish_non_exhaustive()
    }
}

static RUNTIME: OnceLock<std::result::Result<Arc<CarlaRuntime>, String>> = OnceLock::new();

fn library_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "libcarla_native-plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libcarla_native-plugin.dylib"
    } else {
        "libcarla_native-plugin.so"
    }
}

fn absolute_override(name: &str, value: std::ffi::OsString) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        bail!("{name} must be an absolute path: {}", path.display());
    }
    Ok(path)
}

fn library_candidates() -> Result<Vec<PathBuf>> {
    if let Some(path) = std::env::var_os("SHOOP_CARLA_NATIVE_LIBRARY") {
        return Ok(vec![absolute_override("SHOOP_CARLA_NATIVE_LIBRARY", path)?]);
    }
    let mut paths = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            if cfg!(target_os = "macos") {
                paths.push(
                    parent
                        .join("../Frameworks/carla-runtime/lib")
                        .join(library_filename()),
                );
            }
            paths.push(parent.join("carla-runtime/lib").join(library_filename()));
            paths.push(parent.join("carla").join(library_filename()));
            paths.push(parent.join("lib/carla").join(library_filename()));
        }
    }
    if cfg!(target_os = "linux") {
        paths.extend(
            ["/usr/lib/carla", "/usr/lib64/carla", "/usr/local/lib/carla"]
                .into_iter()
                .map(|root| Path::new(root).join(library_filename())),
        );
    }
    Ok(paths)
}

fn resource_dir_for(library: &Path) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SHOOP_CARLA_RESOURCE_DIR") {
        let path = absolute_override("SHOOP_CARLA_RESOURCE_DIR", path)?;
        if path.is_dir() {
            return Ok(path);
        }
        bail!(
            "SHOOP_CARLA_RESOURCE_DIR is not a directory: {}",
            path.display()
        );
    }
    let parent = library
        .parent()
        .ok_or_else(|| anyhow!("Carla library has no parent directory"))?;
    let candidates = [
        parent.join("resources"),
        parent.join("../resources"),
        parent.join("../../share/carla/resources"),
        parent.join("../../Resources/carla"),
    ];
    candidates
        .into_iter()
        .find(|path| {
            path.join(if cfg!(target_os = "windows") {
                "carla-plugin.exe"
            } else {
                "carla-plugin"
            })
            .is_file()
        })
        .ok_or_else(|| {
            anyhow!(
                "could not locate Carla UI resources beside {}",
                library.display()
            )
        })
}

fn descriptor_label(descriptor: &NativePluginDescriptor) -> Result<&str> {
    if descriptor.label.is_null() {
        bail!("Carla descriptor has no label");
    }
    unsafe { CStr::from_ptr(descriptor.label) }
        .to_str()
        .context("Carla descriptor label is not UTF-8")
}

fn validate_descriptor(
    descriptor: NonNull<NativePluginDescriptor>,
    label: &str,
    channels: u32,
) -> Result<()> {
    let descriptor = unsafe { descriptor.as_ref() };
    if descriptor_label(descriptor)? != label
        || descriptor.audio_ins != channels
        || descriptor.audio_outs != channels
        || descriptor.midi_ins != 1
        || descriptor.midi_outs != 1
    {
        bail!("Carla descriptor {label} has an incompatible port layout");
    }
    if descriptor.hints & (NATIVE_PLUGIN_HAS_UI | NATIVE_PLUGIN_USES_STATE)
        != NATIVE_PLUGIN_HAS_UI | NATIVE_PLUGIN_USES_STATE
    {
        bail!("Carla descriptor {label} lacks required UI/state hints");
    }
    if descriptor.instantiate.is_none()
        || descriptor.cleanup.is_none()
        || descriptor.activate.is_none()
        || descriptor.deactivate.is_none()
        || descriptor.process.is_none()
        || descriptor.get_state.is_none()
        || descriptor.set_state.is_none()
        || descriptor.dispatcher.is_none()
        || descriptor.ui_show.is_none()
        || descriptor.ui_idle.is_none()
    {
        bail!("Carla descriptor {label} lacks required callbacks");
    }
    Ok(())
}

fn load_runtime() -> Result<Arc<CarlaRuntime>> {
    let candidates = library_candidates()?;
    let mut failures = Vec::new();
    for candidate in &candidates {
        let canonical = match candidate.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                failures.push(format!("{}: {error}", candidate.display()));
                continue;
            }
        };
        let loaded = (|| -> Result<CarlaRuntime> {
            let resource_dir = resource_dir_for(&canonical)?;
            let library_parent = canonical
                .parent()
                .ok_or_else(|| anyhow!("Carla library has no parent directory"))?;
            let component_binary_dir = library_parent.join("../bin");
            let binary_dir = if component_binary_dir.is_dir() {
                component_binary_dir.canonicalize()?
            } else {
                library_parent.to_owned()
            };
            let library = unsafe { Library::new(&canonical) }
                .with_context(|| format!("loading {}", canonical.display()))?;
            #[cfg(target_os = "windows")]
            let (state_allocator_library, state_free) = {
                // Carla's official Windows build allocates get_state() with
                // msvcrt.dll, while Rust uses the Universal CRT. Crossing
                // those heaps corrupts memory, so resolve Carla's allocator.
                let allocator = unsafe { Library::new("msvcrt.dll") }?;
                let free: libloading::Symbol<StateFree> = unsafe { allocator.get(b"free\0")? };
                let free = *free;
                (Some(allocator), free)
            };
            #[cfg(not(target_os = "windows"))]
            let (state_allocator_library, state_free) =
                (None::<Library>, process_state_free as StateFree);
            unsafe fn get(
                library: &Library,
                symbol: &[u8],
            ) -> Result<NonNull<NativePluginDescriptor>> {
                let getter: libloading::Symbol<GetDescriptor> = library.get(symbol)?;
                NonNull::new(getter().cast_mut()).ok_or_else(|| anyhow!("Carla descriptor is null"))
            }
            let rack = unsafe { get(&library, b"carla_get_native_rack_plugin\0")? };
            let patchbay = unsafe { get(&library, b"carla_get_native_patchbay_plugin\0")? };
            let patchbay16 = unsafe { get(&library, b"carla_get_native_patchbay16_plugin\0")? };
            validate_descriptor(rack, "carlarack", 2)?;
            validate_descriptor(patchbay, "carlapatchbay", 2)?;
            validate_descriptor(patchbay16, "carlapatchbay16", 16)?;
            Ok(CarlaRuntime {
                library_path: canonical.clone(),
                _library: library,
                _state_allocator_library: state_allocator_library,
                state_free,
                rack,
                patchbay,
                patchbay16,
                resource_dir,
                binary_dir,
            })
        })();
        match loaded {
            Ok(runtime) => return Ok(Arc::new(runtime)),
            Err(error) => failures.push(format!("{}: {error:#}", canonical.display())),
        }
    }
    bail!(
        "Carla Native runtime is unavailable; checked {}",
        if failures.is_empty() {
            "no candidate paths".to_owned()
        } else {
            failures.join("; ")
        }
    )
}

fn runtime() -> Result<Arc<CarlaRuntime>> {
    RUNTIME
        .get_or_init(|| load_runtime().map_err(|error| format!("{error:#}")))
        .clone()
        .map_err(anyhow::Error::msg)
}

pub fn carla_runtime_availability() -> std::result::Result<(), String> {
    runtime().map(|_| ()).map_err(|error| error.to_string())
}

pub fn carla_runtime_path() -> Result<PathBuf> {
    Ok(runtime()?.library_path.clone())
}

pub fn smoke_test_carla_runtime() -> Result<()> {
    for chain_type in [
        FXChainType::CarlaRack,
        FXChainType::CarlaPatchbay,
        FXChainType::CarlaPatchbay16x,
    ] {
        let mut host = CarlaNativeHost::instantiate(chain_type, 48_000, 64)?;
        host.set_active(true);
        host.process(64)?;
        let state = host.save_state()?;
        host.restore_state(&state)?;
        host.set_active(false);
    }
    Ok(())
}

pub fn smoke_test_carla_ui() -> Result<()> {
    for chain_type in [
        FXChainType::CarlaRack,
        FXChainType::CarlaPatchbay,
        FXChainType::CarlaPatchbay16x,
    ] {
        let mut host = CarlaNativeHost::instantiate(chain_type, 48_000, 64)?;
        for _ in 0..2 {
            host.set_visible(true)?;
            for _ in 0..20 {
                host.idle();
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            if !host.is_visible() {
                bail!("{chain_type:?} external UI closed during smoke test");
            }
            host.set_visible(false)?;
            if host.is_visible() {
                bail!("{chain_type:?} external UI remained visible after hide");
            }
        }
    }
    Ok(())
}

struct HostContext {
    sample_rate: f64,
    buffer_size: u32,
    time_info: NativeTimeInfo,
    midi_output: Vec<NativeMidiEvent>,
    midi_output_count: usize,
    process_frames: u32,
    file_dialog_result: Option<CString>,
    visible: AtomicBool,
}

unsafe fn context<'a>(handle: NativeHostHandle) -> Option<&'a mut HostContext> {
    (handle as *mut HostContext).as_mut()
}

unsafe extern "C" fn host_get_buffer_size(handle: NativeHostHandle) -> u32 {
    unsafe { context(handle) }
        .map(|host| host.buffer_size)
        .unwrap_or(1)
}

unsafe extern "C" fn host_get_sample_rate(handle: NativeHostHandle) -> f64 {
    unsafe { context(handle) }
        .map(|host| host.sample_rate)
        .unwrap_or(48_000.0)
}

unsafe extern "C" fn host_is_offline(_handle: NativeHostHandle) -> bool {
    false
}

unsafe extern "C" fn host_get_time_info(handle: NativeHostHandle) -> *const NativeTimeInfo {
    unsafe { context(handle) }
        .map(|host| &host.time_info as *const NativeTimeInfo)
        .unwrap_or(std::ptr::null())
}

unsafe extern "C" fn host_write_midi_event(
    handle: NativeHostHandle,
    event: *const NativeMidiEvent,
) -> bool {
    let (Some(host), Some(event)) = (unsafe { context(handle) }, unsafe { event.as_ref() }) else {
        return false;
    };
    if event.port != 0
        || event.size == 0
        || event.size > 4
        || event.time >= host.process_frames
        || host.midi_output_count >= host.midi_output.len()
    {
        return false;
    }
    host.midi_output[host.midi_output_count] = *event;
    host.midi_output_count += 1;
    true
}

unsafe extern "C" fn host_ui_parameter_changed(
    _handle: NativeHostHandle,
    _index: u32,
    _value: f32,
) {
}

unsafe extern "C" fn host_ui_midi_program_changed(
    _handle: NativeHostHandle,
    _channel: u8,
    _bank: u32,
    _program: u32,
) {
}

unsafe extern "C" fn host_ui_custom_data_changed(
    _handle: NativeHostHandle,
    _key: *const c_char,
    _value: *const c_char,
) {
}

unsafe extern "C" fn host_ui_closed(handle: NativeHostHandle) {
    if let Some(host) = unsafe { context(handle) } {
        host.visible.store(false, Ordering::Release);
    }
}

fn file_dialog_title(title: *const c_char) -> String {
    if title.is_null() {
        return "Select file".to_owned();
    }
    unsafe { CStr::from_ptr(title) }
        .to_string_lossy()
        .into_owned()
}

unsafe fn host_file_dialog(
    handle: NativeHostHandle,
    is_dir: bool,
    title: *const c_char,
    save: bool,
) -> *const c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let host = unsafe { context(handle) }?;
        host.file_dialog_result = None;
        let dialog = rfd::FileDialog::new().set_title(file_dialog_title(title));
        let selected = if is_dir {
            dialog.pick_folder()
        } else if save {
            dialog.save_file()
        } else {
            dialog.pick_file()
        }?;
        let selected = CString::new(selected.to_string_lossy().as_bytes()).ok()?;
        host.file_dialog_result = Some(selected);
        host.file_dialog_result.as_ref().map(|path| path.as_ptr())
    }));
    result.ok().flatten().unwrap_or(std::ptr::null())
}

unsafe extern "C" fn host_open_file_dialog(
    handle: NativeHostHandle,
    is_dir: bool,
    title: *const c_char,
    _filter: *const c_char,
) -> *const c_char {
    unsafe { host_file_dialog(handle, is_dir, title, false) }
}

unsafe extern "C" fn host_save_file_dialog(
    handle: NativeHostHandle,
    is_dir: bool,
    title: *const c_char,
    _filter: *const c_char,
) -> *const c_char {
    unsafe { host_file_dialog(handle, is_dir, title, true) }
}

unsafe extern "C" fn host_dispatcher(
    handle: NativeHostHandle,
    opcode: c_int,
    _index: i32,
    _value: isize,
    _pointer: *mut c_void,
    _option: f32,
) -> isize {
    match opcode {
        NATIVE_HOST_OPCODE_UI_UNAVAILABLE => {
            if let Some(host) = unsafe { context(handle) } {
                host.visible.store(false, Ordering::Release);
            }
            0
        }
        // Shoop does not provide a JUCE event loop; Carla must own its event servicing.
        NATIVE_HOST_OPCODE_INTERNAL_PLUGIN => 0,
        _ => 0,
    }
}

fn state_chain_label(chain_type: FXChainType) -> Result<&'static str> {
    match chain_type {
        FXChainType::CarlaRack => Ok("rack"),
        FXChainType::CarlaPatchbay => Ok("patchbay"),
        FXChainType::CarlaPatchbay16x => Ok("patchbay16"),
        _ => bail!("{chain_type:?} is not a Carla chain type"),
    }
}

fn encode_state(chain_type: FXChainType, bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_STATE_BYTES {
        bail!("Carla state exceeds {MAX_STATE_BYTES} bytes");
    }
    if bytes.contains(&0) {
        bail!("Carla state contains an interior NUL byte");
    }
    Ok(format!(
        "{STATE_PREFIX_V2}{}:{}",
        state_chain_label(chain_type)?,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn decode_native_state(encoded: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("invalid Carla Native state base64")
}

fn decode_state(state: &str, expected_chain_type: FXChainType) -> Result<Vec<u8>> {
    let decoded = if let Some(tagged) = state.strip_prefix(STATE_PREFIX_V2) {
        let (chain, encoded) = tagged
            .split_once(':')
            .ok_or_else(|| anyhow!("Carla Native state has no chain tag"))?;
        let expected = state_chain_label(expected_chain_type)?;
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
        let mut bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
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

pub struct CarlaNativeHost {
    runtime: Arc<CarlaRuntime>,
    descriptor: NonNull<NativePluginDescriptor>,
    handle: NonNull<c_void>,
    host_context: Box<HostContext>,
    _resource_dir: CString,
    _binary_dir: CString,
    _ui_name: CString,
    host_descriptor: Box<NativeHostDescriptor>,
    info: CarlaProcessorInfo,
    audio_inputs: Vec<Vec<f32>>,
    audio_outputs: Vec<Vec<f32>>,
    input_pointers: Vec<*mut f32>,
    output_pointers: Vec<*mut f32>,
    midi_inputs: Vec<NativeMidiEvent>,
    active: bool,
}

unsafe impl Send for CarlaNativeHost {}

impl std::fmt::Debug for CarlaNativeHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CarlaNativeHost")
            .field("info", &self.info)
            .field("active", &self.active)
            .field("resource_dir", &self.runtime.resource_dir)
            .finish_non_exhaustive()
    }
}

impl CarlaNativeHost {
    #[tracing::instrument(
        name = "engine.plugin.instantiate",
        skip_all,
        fields(chain_type = chain_type as u32, sample_rate, buffer_size)
    )]
    pub fn instantiate(
        chain_type: FXChainType,
        sample_rate: u32,
        buffer_size: u32,
    ) -> Result<Self> {
        let runtime = runtime()?;
        let (descriptor, channels) = match chain_type {
            FXChainType::CarlaRack => (runtime.rack, 2),
            FXChainType::CarlaPatchbay => (runtime.patchbay, 2),
            FXChainType::CarlaPatchbay16x => (runtime.patchbay16, 16),
            _ => bail!("{chain_type:?} is not a Carla chain type"),
        };
        let resource_dir = CString::new(runtime.resource_dir.to_string_lossy().as_bytes())?;
        let binary_dir = CString::new(runtime.binary_dir.to_string_lossy().as_bytes())?;
        let ui_name = CString::new("ShoopDaLoop Carla")?;
        let mut host_context = Box::new(HostContext {
            sample_rate: sample_rate.max(1) as f64,
            buffer_size: buffer_size.max(1),
            time_info: NativeTimeInfo::default(),
            midi_output: vec![NativeMidiEvent::default(); CARLA_MIDI_BUFFER_CAPACITY],
            midi_output_count: 0,
            process_frames: 0,
            file_dialog_result: None,
            visible: AtomicBool::new(false),
        });
        let mut host_descriptor = Box::new(NativeHostDescriptor {
            handle: (&mut *host_context as *mut HostContext).cast(),
            resource_dir: resource_dir.as_ptr(),
            ui_name: ui_name.as_ptr(),
            ui_parent_id: 0,
            get_buffer_size: Some(host_get_buffer_size),
            get_sample_rate: Some(host_get_sample_rate),
            is_offline: Some(host_is_offline),
            get_time_info: Some(host_get_time_info),
            write_midi_event: Some(host_write_midi_event),
            ui_parameter_changed: Some(host_ui_parameter_changed),
            ui_midi_program_changed: Some(host_ui_midi_program_changed),
            ui_custom_data_changed: Some(host_ui_custom_data_changed),
            ui_closed: Some(host_ui_closed),
            ui_open_file: Some(host_open_file_dialog),
            ui_save_file: Some(host_save_file_dialog),
            dispatcher: Some(host_dispatcher),
        });
        let instantiate = unsafe { descriptor.as_ref() }
            .instantiate
            .expect("validated Carla instantiate callback");
        let handle = NonNull::new(unsafe { instantiate(&*host_descriptor) })
            .ok_or_else(|| anyhow!("Carla Native plugin failed to instantiate"))?;
        if let Some(dispatcher) = unsafe { descriptor.as_ref() }.dispatcher {
            unsafe {
                dispatcher(
                    handle.as_ptr(),
                    NATIVE_PLUGIN_OPCODE_HOST_OPTION,
                    ENGINE_OPTION_PATH_BINARIES,
                    0,
                    binary_dir.as_ptr().cast_mut().cast(),
                    0.0,
                )
            };
        }
        // Carla retains the host descriptor pointer, so do not move its allocation.
        host_descriptor.handle = (&mut *host_context as *mut HostContext).cast();
        let info = CarlaProcessorInfo {
            chain_type,
            audio_inputs: channels,
            audio_outputs: channels,
            midi_inputs: 1,
            midi_outputs: 1,
        };
        let mut audio_inputs = vec![vec![0.0; CARLA_MAX_BUFFER_SIZE]; channels];
        let mut audio_outputs = vec![vec![0.0; CARLA_MAX_BUFFER_SIZE]; channels];
        let input_pointers = audio_inputs.iter_mut().map(Vec::as_mut_ptr).collect();
        let output_pointers = audio_outputs.iter_mut().map(Vec::as_mut_ptr).collect();
        Ok(Self {
            runtime,
            descriptor,
            handle,
            host_context,
            _resource_dir: resource_dir,
            _binary_dir: binary_dir,
            _ui_name: ui_name,
            host_descriptor,
            info,
            audio_inputs,
            audio_outputs,
            input_pointers,
            output_pointers,
            midi_inputs: Vec::with_capacity(CARLA_MIDI_BUFFER_CAPACITY),
            active: false,
        })
    }

    fn descriptor(&self) -> &NativePluginDescriptor {
        unsafe { self.descriptor.as_ref() }
    }

    fn idle(&mut self) {
        if let Some(idle) = self.descriptor().ui_idle {
            unsafe { idle(self.handle.as_ptr()) };
        }
    }
}

impl Drop for CarlaNativeHost {
    fn drop(&mut self) {
        let (show, deactivate, cleanup) = {
            let descriptor = self.descriptor();
            (
                descriptor.ui_show,
                descriptor.deactivate,
                descriptor.cleanup,
            )
        };
        if self.host_context.visible.swap(false, Ordering::AcqRel) {
            if let Some(show) = show {
                unsafe { show(self.handle.as_ptr(), false) };
            }
        }
        if self.active {
            if let Some(deactivate) = deactivate {
                unsafe { deactivate(self.handle.as_ptr()) };
            }
            self.active = false;
        }
        if let Some(cleanup) = cleanup {
            unsafe { cleanup(self.handle.as_ptr()) };
        }
        let _ = &self.host_descriptor;
    }
}

impl CarlaProcessor for CarlaNativeHost {
    fn info(&self) -> CarlaProcessorInfo {
        self.info
    }

    fn idle(&mut self) {
        CarlaNativeHost::idle(self);
    }

    fn set_active(&mut self, active: bool) {
        if active == self.active {
            return;
        }
        if active {
            if let Some(activate) = self.descriptor().activate {
                unsafe { activate(self.handle.as_ptr()) };
            }
        } else if let Some(deactivate) = self.descriptor().deactivate {
            unsafe { deactivate(self.handle.as_ptr()) };
        }
        self.active = active;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_visible(&mut self, visible: bool) -> Result<()> {
        let show = self
            .descriptor()
            .ui_show
            .ok_or_else(|| anyhow!("Carla Native plugin has no external UI"))?;
        self.host_context.visible.store(visible, Ordering::Release);
        unsafe { show(self.handle.as_ptr(), visible) };
        if visible && !self.host_context.visible.load(Ordering::Acquire) {
            bail!("Carla external UI is unavailable");
        }
        Ok(())
    }

    fn is_visible(&mut self) -> bool {
        self.host_context.visible.load(Ordering::Acquire)
    }

    #[tracing::instrument(name = "engine.plugin.save_state", skip_all)]
    fn save_state(&mut self) -> Result<String> {
        let get_state = self
            .descriptor()
            .get_state
            .ok_or_else(|| anyhow!("Carla Native plugin has no state getter"))?;
        let state = unsafe { get_state(self.handle.as_ptr()) };
        let state = NonNull::new(state).ok_or_else(|| anyhow!("Carla state save failed"))?;
        let bytes = unsafe { CStr::from_ptr(state.as_ptr()) }
            .to_bytes()
            .to_vec();
        unsafe { (self.runtime.state_free)(state.as_ptr().cast()) };
        encode_state(self.info.chain_type, &bytes)
    }

    #[tracing::instrument(
        name = "engine.plugin.restore_state",
        skip_all,
        fields(state_bytes = state.len())
    )]
    fn restore_state(&mut self, state: &str) -> Result<()> {
        let bytes = decode_state(state, self.info.chain_type)?;
        let state = CString::new(bytes)?;
        let set_state = self
            .descriptor()
            .set_state
            .ok_or_else(|| anyhow!("Carla Native plugin has no state setter"))?;
        unsafe { set_state(self.handle.as_ptr(), state.as_ptr()) };
        Ok(())
    }

    fn audio_input_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        self.audio_inputs.get_mut(index).map(Vec::as_mut_slice)
    }

    fn audio_output(&self, index: usize) -> Option<&[f32]> {
        self.audio_outputs.get(index).map(Vec::as_slice)
    }

    fn set_midi_input_events(&mut self, index: usize, events: &[(u32, &[u8])]) -> Result<()> {
        if index != 0 {
            bail!("No Carla MIDI input port {index}");
        }
        if events.len() > CARLA_MIDI_BUFFER_CAPACITY {
            bail!("Carla MIDI input event capacity exceeded");
        }
        self.midi_inputs.clear();
        for (time, bytes) in events {
            if bytes.is_empty() || bytes.len() > 4 {
                bail!("invalid Carla Native MIDI input event size");
            }
            let mut event = NativeMidiEvent {
                time: *time,
                port: 0,
                size: bytes.len() as u8,
                data: [0; 4],
            };
            event.data[..bytes.len()].copy_from_slice(bytes);
            self.midi_inputs.push(event);
        }
        Ok(())
    }

    fn midi_output_events(&mut self, index: usize) -> Result<Vec<(u32, Vec<u8>)>> {
        if index != 0 {
            bail!("No Carla MIDI output port {index}");
        }
        Ok(
            self.host_context.midi_output[..self.host_context.midi_output_count]
                .iter()
                .map(|event| {
                    (
                        event.time,
                        event.data[..event.size.min(4) as usize].to_vec(),
                    )
                })
                .collect(),
        )
    }

    fn fill_midi_output_events(
        &mut self,
        index: usize,
        destination: &mut CarlaMidiBuffer,
    ) -> Result<()> {
        if index != 0 {
            bail!("No Carla MIDI output port {index}");
        }
        destination.clear();
        for event in &self.host_context.midi_output[..self.host_context.midi_output_count] {
            destination.push(event.time, &event.data[..event.size.min(4) as usize])?;
        }
        Ok(())
    }

    fn process(&mut self, frames: usize) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        if frames == 0 || frames > CARLA_MAX_BUFFER_SIZE {
            bail!("Carla Native process block must be in 1..={CARLA_MAX_BUFFER_SIZE} frames");
        }
        if self
            .midi_inputs
            .iter()
            .any(|event| event.time as usize >= frames)
        {
            bail!("Carla MIDI event lies outside the process block");
        }
        self.host_context.midi_output_count = 0;
        self.host_context.process_frames = frames as u32;
        let process = self
            .descriptor()
            .process
            .ok_or_else(|| anyhow!("Carla Native plugin has no process callback"))?;
        let _span =
            shoop_tracing::realtime_span_detail!("engine.rt.fx.plugin_process", value = frames);
        unsafe {
            process(
                self.handle.as_ptr(),
                self.input_pointers.as_mut_ptr(),
                self.output_pointers.as_mut_ptr(),
                frames as u32,
                self.midi_inputs.as_ptr(),
                self.midi_inputs.len() as u32,
            )
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_overrides_must_be_absolute() {
        assert!(absolute_override("TEST", "relative/library.so".into()).is_err());
        let absolute = if cfg!(target_os = "windows") {
            std::ffi::OsString::from(r"C:\carla\library.dll")
        } else {
            std::ffi::OsString::from("/carla/library.so")
        };
        assert!(absolute_override("TEST", absolute).is_ok());
    }

    #[test]
    fn native_ffi_layout_matches_pinned_carla_header() {
        assert_eq!(std::mem::size_of::<NativeMidiEvent>(), 12);
        assert_eq!(std::mem::align_of::<NativeMidiEvent>(), 4);
        assert_eq!(std::mem::size_of::<NativeTimeInfo>(), 80);
        assert_eq!(std::mem::align_of::<NativeTimeInfo>(), 8);
        assert_eq!(std::mem::size_of::<NativeHostDescriptor>(), 128);
        assert_eq!(std::mem::align_of::<NativeHostDescriptor>(), 8);
        assert_eq!(std::mem::offset_of!(NativeHostDescriptor, dispatcher), 120);
        assert_eq!(std::mem::size_of::<NativePluginDescriptor>(), 280);
        assert_eq!(std::mem::align_of::<NativePluginDescriptor>(), 8);
        assert_eq!(
            std::mem::offset_of!(NativePluginDescriptor, instantiate),
            72
        );
        assert_eq!(std::mem::offset_of!(NativePluginDescriptor, process), 208);
        assert_eq!(std::mem::offset_of!(NativePluginDescriptor, get_state), 216);
        assert_eq!(std::mem::offset_of!(NativePluginDescriptor, ui_width), 272);
    }

    #[test]
    fn direct_state_codec_round_trips_and_rejects_invalid_input() {
        let encoded = encode_state(FXChainType::CarlaRack, b"<CARLA-PROJECT />").unwrap();
        assert_eq!(
            decode_state(&encoded, FXChainType::CarlaRack).unwrap(),
            b"<CARLA-PROJECT />"
        );
        let version_one = format!(
            "{STATE_PREFIX_V1}{}",
            base64::engine::general_purpose::STANDARD.encode(b"old direct state")
        );
        assert_eq!(
            decode_state(&version_one, FXChainType::CarlaRack).unwrap(),
            b"old direct state"
        );
        assert!(encode_state(FXChainType::CarlaRack, b"bad\0state").is_err());
        assert!(decode_state(&encoded, FXChainType::CarlaPatchbay).is_err());
        assert!(decode_state(
            "shoop-carla-native-state:2:rack:not-base64!",
            FXChainType::CarlaRack
        )
        .is_err());
        let oversized = format!(
            "{STATE_PREFIX_V2}rack:{}",
            base64::engine::general_purpose::STANDARD.encode(vec![0_u8; MAX_STATE_BYTES + 1])
        );
        assert!(decode_state(&oversized, FXChainType::CarlaRack).is_err());
    }

    #[test]
    fn legacy_lv2_chunks_for_every_descriptor_decode_to_native_state() {
        for (state, project, chain_type) in [
            (
                include_str!("../test_data/carla_legacy_rack_loaded_state.json"),
                include_bytes!("../test_data/carla_legacy_rack_loaded_project.xml").as_slice(),
                FXChainType::CarlaRack,
            ),
            (
                include_str!("../test_data/carla_legacy_patchbay_loaded_state.json"),
                include_bytes!("../test_data/carla_legacy_patchbay_loaded_project.xml").as_slice(),
                FXChainType::CarlaPatchbay,
            ),
            (
                include_str!("../test_data/carla_legacy_patchbay16_loaded_state.json"),
                include_bytes!("../test_data/carla_legacy_patchbay16_loaded_project.xml")
                    .as_slice(),
                FXChainType::CarlaPatchbay16x,
            ),
        ] {
            assert_eq!(decode_state(state, chain_type).unwrap(), project);
        }
    }

    #[test]
    fn legacy_state_codec_rejects_malformed_wrong_type_and_nul_contracts() {
        let wrong_type =
            r#"{"http://kxstudio.sf.net/ns/carla/chunk":{"type":"wrong","value":"AA=="}}"#;
        assert!(decode_state(wrong_type, FXChainType::CarlaRack).is_err());
        let no_trailing_nul = format!(
            r#"{{"{LEGACY_CHUNK_URI}":{{"type":"{LEGACY_ATOM_STRING_URI}","value":"{}"}}}}"#,
            base64::engine::general_purpose::STANDARD.encode(b"state")
        );
        assert!(decode_state(&no_trailing_nul, FXChainType::CarlaRack).is_err());
        let interior_nul = format!(
            r#"{{"{LEGACY_CHUNK_URI}":{{"type":"{LEGACY_ATOM_STRING_URI}","value":"{}"}}}}"#,
            base64::engine::general_purpose::STANDARD.encode(b"bad\0state\0")
        );
        assert!(decode_state(&interior_nul, FXChainType::CarlaRack).is_err());
    }

    #[test]
    fn shows_and_hides_carla_ui_when_opted_in() {
        if std::env::var_os("SHOOP_TEST_CARLA_UI").is_none() {
            eprintln!("skipping Carla UI smoke test; set SHOOP_TEST_CARLA_UI=1");
            return;
        }
        let _exclusive = lock_carla_test();
        smoke_test_carla_ui().expect("Carla runtime required for opted-in UI smoke test");
        let mut host = CarlaNativeHost::instantiate(FXChainType::CarlaRack, 48_000, 64).unwrap();
        host.set_visible(true).unwrap();
        unsafe { host_ui_closed(host.host_descriptor.handle) };
        assert!(!host.is_visible());
    }

    #[test]
    fn probes_and_runs_installed_carla_when_available() {
        let _exclusive = lock_carla_test();
        let fixtures = [
            (
                FXChainType::CarlaRack,
                include_str!("../test_data/carla_legacy_rack_loaded_state.json"),
            ),
            (
                FXChainType::CarlaPatchbay,
                include_str!("../test_data/carla_legacy_patchbay_loaded_state.json"),
            ),
            (
                FXChainType::CarlaPatchbay16x,
                include_str!("../test_data/carla_legacy_patchbay16_loaded_state.json"),
            ),
        ];
        for (chain_type, fixture) in fixtures {
            let mut host = match CarlaNativeHost::instantiate(chain_type, 48_000, 64) {
                Ok(host) => host,
                Err(error) if std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none() => {
                    eprintln!(
                        "skipping Carla Native runtime test; runtime is unavailable: {error:#}"
                    );
                    return;
                }
                Err(error) => panic!("required Carla Native runtime is unavailable: {error:#}"),
            };
            host.set_active(true);
            for channel in 0..host.info().audio_inputs {
                host.audio_input_mut(channel).unwrap()[..64].fill(0.25);
            }
            host.set_midi_input_events(0, &[(7, &[0x90, 60, 100])])
                .unwrap();
            host.process(64).unwrap();
            if chain_type == FXChainType::CarlaRack {
                assert_eq!(&host.audio_output(0).unwrap()[..64], &[0.25; 64]);
                assert_eq!(
                    host.midi_output_events(0).unwrap(),
                    vec![(7, vec![0x90, 60, 100])]
                );
            }

            host.restore_state(fixture).unwrap();
            host.set_active(false);
            host.set_active(true);
            let mut loaded_processed = false;
            for _ in 0..100 {
                for channel in 0..host.info().audio_inputs {
                    host.audio_input_mut(channel).unwrap()[..64].fill(0.25);
                }
                host.set_midi_input_events(0, &[(7, &[0x90, 60, 100])])
                    .unwrap();
                host.process(64).unwrap();
                if host.audio_output(0).unwrap()[..64]
                    .iter()
                    .any(|sample| sample.abs() > 0.1)
                    && host.midi_output_events(0).unwrap() == vec![(7, vec![0x90, 60, 100])]
                {
                    loaded_processed = true;
                    break;
                }
                host.idle();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let state = host.save_state().unwrap();
            assert!(
                loaded_processed,
                "{chain_type:?} loaded plugin routing did not process; state={}",
                String::from_utf8_lossy(&decode_state(&state, chain_type).unwrap())
            );
            let state_xml = String::from_utf8(decode_state(&state, chain_type).unwrap()).unwrap();
            assert!(state_xml.contains("<Label>audiogain_s</Label>"));
            assert!(state_xml.contains("<Label>midithrough</Label>"));
            host.restore_state(&state).unwrap();
        }
    }
}
