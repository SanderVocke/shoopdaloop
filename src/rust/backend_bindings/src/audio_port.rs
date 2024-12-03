use anyhow;
use crate::ffi;
use std::sync::Mutex;

use crate::backend_session::BackendSession;
use crate::audio_driver::AudioDriver;
use crate::port::{PortDirection, PortConnectability};

#[derive(Debug)]
pub struct AudioPortStateInfo {
    pub input_peak: f32,
    pub output_peak: f32,
    pub gain: f32,
    pub muted: u32,
    pub passthrough_muted: u32,
    pub ringbuffer_n_samples: u32,
    pub name: String,
}

impl AudioPortStateInfo {
    pub fn from_ffi(ffi_info: &ffi::shoop_audio_port_state_info_t) -> Self {
        unsafe {
            AudioPortStateInfo {
                input_peak: ffi_info.input_peak,
                output_peak: ffi_info.output_peak,
                gain: ffi_info.gain,
                muted: ffi_info.muted,
                passthrough_muted: ffi_info.passthrough_muted,
                ringbuffer_n_samples: ffi_info.ringbuffer_n_samples,
                name: std::ffi::CStr::from_ptr(ffi_info.name).to_string_lossy().into_owned(),
            }
        }
    }
}

pub struct AudioPort {
    obj : Mutex<*mut ffi::shoopdaloop_audio_port_t>,
}

unsafe impl Send for AudioPort {}
unsafe impl Sync for AudioPort {}

impl AudioPort {
    pub fn new_driver_port(backend_session : &BackendSession,
                           audio_driver : &AudioDriver,
                           name_hint : &str,
                           direction : &PortDirection,
                           min_n_ringbuffer_samples : u32) -> Result<Self, anyhow::Error>
    {
        let name_hint_ptr = name_hint.as_ptr() as *const i8;
        let obj = unsafe { ffi::open_driver_audio_port
                                (backend_session.unsafe_backend_ptr(),
                                 audio_driver.unsafe_backend_ptr(),
                                 name_hint_ptr,
                                 *direction as u32,
                                 min_n_ringbuffer_samples) };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create audio port"));
        }
        Ok(AudioPort {
            obj : Mutex::new(obj),
        })
    }

    pub fn new(ptr: *mut ffi::shoopdaloop_audio_port_t) -> Self {
        AudioPort {
            obj : Mutex::new(ptr),
        }
    }

    pub fn input_connectability(&self) -> PortConnectability {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            PortConnectability::from_ffi(ffi::get_audio_port_input_connectability(obj))
        }
    }

    pub fn output_connectability(&self) -> PortConnectability {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            PortConnectability::from_ffi(ffi::get_audio_port_output_connectability(obj))
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

    pub fn unsafe_port_from_raw_ptr(ptr : usize) -> Self {
        AudioPort {
            obj : Mutex::new(ptr as *mut ffi::shoopdaloop_audio_port_t),
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_audio_port_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for AudioPort {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_audio_port(obj) };
    }
}
