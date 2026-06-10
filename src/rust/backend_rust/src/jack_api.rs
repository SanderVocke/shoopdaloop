use anyhow::{anyhow, Result};
use jack_sys as jack;
use regex::Regex;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Mutex;

const PORT_DATA_TYPE_AUDIO: u32 = 0;
const PORT_DATA_TYPE_MIDI: u32 = 1;

const PORT_DIRECTION_INPUT: u32 = 0;
const PORT_DIRECTION_OUTPUT: u32 = 1;

pub trait JackApi: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn supports_processing(&self) -> bool;
    fn init(&self) -> Result<()>;
    fn client_open(&self, name: &str) -> Result<usize>;
    fn activate(&self, client: usize) -> i32;
    fn deactivate(&self, client: usize) -> i32;
    fn client_close(&self, client: usize) -> i32;
    fn get_client_name(&self, client: usize) -> String;
    fn get_sample_rate(&self, client: usize) -> u32;
    fn get_buffer_size(&self, client: usize) -> u32;
    fn cpu_load(&self, client: usize) -> f32;

    fn set_process_callback(&self, client: usize, driver_ptr: usize) -> i32;
    fn set_xrun_callback(&self, client: usize, driver_ptr: usize) -> i32;
    fn set_port_connect_callback(&self, client: usize, driver_ptr: usize) -> i32;
    fn set_port_registration_callback(&self, client: usize, driver_ptr: usize) -> i32;
    fn set_port_rename_callback(&self, client: usize, driver_ptr: usize) -> i32;
    fn set_error_info_logging(&self);

    fn port_register(&self, client: usize, name: &str, data_type: u32, direction: u32) -> usize;
    fn port_unregister(&self, client: usize, port: usize) -> i32;
    fn port_name(&self, port: usize) -> String;
    fn port_get_buffer(&self, port: usize, nframes: u32) -> usize;
    fn port_is_input(&self, port: usize) -> bool;
    fn port_is_output(&self, port: usize) -> bool;
    fn port_is_audio(&self, port: usize) -> bool;
    fn port_is_midi(&self, port: usize) -> bool;
    fn port_is_mine(&self, client: usize, port: usize) -> bool;
    fn get_ports(&self, client: usize) -> Vec<String>;
    fn port_by_name(&self, client: usize, name: &str) -> usize;
    fn port_get_all_connections(&self, client: usize, port: usize) -> Vec<String>;
    fn connect(&self, client: usize, src: &str, dst: &str) -> i32;
    fn disconnect(&self, client: usize, src: &str, dst: &str) -> i32;

    fn midi_get_event_count(&self, buffer: usize) -> u32;
    unsafe fn midi_event_get(
        &self,
        buffer: usize,
        index: u32,
        out_time: &mut u32,
        out_size: &mut u16,
        out_data: *mut u8,
    ) -> bool;
    fn midi_clear_buffer(&self, buffer: usize);
    unsafe fn midi_event_write(&self, buffer: usize, time: u32, size: u16, data: *const u8) -> i32;
}

pub struct JackApiObject {
    pub inner: Box<dyn JackApi>,
}

impl JackApiObject {
    pub fn new_real() -> Self {
        Self {
            inner: Box::new(RealJackApi),
        }
    }

    pub fn new_test() -> Self {
        Self {
            inner: Box::new(TestJackApi::new()),
        }
    }
}

fn cstring(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| anyhow!("string contains interior NUL byte"))
}

fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn real_client(client: usize) -> *mut jack::jack_client_t {
    client as *mut jack::jack_client_t
}

fn real_port(port: usize) -> *mut jack::jack_port_t {
    port as *mut jack::jack_port_t
}

unsafe extern "C" fn process_cb(nframes: jack::jack_nframes_t, arg: *mut c_void) -> c_int {
    crate::jack_api_cxx::ffi::jackapi_process_cb(arg as usize, nframes)
}

unsafe extern "C" fn xrun_cb(arg: *mut c_void) -> c_int {
    crate::jack_api_cxx::ffi::jackapi_xrun_cb(arg as usize)
}

unsafe extern "C" fn port_connect_cb(
    _a: jack::jack_port_id_t,
    _b: jack::jack_port_id_t,
    _connect: c_int,
    arg: *mut c_void,
) {
    crate::jack_api_cxx::ffi::jackapi_port_update_cb(arg as usize)
}

