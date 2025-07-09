use crate::ffi;
use anyhow;
use std::sync::Mutex;

use crate::audio_driver::AudioDriver;
use crate::midi::MidiEvent;
use crate::port::PortDirection;

pub struct DecoupledMidiPort {
    obj: Mutex<*mut ffi::shoopdaloop_decoupled_midi_port_t>,
}

unsafe impl Send for DecoupledMidiPort {}
unsafe impl Sync for DecoupledMidiPort {}

impl DecoupledMidiPort {
    pub fn new_driver_port(
        audio_driver: &AudioDriver,
        name_hint: &str,
        direction: &PortDirection,
    ) -> Result<Self, anyhow::Error> {
        let name_hint_cstr = std::ffi::CString::new(name_hint)?;
        let name_hint_ptr = name_hint_cstr.as_ptr();
        let obj = unsafe {
            ffi::open_decoupled_midi_port(
                audio_driver.unsafe_backend_ptr(),
                name_hint_ptr,
                *direction as ffi::shoop_port_direction_t,
            )
        };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create audio port"));
        }
        Ok(DecoupledMidiPort {
            obj: Mutex::new(obj),
        })
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_decoupled_midi_port_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }

    pub fn maybe_next_message(&self) -> Option<MidiEvent> {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let msg = unsafe { ffi::maybe_next_message(obj) };
        if !msg.is_null() {
            let msg_ref = unsafe { &*msg };
            let midi_msg = MidiEvent::new(msg_ref);
            unsafe { ffi::destroy_midi_event(msg) };
            return Some(midi_msg);
        }
        None
    }

    pub fn name(&self) -> String {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let c_str = unsafe { ffi::get_decoupled_midi_port_name(obj) };
        let name = unsafe { std::ffi::CStr::from_ptr(c_str) };
        name.to_string_lossy().into_owned()
    }

    pub fn send_midi(&self, msg: &[u8]) {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let data_type = std::mem::size_of::<u8>() * msg.len();
        let arr = msg.as_ptr() as *mut u8; // Cast the pointer to *mut u8
        unsafe { ffi::send_decoupled_midi(obj, data_type as u32, arr) };
    }

    pub fn get_connections_state(&self) -> std::collections::HashMap<String, bool> {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let state = unsafe { ffi::get_decoupled_midi_port_connections_state(obj) };
        let state_ref = unsafe { &*state };
        let mut connections = std::collections::HashMap::new();
        for i in 0..state_ref.n_ports {
            let port = unsafe { &*state_ref.ports.add(i as usize) };
            let name = unsafe { std::ffi::CStr::from_ptr(port.name) }
                .to_string_lossy()
                .into_owned();
            connections.insert(name, port.connected != 0);
        }
        unsafe { ffi::destroy_port_connections_state(state) };
        connections
    }

    pub fn connect_external_port(&self, name: &str) {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let c_name = std::ffi::CString::new(name).expect("CString::new failed");
        unsafe { ffi::connect_external_decoupled_midi_port(obj, c_name.as_ptr()) };
    }

    pub fn disconnect_external_port(&self, name: &str) {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let c_name = std::ffi::CString::new(name).expect("CString::new failed");
        unsafe { ffi::disconnect_external_decoupled_midi_port(obj, c_name.as_ptr()) };
    }
}

impl Drop for DecoupledMidiPort {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::close_decoupled_midi_port(obj) };
        unsafe { ffi::destroy_shoopdaloop_decoupled_midi_port(obj) };
    }
}
