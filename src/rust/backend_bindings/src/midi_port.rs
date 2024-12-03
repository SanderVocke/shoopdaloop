use anyhow;
use crate::ffi;
use std::sync::Mutex;

use crate::backend_session::BackendSession;
use crate::audio_driver::AudioDriver;
use crate::port::{PortDirection, PortConnectability};

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
                name: std::ffi::CStr::from_ptr(ffi_state.name).to_string_lossy().into_owned(),
            }
        }
    }
}

pub struct MidiPort {
    obj : Mutex<*mut ffi::shoopdaloop_midi_port_t>,
}

unsafe impl Send for MidiPort {}
unsafe impl Sync for MidiPort {}

impl MidiPort {
    pub fn new_driver_port(backend_session : &BackendSession,
                           audio_driver : &AudioDriver,
                           name_hint : &str,
                           direction : &PortDirection,
                        min_n_ringbuffer_samples : u32) -> Result<Self, anyhow::Error>
    {
        let name_hint_ptr = name_hint.as_ptr() as *const i8;
        let obj = unsafe { ffi::open_driver_midi_port
                                (backend_session.unsafe_backend_ptr(),
                                audio_driver.unsafe_backend_ptr(),
                                name_hint_ptr,
                                *direction as u32,
                                min_n_ringbuffer_samples) };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create audio port"));
        }
        Ok(MidiPort {
            obj : Mutex::new(obj),
        })
    }

    pub fn new(ptr : *mut ffi::shoopdaloop_midi_port_t) -> Self {
        MidiPort {
            obj : Mutex::new(ptr),
        }
    }

    pub fn input_connectability(&self) -> PortConnectability {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            PortConnectability::from_ffi(ffi::get_midi_port_input_connectability(obj))
        }
    }

    pub fn output_connectability(&self) -> PortConnectability {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            PortConnectability::from_ffi(ffi::get_midi_port_output_connectability(obj))
        }
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