unsafe extern "C" fn port_registration_cb(
    _port: jack::jack_port_id_t,
    _registered: c_int,
    arg: *mut c_void,
) {
    crate::jack_api_cxx::ffi::jackapi_port_update_cb(arg as usize)
}

unsafe extern "C" fn port_rename_cb(
    _port: jack::jack_port_id_t,
    _old_name: *const c_char,
    _new_name: *const c_char,
    arg: *mut c_void,
) -> c_int {
    crate::jack_api_cxx::ffi::jackapi_port_update_cb(arg as usize);
    0
}

unsafe extern "C" fn error_cb(msg: *const c_char) {
    crate::jack_api_cxx::ffi::jackapi_error_log(cstr_to_string(msg).as_str())
}

unsafe extern "C" fn info_cb(msg: *const c_char) {
    crate::jack_api_cxx::ffi::jackapi_info_log(cstr_to_string(msg).as_str())
}

pub struct RealJackApi;

impl JackApi for RealJackApi {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn supports_processing(&self) -> bool {
        true
    }

    fn init(&self) -> Result<()> {
        jack::library()
            .map(|_| ())
            .map_err(|e| anyhow!("Unable to find JACK client library: {e}"))
    }

    fn client_open(&self, name: &str) -> Result<usize> {
        self.init()?;
        let name = cstring(name)?;
        let mut status: jack::jack_status_t = 0;
        let client = unsafe {
            jack::jack_client_open(name.as_ptr(), jack::JackNullOption, &mut status as *mut _)
        };
        if client.is_null() {
            Err(anyhow!("Unable to open JACK client (status {status})"))
        } else {
            Ok(client as usize)
        }
    }

    fn activate(&self, client: usize) -> i32 {
        unsafe { jack::jack_activate(real_client(client)) }
    }

    fn deactivate(&self, client: usize) -> i32 {
        unsafe { jack::jack_deactivate(real_client(client)) }
    }

    fn client_close(&self, client: usize) -> i32 {
        unsafe { jack::jack_client_close(real_client(client)) }
    }

    fn get_client_name(&self, client: usize) -> String {
        unsafe { cstr_to_string(jack::jack_get_client_name(real_client(client))) }
    }

    fn get_sample_rate(&self, client: usize) -> u32 {
        unsafe { jack::jack_get_sample_rate(real_client(client)) as u32 }
    }

    fn get_buffer_size(&self, client: usize) -> u32 {
        unsafe { jack::jack_get_buffer_size(real_client(client)) }
    }

    fn cpu_load(&self, client: usize) -> f32 {
        unsafe { jack::jack_cpu_load(real_client(client)) }
    }

