//! Direct hosting of Carla Rack and Patchbay through the Carla Native C ABI.

use crate::carla_processor::{
    CarlaMidiBuffer, CarlaProcessor, CarlaProcessorInfo, ProcessorLatencyDiagnostic,
    ProcessorLatencyObservation,
};
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
type LatencyAdapterVersion = unsafe extern "C" fn() -> u32;
type QueryNativeLatency = unsafe extern "C" fn(
    *const NativePluginDescriptor,
    NativePluginHandle,
    *mut ShoopCarlaLatencyResult,
) -> bool;

const SHOOP_CARLA_LATENCY_ABI_VERSION: u32 = 1;
const SHOOP_CARLA_LATENCY_EXACT: u32 = 0;
const SHOOP_CARLA_LATENCY_RANGE: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct ShoopCarlaLatencyResult {
    struct_size: u32,
    abi_version: u32,
    status: u32,
    minimum_frames: u32,
    maximum_frames: u32,
    path_count: u32,
}

#[derive(Clone, Copy)]
enum LatencyAdapter {
    Available(QueryNativeLatency),
    Missing,
    VersionMismatch(u32),
}

#[cfg(not(target_os = "windows"))]
unsafe extern "C" fn process_state_free(pointer: *mut c_void) {
    libc::free(pointer);
}

struct CarlaRuntime {
    library_path: PathBuf,
    _library: Library,
    _state_allocator_library: Option<Library>,
    state_free: StateFree,
    latency_adapter: LatencyAdapter,
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

#[cfg(target_os = "windows")]
fn windows_process_path(path: PathBuf) -> Result<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let text = path.to_string_lossy();
    let path = text
        .strip_prefix(r"\\?\")
        .map(PathBuf::from)
        .unwrap_or(path);
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(long_path: *const u16, short_path: *mut u16, capacity: u32) -> u32;
    }
    let mut long: Vec<u16> = path.as_os_str().encode_wide().collect();
    long.push(0);
    let required = unsafe { GetShortPathNameW(long.as_ptr(), std::ptr::null_mut(), 0) };
    if required == 0 {
        return Ok(path);
    }
    let mut short = vec![0_u16; required as usize];
    let written = unsafe { GetShortPathNameW(long.as_ptr(), short.as_mut_ptr(), required) };
    if written == 0 || written >= required {
        bail!(
            "could not resolve Windows short path for {}",
            path.display()
        );
    }
    short.truncate(written as usize);
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&short)))
}

