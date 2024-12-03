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

    pub fn get_state(&self) -> AudioPortStateInfo {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            let state = ffi::get_audio_port_state(obj);
            let rval = AudioPortStateInfo::from_ffi(&*state);
            ffi::destroy_audio_port_state_info(state);
            rval
        }
    }

    pub fn set_gain(&self, gain: f32) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            ffi::set_audio_port_gain(obj, gain);
        }
    }

    pub fn set_muted(&self, muted: bool) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            ffi::set_audio_port_muted(obj, if muted { 1 } else { 0 });
        }
    }

    pub fn set_passthrough_muted(&self, muted: bool) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            ffi::set_audio_port_passthroughMuted(obj, if muted { 1 } else { 0 });
        }
    }

    pub fn connect_internal(&self, other: &AudioPort) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let other_guard = other.obj.lock().unwrap();
        let other_obj = *other_guard;
        unsafe {
            ffi::connect_audio_port_internal(obj, other_obj);
        }
    }

    pub fn dummy_queue_data(&self, data: &[f32]) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            ffi::dummy_audio_port_queue_data(obj, data.len() as u32, data.as_ptr());
        }
    }

    pub fn dummy_dequeue_data(&self, n_samples: u32) -> Vec<f32> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let mut data = vec![0.0; n_samples as usize];
        unsafe {
            ffi::dummy_audio_port_dequeue_data(obj, n_samples, data.as_mut_ptr());
        }
        data
    }

    pub fn dummy_request_data(&self, n_samples: u32) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            ffi::dummy_audio_port_request_data(obj, n_samples);
        }
    }

    pub fn get_connections_state(&self) -> HashMap<String, bool> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            let state = ffi::get_audio_port_connections_state(obj);
            let mut rval = HashMap::new();
            for i in 0..(*state).n_ports {
                let port = &(*state).ports[i as usize];
                let name = CStr::from_ptr(port.name).to_string_lossy().into_owned();
                rval.insert(name, port.connected != 0);
            }
            ffi::destroy_port_connections_state(state);
            rval
        }
    }

    pub fn connect_external_port(&self, name: &str) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let c_name = CString::new(name).unwrap();
        unsafe {
            ffi::connect_audio_port_external(obj, c_name.as_ptr());
        }
    }

    pub fn disconnect_external_port(&self, name: &str) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let c_name = CString::new(name).unwrap();
        unsafe {
            ffi::disconnect_audio_port_external(obj, c_name.as_ptr());
        }
    }

    pub fn set_ringbuffer_n_samples(&self, n: u32) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        unsafe {
            ffi::set_audio_port_ringbuffer_n_samples(obj, n);
        }
    }
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