    fn set_process_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        unsafe {
            jack::jack_set_process_callback(real_client(client), Some(process_cb), driver_ptr as *mut c_void)
        }
    }

    fn set_xrun_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        unsafe { jack::jack_set_xrun_callback(real_client(client), Some(xrun_cb), driver_ptr as *mut c_void) }
    }

    fn set_port_connect_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        unsafe {
            jack::jack_set_port_connect_callback(
                real_client(client),
                Some(port_connect_cb),
                driver_ptr as *mut c_void,
            )
        }
    }

    fn set_port_registration_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        unsafe {
            jack::jack_set_port_registration_callback(
                real_client(client),
                Some(port_registration_cb),
                driver_ptr as *mut c_void,
            )
        }
    }

    fn set_port_rename_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        unsafe {
            jack::jack_set_port_rename_callback(
                real_client(client),
                Some(port_rename_cb),
                driver_ptr as *mut c_void,
            )
        }
    }

    fn set_error_info_logging(&self) {
        unsafe {
            jack::jack_set_error_function(Some(error_cb));
            jack::jack_set_info_function(Some(info_cb));
        }
    }

    fn port_register(&self, client: usize, name: &str, data_type: u32, direction: u32) -> usize {
        let Ok(name) = cstring(name) else { return 0; };
        let port_type = match data_type {
            PORT_DATA_TYPE_AUDIO => jack::FLOAT_MONO_AUDIO,
            PORT_DATA_TYPE_MIDI => jack::RAW_MIDI_TYPE,
            _ => return 0,
        };
        let Ok(port_type) = cstring(port_type) else { return 0; };
        let flags = match direction {
            PORT_DIRECTION_INPUT => jack::JackPortIsInput as u64,
            PORT_DIRECTION_OUTPUT => jack::JackPortIsOutput as u64,
            _ => 0,
        };
        unsafe {
            jack::jack_port_register(
                real_client(client),
                name.as_ptr(),
                port_type.as_ptr(),
                flags as libc::c_ulong,
                0,
            ) as usize
        }
    }

    fn port_unregister(&self, client: usize, port: usize) -> i32 {
        unsafe { jack::jack_port_unregister(real_client(client), real_port(port)) }
    }

    fn port_name(&self, port: usize) -> String {
        unsafe { cstr_to_string(jack::jack_port_name(real_port(port))) }
    }

    fn port_get_buffer(&self, port: usize, nframes: u32) -> usize {
        unsafe { jack::jack_port_get_buffer(real_port(port), nframes) as usize }
    }

    fn port_is_input(&self, port: usize) -> bool {
        unsafe { jack::jack_port_flags(real_port(port)) & (jack::JackPortIsInput as i32) != 0 }
    }

    fn port_is_output(&self, port: usize) -> bool {
        unsafe { jack::jack_port_flags(real_port(port)) & (jack::JackPortIsOutput as i32) != 0 }
    }

    fn port_is_audio(&self, port: usize) -> bool {
        unsafe { cstr_to_string(jack::jack_port_type(real_port(port))) == jack::FLOAT_MONO_AUDIO }
    }

    fn port_is_midi(&self, port: usize) -> bool {
        unsafe { cstr_to_string(jack::jack_port_type(real_port(port))) == jack::RAW_MIDI_TYPE }
    }

    fn port_is_mine(&self, client: usize, port: usize) -> bool {
        unsafe { jack::jack_port_is_mine(real_client(client), real_port(port)) != 0 }
    }

    fn get_ports(&self, client: usize) -> Vec<String> {
        let names = unsafe { jack::jack_get_ports(real_client(client), std::ptr::null(), std::ptr::null(), 0) };
        if names.is_null() {
            return Vec::new();
        }
        let mut result = Vec::new();
        unsafe {
            let mut idx = 0;
            loop {
                let ptr = *names.add(idx);
                if ptr.is_null() {
                    break;
                }
                result.push(cstr_to_string(ptr));
                idx += 1;
            }
            jack::jack_free(names as *mut c_void);
        }
        result
    }

    fn port_by_name(&self, client: usize, name: &str) -> usize {
        let Ok(name) = cstring(name) else { return 0; };
        unsafe { jack::jack_port_by_name(real_client(client), name.as_ptr()) as usize }
    }

    fn port_get_all_connections(&self, client: usize, port: usize) -> Vec<String> {
        let names = unsafe { jack::jack_port_get_all_connections(real_client(client), real_port(port)) };
        if names.is_null() {
            return Vec::new();
        }
        let mut result = Vec::new();
        unsafe {
            let mut idx = 0;
            loop {
                let ptr = *names.add(idx);
                if ptr.is_null() {
                    break;
                }
                result.push(cstr_to_string(ptr));
                idx += 1;
            }
            jack::jack_free(names as *mut c_void);
        }
        result
    }

    fn connect(&self, client: usize, src: &str, dst: &str) -> i32 {
        let Ok(src) = cstring(src) else { return -1; };
        let Ok(dst) = cstring(dst) else { return -1; };
        unsafe { jack::jack_connect(real_client(client), src.as_ptr(), dst.as_ptr()) }
    }

    fn disconnect(&self, client: usize, src: &str, dst: &str) -> i32 {
        let Ok(src) = cstring(src) else { return -1; };
        let Ok(dst) = cstring(dst) else { return -1; };
        unsafe { jack::jack_disconnect(real_client(client), src.as_ptr(), dst.as_ptr()) }
    }

    fn midi_get_event_count(&self, buffer: usize) -> u32 {
        unsafe { jack::jack_midi_get_event_count(buffer as *mut c_void) }
    }

    unsafe fn midi_event_get(
        &self,
        buffer: usize,
        index: u32,
        out_time: &mut u32,
        out_size: &mut u16,
        out_data: *mut u8,
    ) -> bool {
        let mut event = jack::jack_midi_event_t::default();
        if jack::jack_midi_event_get(&mut event, buffer as *mut c_void, index) != 0 {
            return false;
        }
        *out_time = event.time;
        let size = std::cmp::min(event.size, 4);
        *out_size = size as u16;
        if !out_data.is_null() && !event.buffer.is_null() {
            std::ptr::copy_nonoverlapping(event.buffer as *const u8, out_data, size);
        }
        true
    }

    fn midi_clear_buffer(&self, buffer: usize) {
        unsafe { jack::jack_midi_clear_buffer(buffer as *mut c_void) }
    }

    unsafe fn midi_event_write(&self, buffer: usize, time: u32, size: u16, data: *const u8) -> i32 {
        let size = std::cmp::min(size as usize, 4);
        jack::jack_midi_event_write(
            buffer as *mut c_void,
            time,
            data as *const jack::jack_midi_data_t,
            size,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TestPortType {
    Audio,
    Midi,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TestDirection {
    Input,
    Output,
}

#[derive(Clone)]
struct MidiEvent {
    time: u32,
    bytes: Vec<u8>,
}

struct TestClient {
    name: String,
    active: bool,
    port_handles: Vec<usize>,
}

struct TestPort {
    name: String,
    client_handle: usize,
    port_type: TestPortType,
    direction: TestDirection,
    connections: HashSet<String>,
    audio_buffer: Vec<f32>,
    midi_buffer: Vec<MidiEvent>,
}

#[derive(Default)]
struct TestCallbackState {
    process_driver_ptr: Option<usize>,
    xrun_driver_ptr: Option<usize>,
    port_connect_driver_ptr: Option<usize>,
    port_registration_driver_ptr: Option<usize>,
    port_rename_driver_ptr: Option<usize>,
}

#[derive(Default)]
struct TestState {
    clients_by_handle: HashMap<usize, Box<TestClient>>,
    clients_by_name: HashMap<String, usize>,
    ports_by_handle: HashMap<usize, Box<TestPort>>,
    callbacks_by_client: HashMap<usize, TestCallbackState>,
}

pub struct TestJackApi {
    state: Mutex<TestState>,
}

impl TestJackApi {
    pub fn new() -> Self {
        let this = Self {
            state: Mutex::new(TestState::default()),
        };
        this.reset();
        this
    }

    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        *state = TestState::default();
        Self::populate_defaults(&mut state);
    }

    fn populate_defaults(state: &mut TestState) {
        let c1 = Self::add_client(state, "test_client_1".to_string());
        let c2 = Self::add_client(state, "test_client_2".to_string());
        for client in [c1, c2] {
            Self::add_port(state, client, "audio_in".to_string(), TestPortType::Audio, TestDirection::Input);
            Self::add_port(state, client, "audio_out".to_string(), TestPortType::Audio, TestDirection::Output);
            Self::add_port(state, client, "midi_in".to_string(), TestPortType::Midi, TestDirection::Input);
            Self::add_port(state, client, "midi_out".to_string(), TestPortType::Midi, TestDirection::Output);
        }
    }

    fn add_client(state: &mut TestState, name: String) -> usize {
        let mut client = Box::new(TestClient {
            name: name.clone(),
            active: false,
            port_handles: Vec::new(),
        });
        let handle = (&mut *client as *mut TestClient) as usize;
        state.clients_by_name.insert(name, handle);
        state.clients_by_handle.insert(handle, client);
        handle
    }

    fn add_port(
        state: &mut TestState,
        client_handle: usize,
        name: String,
        port_type: TestPortType,
        direction: TestDirection,
    ) -> usize {
        let mut port = Box::new(TestPort {
            name,
            client_handle,
            port_type,
            direction,
            connections: HashSet::new(),
            audio_buffer: Vec::new(),
            midi_buffer: Vec::new(),
        });
        let handle = (&mut *port as *mut TestPort) as usize;
        state.ports_by_handle.insert(handle, port);
        if let Some(client) = state.clients_by_handle.get_mut(&client_handle) {
            client.port_handles.push(handle);
        }
        handle
    }

    fn ensure_client(state: &mut TestState, name: &str) -> usize {
        if let Some(handle) = state.clients_by_name.get(name).copied() {
            handle
        } else {
            Self::add_client(state, name.to_string())
        }
    }

    fn full_port_name(state: &TestState, port_handle: usize) -> Option<String> {
        let port = state.ports_by_handle.get(&port_handle)?;
        let client = state.clients_by_handle.get(&port.client_handle)?;
        Some(format!("{}:{}", client.name, port.name))
    }

    fn find_port_by_name(state: &TestState, client_handle: usize, name: &str) -> Option<usize> {
        if let Some((client_name, port_name)) = name.split_once(':') {
            let client_handle = state.clients_by_name.get(client_name).copied()?;
            let client = state.clients_by_handle.get(&client_handle)?;
            client
                .port_handles
                .iter()
                .copied()
                .find(|handle| state.ports_by_handle.get(handle).map(|p| p.name.as_str()) == Some(port_name))
        } else {
            let client = state.clients_by_handle.get(&client_handle)?;
            client
                .port_handles
                .iter()
                .copied()
                .find(|handle| state.ports_by_handle.get(handle).map(|p| p.name.as_str()) == Some(name))
        }
    }

    fn push_midi_event_locked(state: &mut TestState, port: usize, time: u32, data: &[u8]) -> bool {
        let Some(port) = state.ports_by_handle.get_mut(&port) else { return false; };
        if port.port_type != TestPortType::Midi {
            return false;
        }
        port.midi_buffer.push(MidiEvent {
            time,
            bytes: data.iter().copied().take(4).collect(),
        });
        true
    }

    pub fn test_reset(&self) {
        self.reset();
    }

    pub unsafe fn test_push_midi_event(&self, port: usize, time: u32, size: u16, data: *const u8) -> bool {
        if data.is_null() {
            return false;
        }
        let mut state = self.state.lock().unwrap();
        let slice = std::slice::from_raw_parts(data, std::cmp::min(size as usize, 4));
        Self::push_midi_event_locked(&mut state, port, time, slice)
    }

    pub fn test_clear_midi_buffer(&self, port: usize) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(port) = state.ports_by_handle.get_mut(&port) else { return false; };
        port.midi_buffer.clear();
        true
    }

    pub fn test_midi_buffer_size(&self, port: usize) -> usize {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&port)
            .map(|port| port.midi_buffer.len())
            .unwrap_or(0)
    }

    pub unsafe fn test_get_midi_event(
        &self,
        port: usize,
        index: usize,
        out_time: &mut u32,
        out_size: &mut u16,
        out_data: *mut u8,
    ) -> bool {
        let state = self.state.lock().unwrap();
        let Some(port) = state.ports_by_handle.get(&port) else { return false; };
        let Some(event) = port.midi_buffer.get(index) else { return false; };
        *out_time = event.time;
        *out_size = event.bytes.len() as u16;
        if !out_data.is_null() {
            std::ptr::copy_nonoverlapping(event.bytes.as_ptr(), out_data, event.bytes.len());
        }
        true
    }
}