#[cfg(not(target_os = "windows"))]
fn windows_process_path(path: PathBuf) -> Result<PathBuf> {
    Ok(path)
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
            let resource_dir = windows_process_path(resource_dir_for(&canonical)?)?;
            let library_parent = canonical
                .parent()
                .ok_or_else(|| anyhow!("Carla library has no parent directory"))?;
            let component_binary_dir = library_parent.join("../bin");
            let binary_dir = windows_process_path(if component_binary_dir.is_dir() {
                component_binary_dir.canonicalize()?
            } else {
                library_parent.to_owned()
            })?;
            let library = unsafe { Library::new(&canonical) }
                .with_context(|| format!("loading {}", canonical.display()))?;
            let latency_adapter = unsafe {
                let version = library
                    .get::<LatencyAdapterVersion>(b"shoop_carla_latency_adapter_version\0")
                    .ok()
                    .map(|symbol| *symbol);
                let query = library
                    .get::<QueryNativeLatency>(b"shoop_carla_query_native_latency\0")
                    .ok()
                    .map(|symbol| *symbol);
                match (version, query) {
                    (Some(version), Some(query)) => {
                        let version = version();
                        if version == SHOOP_CARLA_LATENCY_ABI_VERSION {
                            LatencyAdapter::Available(query)
                        } else {
                            LatencyAdapter::VersionMismatch(version)
                        }
                    }
                    _ => LatencyAdapter::Missing,
                }
            };
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
                latency_adapter,
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

pub fn encode_carla_project_state(chain_type: FXChainType, bytes: &[u8]) -> Result<String> {
    encode_state(chain_type, bytes)
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
    latency: ProcessorLatencyObservation,
    latency_diagnostic: ProcessorLatencyDiagnostic,
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
        let mut host = Self {
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
            latency: ProcessorLatencyObservation::unknown(sample_rate, 1),
            latency_diagnostic: ProcessorLatencyDiagnostic::Unsupported,
            active: false,
        };
        host.refresh_latency();
        Ok(host)
    }

    fn refresh_latency(&mut self) {
        let query = match self.runtime.latency_adapter {
            LatencyAdapter::Available(query) => query,
            LatencyAdapter::Missing => {
                self.latency_diagnostic = ProcessorLatencyDiagnostic::Unsupported;
                self.latency = ProcessorLatencyObservation::unknown(
                    self.host_context.sample_rate as u32,
                    self.latency.revision,
                );
                return;
            }
            LatencyAdapter::VersionMismatch(version) => {
                let _reported_version = version;
                self.latency_diagnostic = ProcessorLatencyDiagnostic::VersionMismatch;
                self.latency = ProcessorLatencyObservation::unknown(
                    self.host_context.sample_rate as u32,
                    self.latency.revision,
                );
                return;
            }
        };
        let mut result = ShoopCarlaLatencyResult {
            struct_size: std::mem::size_of::<ShoopCarlaLatencyResult>() as u32,
            ..Default::default()
        };
        if !unsafe { query(self.descriptor.as_ptr(), self.handle.as_ptr(), &mut result) }
            || result.abi_version != SHOOP_CARLA_LATENCY_ABI_VERSION
        {
            self.latency_diagnostic = ProcessorLatencyDiagnostic::VersionMismatch;
            return;
        }
        let certainty = match result.status {
            SHOOP_CARLA_LATENCY_EXACT if result.minimum_frames == result.maximum_frames => {
                shoop_latency::LatencyCertainty::Exact
            }
            SHOOP_CARLA_LATENCY_RANGE if result.minimum_frames <= result.maximum_frames => {
                shoop_latency::LatencyCertainty::Range
            }
            _ => {
                self.latency_diagnostic = ProcessorLatencyDiagnostic::Unsupported;
                self.latency = ProcessorLatencyObservation::unknown(
                    self.host_context.sample_rate as u32,
                    self.latency.revision,
                );
                return;
            }
        };
        let changed = self.latency.range.is_none_or(|range| {
            range.min() != result.minimum_frames || range.max() != result.maximum_frames
        }) || self.latency.certainty != certainty;
        let revision = if changed {
            self.latency.revision.saturating_add(1)
        } else {
            self.latency.revision
        };
        self.latency = ProcessorLatencyObservation::new(
            shoop_latency::LatencyRangeFrames::new(result.minimum_frames, result.maximum_frames)
                .ok(),
            certainty,
            self.host_context.sample_rate as u32,
            revision,
        )
        .unwrap_or_else(|_| {
            ProcessorLatencyObservation::unknown(self.host_context.sample_rate as u32, revision)
        });
        self.latency_diagnostic = match self.info.chain_type {
            FXChainType::CarlaRack => ProcessorLatencyDiagnostic::CarlaRackAggregate,
            FXChainType::CarlaPatchbay | FXChainType::CarlaPatchbay16x => {
                ProcessorLatencyDiagnostic::CarlaPatchbayGraphRange
            }
            _ => ProcessorLatencyDiagnostic::Unsupported,
        };
    }

    pub fn latency_diagnostic(&self) -> ProcessorLatencyDiagnostic {
        self.latency_diagnostic
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

    fn latency(&self) -> ProcessorLatencyObservation {
        self.latency
    }

    fn latency_diagnostic(&self) -> ProcessorLatencyDiagnostic {
        self.latency_diagnostic
    }

    fn idle(&mut self) {
        CarlaNativeHost::idle(self);
        self.refresh_latency();
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
        self.refresh_latency();
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
        self.refresh_latency();
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

    #[shoop_wasm_test_support::shoop_test]
    fn runtime_overrides_must_be_absolute() {
        assert!(absolute_override("TEST", "relative/library.so".into()).is_err());
        let absolute = if cfg!(target_os = "windows") {
            std::ffi::OsString::from(r"C:\carla\library.dll")
        } else {
            std::ffi::OsString::from("/carla/library.so")
        };
        assert!(absolute_override("TEST", absolute).is_ok());
    }

    #[cfg(target_os = "windows")]
    #[shoop_wasm_test_support::shoop_test]
    fn windows_process_paths_avoid_verbatim_prefixes_rejected_by_carla_helpers() {
        assert_eq!(
            windows_process_path(PathBuf::from(r"\\?\D:\Shoop\carla-runtime\resources")).unwrap(),
            PathBuf::from(r"D:\Shoop\carla-runtime\resources")
        );
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn pinned_adapter_reports_rack_and_reachable_patchbay_paths() {
        let _exclusive = lock_carla_test();
        let Ok(rack) = CarlaNativeHost::instantiate(FXChainType::CarlaRack, 48_000, 64) else {
            eprintln!("skipping Carla latency adapter test; runtime unavailable");
            return;
        };
        if rack.latency_diagnostic() == ProcessorLatencyDiagnostic::Unsupported {
            assert!(rack.latency().range.is_none());
            eprintln!("skipping Carla latency adapter assertions; installed runtime is unpatched");
            return;
        }
        assert_eq!(
            rack.latency_diagnostic(),
            ProcessorLatencyDiagnostic::CarlaRackAggregate
        );
        assert_eq!(rack.latency().range.unwrap().min(), 0);
        assert_eq!(rack.latency().range.unwrap().max(), 0);
        drop(rack);

        for chain_type in [FXChainType::CarlaPatchbay, FXChainType::CarlaPatchbay16x] {
            let host = CarlaNativeHost::instantiate(chain_type, 48_000, 64).unwrap();
            assert_eq!(
                host.latency_diagnostic(),
                ProcessorLatencyDiagnostic::CarlaPatchbayGraphRange
            );
            assert_eq!(host.latency().range.unwrap().min(), 0);
            assert_eq!(host.latency().range.unwrap().max(), 0);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
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
            host.idle();
            let latency = host.latency();
            if host.latency_diagnostic() == ProcessorLatencyDiagnostic::Unsupported {
                assert!(latency.range.is_none());
            } else {
                assert_eq!(latency.range.unwrap().min(), 0);
                assert_eq!(latency.range.unwrap().max(), 0);
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

    #[shoop_wasm_test_support::shoop_test]
    fn real_nonzero_rack_and_branched_patchbay_latency_match_impulse_paths() {
        let explicit = std::env::var_os("SHOOP_CARLA_NONZERO_RACK_STATE_XML")
            .zip(std::env::var_os("SHOOP_CARLA_BRANCHED_PATCHBAY_STATE_XML"));
        let (rack_xml, patchbay_xml) = if let Some((rack_path, patchbay_path)) = explicit {
            (
                std::fs::read(rack_path).expect("read nonzero Rack Carla XML"),
                std::fs::read(patchbay_path).expect("read branched Patchbay Carla XML"),
            )
        } else if let Some(binary) = std::env::var_os("SHOOP_CARLA_NONZERO_PLUGIN_BINARY") {
            let binary = binary.to_string_lossy();
            let plugin = format!(
                r#"
 <Plugin>
  <Info>
   <Type>LADSPA</Type>
   <Name>Rubber Band Live Mono Pitch Shifter</Name>
   <Binary>{binary}</Binary>
   <Label>rubberband-live-pitchshifter-mono</Label>
  </Info>
  <Data><Active>Yes</Active><ControlChannel>1</ControlChannel><Options>0x0</Options></Data>
 </Plugin>
"#,
            );
            let rack = format!(
                r#"<?xml version='1.0' encoding='UTF-8'?>
<!DOCTYPE CARLA-PROJECT>
<CARLA-PROJECT VERSION='2.5'>
 <EngineSettings><ForceStereo>true</ForceStereo><PreferPluginBridges>false</PreferPluginBridges><PreferUiBridges>false</PreferUiBridges><UIsAlwaysOnTop>false</UIsAlwaysOnTop><MaxParameters>200</MaxParameters><UIBridgesTimeout>4000</UIBridgesTimeout></EngineSettings>
{plugin}</CARLA-PROJECT>
"#,
            );
            let fixture = include_str!("../test_data/carla_legacy_patchbay_loaded_state.json");
            let encoded = decode_state(fixture, FXChainType::CarlaPatchbay).unwrap();
            let mut patchbay = String::from_utf8(encoded).unwrap();
            patchbay = patchbay.replace("\n <Patchbay>", &format!("{plugin}\n <Patchbay>"));
            patchbay = patchbay.replace(
                "<Target>Audio Gain (Stereo):input_2</Target>",
                "<Target>Rubber Band Live Mono Pitch Shifter:Input</Target>",
            );
            patchbay = patchbay.replace(
                "<Source>Audio Gain (Stereo):output_2</Source>\n   <Target>Audio Output:Right</Target>",
                "<Source>Rubber Band Live Mono Pitch Shifter:Output</Source>\n   <Target>Audio Output:Right</Target>",
            );
            (rack.into_bytes(), patchbay.into_bytes())
        } else {
            eprintln!(
                "skipping real nonzero Carla latency test; provide XML fixtures or SHOOP_CARLA_NONZERO_PLUGIN_BINARY"
            );
            return;
        };
        let _exclusive = lock_carla_test();

        let mut rack = CarlaNativeHost::instantiate(FXChainType::CarlaRack, 48_000, 64).unwrap();
        rack.restore_state(&encode_state(FXChainType::CarlaRack, &rack_xml).unwrap())
            .unwrap();
        rack.set_active(true);
        let mut rack_latency = None;
        for _ in 0..100 {
            rack.process(64).unwrap();
            rack.idle();
            if rack.latency().range.is_some_and(|range| range.max() > 0) {
                rack_latency = rack.latency().range;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let rack_latency = rack_latency.unwrap_or_else(|| {
            let state = rack
                .save_state()
                .unwrap_or_else(|_| "unavailable".to_owned());
            panic!(
                "nonzero Rack plugin did not report latency; diagnostic={:?}; state={}",
                rack.latency_diagnostic(),
                decode_state(&state, FXChainType::CarlaRack)
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_else(|_| state)
            )
        });
        assert_eq!(rack_latency.min(), rack_latency.max());
        let expected = rack_latency.max();
        let mut peak = (0.0_f32, 0_u32);
        let blocks = expected.div_ceil(64) + 8;
        for block in 0..blocks {
            for channel in 0..rack.info().audio_inputs {
                rack.audio_input_mut(channel).unwrap()[..64].fill(0.0);
            }
            if block == 0 {
                rack.audio_input_mut(0).unwrap()[0] = 1.0;
            }
            rack.process(64).unwrap();
            for (offset, sample) in rack.audio_output(0).unwrap()[..64].iter().enumerate() {
                if sample.abs() > peak.0 {
                    peak = (sample.abs(), block * 64 + offset as u32);
                }
            }
        }
        assert!(peak.0 > 1.0e-6);
        assert_eq!(peak.1, expected);
        drop(rack);

        let mut patchbay =
            CarlaNativeHost::instantiate(FXChainType::CarlaPatchbay, 48_000, 64).unwrap();
        patchbay
            .restore_state(&encode_state(FXChainType::CarlaPatchbay, &patchbay_xml).unwrap())
            .unwrap();
        patchbay.set_active(true);
        let mut range = None;
        for _ in 0..100 {
            patchbay.process(64).unwrap();
            patchbay.idle();
            if patchbay
                .latency()
                .range
                .is_some_and(|value| value.max() > 0)
            {
                range = patchbay.latency().range;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let range = range.expect("branched Patchbay did not report reachable latency paths");
        assert_eq!(range.min(), 0);
        assert_eq!(range.max(), expected);
        let mut routing_ready = false;
        for _ in 0..100 {
            for channel in 0..patchbay.info().audio_inputs {
                patchbay.audio_input_mut(channel).unwrap()[..64].fill(0.25);
            }
            patchbay.process(64).unwrap();
            if patchbay.audio_output(0).unwrap()[..64]
                .iter()
                .any(|sample| sample.abs() > 0.1)
                && patchbay.audio_output(1).unwrap()[..64]
                    .iter()
                    .any(|sample| sample.abs() > 0.01)
            {
                routing_ready = true;
                break;
            }
            patchbay.idle();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            routing_ready,
            "branched Patchbay routes did not become active"
        );
        for _ in 0..(expected.div_ceil(64) + 4) {
            for channel in 0..patchbay.info().audio_inputs {
                patchbay.audio_input_mut(channel).unwrap()[..64].fill(0.0);
            }
            patchbay.process(64).unwrap();
        }

        let mut zero_onset = None;
        let mut delayed_peak = (0.0_f32, 0_u32);
        for block in 0..(expected.div_ceil(64) + 8) {
            for channel in 0..patchbay.info().audio_inputs {
                patchbay.audio_input_mut(channel).unwrap()[..64].fill(0.0);
                if block == 0 {
                    patchbay.audio_input_mut(channel).unwrap()[0] = 1.0;
                }
            }
            patchbay.process(64).unwrap();
            if zero_onset.is_none() {
                zero_onset = patchbay.audio_output(0).unwrap()[..64]
                    .iter()
                    .position(|sample| sample.abs() > 1.0e-6)
                    .map(|offset| block * 64 + offset as u32);
            }
            for (offset, sample) in patchbay.audio_output(1).unwrap()[..64].iter().enumerate() {
                if sample.abs() > delayed_peak.0 {
                    delayed_peak = (sample.abs(), block * 64 + offset as u32);
                }
            }
        }
        assert_eq!(zero_onset, Some(0));
        assert!(delayed_peak.0 > 1.0e-6);
        assert_eq!(delayed_peak.1, expected);
    }
}
