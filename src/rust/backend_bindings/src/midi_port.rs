use crate::ffi;
use anyhow;
use std::sync::Mutex;

use crate::audio_driver::AudioDriver;
use crate::backend_session::BackendSession;
use crate::midi::MidiEvent;
use crate::port::{PortConnectability, PortDirection};
use std::collections::HashMap;
use std::ffi::{CStr, CString};

#[derive(Debug)]
pub struct MidiPortState {
    pub n_input_events: u32,
    pub n_input_notes_active: u32,
    pub n_output_events: u32,
    pub n_output_notes_active: u32,
    pub muted: u32,
    pub passthrough_muted: u32,
    pub ringbuffer_n_samples: u32,
    pub name: String,
}

impl MidiPortState {
    pub fn from_ffi(ffi_state: &ffi::shoop_midi_port_state_info_t) -> Self {
        unsafe {
            MidiPortState {
                n_input_events: ffi_state.n_input_events,
                n_input_notes_active: ffi_state.n_input_notes_active,
                n_output_events: ffi_state.n_output_events,
                n_output_notes_active: ffi_state.n_output_notes_active,
                muted: ffi_state.muted,
                passthrough_muted: ffi_state.passthrough_muted,
                ringbuffer_n_samples: ffi_state.ringbuffer_n_samples,
                name: std::ffi::CStr::from_ptr(ffi_state.name)
                    .to_string_lossy()
                    .into_owned(),
            }
        }
    }
}

pub struct MidiPort {
    obj: Mutex<*mut ffi::shoopdaloop_midi_port_t>,
}

unsafe impl Send for MidiPort {}
unsafe impl Sync for MidiPort {}

impl MidiPort {
    pub fn new_driver_port(
        backend_session: &BackendSession,
        audio_driver: &AudioDriver,
        name_hint: &str,
        direction: &PortDirection,
        min_n_ringbuffer_samples: u32,
    ) -> Result<Self, anyhow::Error> {
        let c_name_hint = CString::new(name_hint).expect("CString::new failed");
        let name_hint_ptr = c_name_hint.as_ptr();
        let obj = unsafe {
            ffi::open_driver_midi_port(
                backend_session.unsafe_backend_ptr(),
                audio_driver.unsafe_backend_ptr(),
                name_hint_ptr,
                *direction as ffi::shoop_port_direction_t,
                min_n_ringbuffer_samples,
            )
        };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create audio port"));
        }
        Ok(MidiPort {
            obj: Mutex::new(obj),
        })
    }

    pub fn new(ptr: *mut ffi::shoopdaloop_midi_port_t) -> Self {
        MidiPort {
            obj: Mutex::new(ptr),
        }
    }

    pub fn get_state(&self) -> Result<MidiPortState, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let state_ptr = unsafe { ffi::get_midi_port_state(obj) };
        if state_ptr.is_null() {
            return Err(anyhow::anyhow!("Failed to get midi port state"));
        }
        let state = unsafe { MidiPortState::from_ffi(&*state_ptr) };
        unsafe { ffi::destroy_midi_port_state_info(state_ptr) };
        Ok(state)
    }

    pub fn set_muted(&self, muted: bool) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe { ffi::set_midi_port_muted(obj, if muted { 1 } else { 0 }) };
    }

    pub fn set_passthrough_muted(&self, muted: bool) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe { ffi::set_midi_port_passthroughMuted(obj, if muted { 1 } else { 0 }) };
    }

    pub fn connect_internal(&self, other: &MidiPort) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let other_guard = other.obj.lock().unwrap();
        let other_obj = *other_guard;
        unsafe { ffi::connect_midi_port_internal(obj, other_obj) };
    }

    pub fn dummy_clear_queues(&self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe { ffi::dummy_midi_port_clear_queues(obj) };
    }

    pub fn dummy_queue_msg(&self, msg: &MidiEvent) {
        self.dummy_queue_msgs(vec![msg.clone()]);
    }

    pub fn dummy_queue_msgs(&self, msgs: Vec<MidiEvent>) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let sequence = unsafe { ffi::alloc_midi_sequence(msgs.len() as u32) };
        for (i, msg) in msgs.iter().enumerate() {
            let event = unsafe { ffi::alloc_midi_event(msg.data.len() as u32) };
            unsafe {
                (*event).time = msg.time;
                (*event).size = msg.size() as u32;
                std::ptr::copy_nonoverlapping(msg.data.as_ptr(), (*event).data, msg.data.len());
                (*sequence).events.offset(i as isize).write(event);
            }
        }
        unsafe { ffi::dummy_midi_port_queue_data(obj, sequence) };
        unsafe { ffi::destroy_midi_sequence(sequence) };
    }

    pub fn dummy_dequeue_data(&self) -> Vec<MidiEvent> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let sequence = unsafe { ffi::dummy_midi_port_dequeue_data(obj) };
        if sequence.is_null() {
            return Vec::default();
        }
        let mut events = Vec::new();
        for i in 0..unsafe { (*sequence).n_events } {
            let event = unsafe { &**(*sequence).events.add(i as usize) };
            let obj = MidiEvent::new(event);
            events.push(obj);
        }
        unsafe { ffi::destroy_midi_sequence(sequence) };
        events
    }

    pub fn dummy_request_data(&self, n_frames: u32) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe { ffi::dummy_midi_port_request_data(obj, n_frames) };
    }

    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let state_ptr = unsafe { ffi::get_midi_port_connections_state(obj) };
        let state = unsafe { &*state_ptr };
        let mut connections = HashMap::new();
        for i in 0..state.n_ports {
            let port = unsafe { &*state.ports.add(i as usize) };
            let name = unsafe { CStr::from_ptr(port.name).to_string_lossy().into_owned() };
            connections.insert(name, port.connected != 0);
        }
        unsafe { ffi::destroy_port_connections_state(state_ptr) };
        connections
    }

    pub fn connect_external_port(&self, name: &str) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let c_name = CString::new(name).expect("CString::new failed");
        unsafe { ffi::connect_midi_port_external(obj, c_name.as_ptr()) };
    }

    pub fn disconnect_external_port(&self, name: &str) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let c_name = CString::new(name).unwrap();
        unsafe { ffi::disconnect_midi_port_external(obj, c_name.as_ptr()) };
    }

    pub fn set_ringbuffer_n_samples(&self, n: u32) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe { ffi::set_midi_port_ringbuffer_n_samples(obj, n) };
    }

    pub fn input_connectability(&self) -> PortConnectability {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe { PortConnectability::from_ffi(ffi::get_midi_port_input_connectability(obj)) }
    }

    pub fn output_connectability(&self) -> PortConnectability {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe { PortConnectability::from_ffi(ffi::get_midi_port_output_connectability(obj)) }
    }

    pub fn direction(&self) -> PortDirection {
        let input_conn = self.input_connectability();
        let output_conn = self.output_connectability();
        if input_conn.external && output_conn.external {
            PortDirection::Any
        } else if input_conn.external {
            PortDirection::Input
        } else if output_conn.external {
            PortDirection::Output
        } else {
            PortDirection::Any
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_midi_port_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for MidiPort {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_midi_port(obj) };
    }
}