impl JackApi for TestJackApi {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn supports_processing(&self) -> bool {
        false
    }

    fn init(&self) -> Result<()> {
        Ok(())
    }

    fn client_open(&self, name: &str) -> Result<usize> {
        let mut state = self.state.lock().unwrap();
        Ok(Self::ensure_client(&mut state, name))
    }

    fn activate(&self, client: usize) -> i32 {
        let mut state = self.state.lock().unwrap();
        if let Some(client) = state.clients_by_handle.get_mut(&client) {
            client.active = true;
        }
        0
    }

    fn deactivate(&self, client: usize) -> i32 {
        let mut state = self.state.lock().unwrap();
        if let Some(client) = state.clients_by_handle.get_mut(&client) {
            client.active = false;
        }
        0
    }

    fn client_close(&self, _client: usize) -> i32 {
        0
    }

    fn get_client_name(&self, client: usize) -> String {
        let state = self.state.lock().unwrap();
        state
            .clients_by_handle
            .get(&client)
            .map(|client| client.name.clone())
            .unwrap_or_default()
    }

    fn get_sample_rate(&self, _client: usize) -> u32 {
        48_000
    }

    fn get_buffer_size(&self, _client: usize) -> u32 {
        2_048
    }

    fn cpu_load(&self, _client: usize) -> f32 {
        0.0
    }

    fn set_process_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        let mut state = self.state.lock().unwrap();
        state.callbacks_by_client.entry(client).or_default().process_driver_ptr = Some(driver_ptr);
        0
    }

    fn set_xrun_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        let mut state = self.state.lock().unwrap();
        state.callbacks_by_client.entry(client).or_default().xrun_driver_ptr = Some(driver_ptr);
        0
    }

    fn set_port_connect_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        let mut state = self.state.lock().unwrap();
        state.callbacks_by_client.entry(client).or_default().port_connect_driver_ptr = Some(driver_ptr);
        0
    }

    fn set_port_registration_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        let mut state = self.state.lock().unwrap();
        state.callbacks_by_client.entry(client).or_default().port_registration_driver_ptr = Some(driver_ptr);
        0
    }

    fn set_port_rename_callback(&self, client: usize, driver_ptr: usize) -> i32 {
        let mut state = self.state.lock().unwrap();
        state.callbacks_by_client.entry(client).or_default().port_rename_driver_ptr = Some(driver_ptr);
        0
    }

    fn set_error_info_logging(&self) {}

    fn port_register(&self, client: usize, name: &str, data_type: u32, direction: u32) -> usize {
        let driver_ptr = {
            let mut state = self.state.lock().unwrap();
            let port_type = match data_type {
                PORT_DATA_TYPE_AUDIO => TestPortType::Audio,
                PORT_DATA_TYPE_MIDI => TestPortType::Midi,
                _ => return 0,
            };
            let direction = match direction {
                PORT_DIRECTION_INPUT => TestDirection::Input,
                PORT_DIRECTION_OUTPUT => TestDirection::Output,
                _ => return 0,
            };
            if let Some(existing) = Self::find_port_by_name(&state, client, name) {
                existing
            } else {
                let handle = Self::add_port(&mut state, client, name.to_string(), port_type, direction);
                if let Some(driver_ptr) = state
                    .callbacks_by_client
                    .get(&client)
                    .and_then(|callbacks| callbacks.port_registration_driver_ptr)
                {
                    drop(state);
                    crate::jack_api_cxx::ffi::jackapi_port_update_cb(driver_ptr);
                }
                return handle;
            }
        };
        driver_ptr
    }

    fn port_unregister(&self, _client: usize, _port: usize) -> i32 {
        0
    }

    fn port_name(&self, port: usize) -> String {
        let state = self.state.lock().unwrap();
        Self::full_port_name(&state, port).unwrap_or_default()
    }

    fn port_get_buffer(&self, port: usize, nframes: u32) -> usize {
        let mut state = self.state.lock().unwrap();
        let Some(port_ref) = state.ports_by_handle.get_mut(&port) else { return 0; };
        match port_ref.port_type {
            TestPortType::Audio => {
                let wanted = nframes.max(port_ref.audio_buffer.len() as u32) as usize;
                port_ref.audio_buffer.resize(wanted.max(1), 0.0);
                port_ref.audio_buffer.as_mut_ptr() as usize
            }
            TestPortType::Midi => port,
        }
    }

    fn port_is_input(&self, port: usize) -> bool {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&port)
            .map(|port| port.direction == TestDirection::Input)
            .unwrap_or(false)
    }

    fn port_is_output(&self, port: usize) -> bool {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&port)
            .map(|port| port.direction == TestDirection::Output)
            .unwrap_or(false)
    }

    fn port_is_audio(&self, port: usize) -> bool {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&port)
            .map(|port| port.port_type == TestPortType::Audio)
            .unwrap_or(false)
    }

    fn port_is_midi(&self, port: usize) -> bool {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&port)
            .map(|port| port.port_type == TestPortType::Midi)
            .unwrap_or(false)
    }

    fn port_is_mine(&self, client: usize, port: usize) -> bool {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&port)
            .map(|port| port.client_handle == client)
            .unwrap_or(false)
    }

    fn get_ports(&self, _client: usize) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .keys()
            .copied()
            .filter_map(|handle| Self::full_port_name(&state, handle))
            .collect()
    }

    fn port_by_name(&self, client: usize, name: &str) -> usize {
        let state = self.state.lock().unwrap();
        Self::find_port_by_name(&state, client, name).unwrap_or(0)
    }

    fn port_get_all_connections(&self, _client: usize, port: usize) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&port)
            .map(|port| port.connections.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn connect(&self, client: usize, src: &str, dst: &str) -> i32 {
        let driver_ptr = {
            let mut state = self.state.lock().unwrap();
            let Some(src_handle) = Self::find_port_by_name(&state, client, src) else { return -1; };
            let Some(dst_handle) = Self::find_port_by_name(&state, client, dst) else { return -1; };
            if let Some(src_port) = state.ports_by_handle.get_mut(&src_handle) {
                src_port.connections.insert(dst.to_string());
            }
            if let Some(dst_port) = state.ports_by_handle.get_mut(&dst_handle) {
                dst_port.connections.insert(src.to_string());
            }
            state
                .callbacks_by_client
                .get(&client)
                .and_then(|callbacks| callbacks.port_connect_driver_ptr)
        };
        if let Some(driver_ptr) = driver_ptr {
            crate::jack_api_cxx::ffi::jackapi_port_update_cb(driver_ptr);
        }
        0
    }

    fn disconnect(&self, client: usize, src: &str, dst: &str) -> i32 {
        let driver_ptr = {
            let mut state = self.state.lock().unwrap();
            let Some(src_handle) = Self::find_port_by_name(&state, client, src) else { return -1; };
            let Some(dst_handle) = Self::find_port_by_name(&state, client, dst) else { return -1; };
            if let Some(src_port) = state.ports_by_handle.get_mut(&src_handle) {
                src_port.connections.remove(dst);
            }
            if let Some(dst_port) = state.ports_by_handle.get_mut(&dst_handle) {
                dst_port.connections.remove(src);
            }
            state
                .callbacks_by_client
                .get(&client)
                .and_then(|callbacks| callbacks.port_connect_driver_ptr)
        };
        if let Some(driver_ptr) = driver_ptr {
            crate::jack_api_cxx::ffi::jackapi_port_update_cb(driver_ptr);
        }
        0
    }

    fn midi_get_event_count(&self, buffer: usize) -> u32 {
        let state = self.state.lock().unwrap();
        state
            .ports_by_handle
            .get(&buffer)
            .map(|port| port.midi_buffer.len() as u32)
            .unwrap_or(0)
    }

    unsafe fn midi_event_get(
        &self,
        buffer: usize,
        index: u32,
        out_time: &mut u32,
        out_size: &mut u16,
        out_data: *mut u8,
    ) -> bool {
        let state = self.state.lock().unwrap();
        let Some(port) = state.ports_by_handle.get(&buffer) else { return false; };
        let Some(event) = port.midi_buffer.get(index as usize) else { return false; };
        *out_time = event.time;
        *out_size = event.bytes.len() as u16;
        if !out_data.is_null() {
            std::ptr::copy_nonoverlapping(event.bytes.as_ptr(), out_data, event.bytes.len());
        }
        true
    }

    fn midi_clear_buffer(&self, buffer: usize) {
        let mut state = self.state.lock().unwrap();
        if let Some(port) = state.ports_by_handle.get_mut(&buffer) {
            port.midi_buffer.clear();
        }
    }

    unsafe fn midi_event_write(&self, buffer: usize, time: u32, size: u16, data: *const u8) -> i32 {
        if data.is_null() {
            return -1;
        }
        let mut state = self.state.lock().unwrap();
        let slice = std::slice::from_raw_parts(data, std::cmp::min(size as usize, 4));
        if Self::push_midi_event_locked(&mut state, buffer, time, slice) {
            0
        } else {
            -1
        }
    }
}

pub fn filter_port_names(
    api: &dyn JackApi,
    client: usize,
    name_regex: Option<&str>,
    direction_filter: Option<u32>,
    data_type_filter: Option<u32>,
) -> Vec<String> {
    let all_names = api.get_ports(client);
    let regex = name_regex.and_then(|pattern| Regex::new(pattern).ok());
    all_names
        .into_iter()
        .filter(|name| regex.as_ref().map(|r| r.is_match(name)).unwrap_or(true))
        .filter(|name| {
            let port = api.port_by_name(client, name);
            if port == 0 {
                return false;
            }
            let direction_ok = match direction_filter {
                Some(PORT_DIRECTION_INPUT) => api.port_is_input(port),
                Some(PORT_DIRECTION_OUTPUT) => api.port_is_output(port),
                Some(_) => false,
                None => true,
            };
            let type_ok = match data_type_filter {
                Some(PORT_DATA_TYPE_AUDIO) => api.port_is_audio(port),
                Some(PORT_DATA_TYPE_MIDI) => api.port_is_midi(port),
                Some(_) => false,
                None => true,
            };
            direction_ok && type_ok
        })
        .collect()
}

pub fn with_test_api<R>(api: &JackApiObject, f: impl FnOnce(&TestJackApi) -> R) -> Option<R> {
    api.inner.as_any().downcast_ref::<TestJackApi>().map(f)
}
