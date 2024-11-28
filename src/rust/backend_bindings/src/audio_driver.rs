use anyhow;
use crate::audio_channel::AudioChannelState;
use crate::ffi;
use std::sync::Mutex;

integer_enum! {
    pub enum AudioDriverType {
        Jack = ffi::shoop_audio_driver_type_t_Jack,
        JackTest = ffi::shoop_audio_driver_type_t_JackTest,
        Dummy = ffi::shoop_audio_driver_type_t_Dummy,
    }
}

pub struct JackAudioDriverSettings {
    pub client_name_hint: String,
    pub maybe_server_name: Option<String>,
}

impl JackAudioDriverSettings {
    pub fn new(client_name_hint: String, maybe_server_name: Option<String>) -> Self {
        JackAudioDriverSettings {
            client_name_hint,
            maybe_server_name,
        }
    }

    pub fn to_ffi(&self) -> ffi::shoop_jack_audio_driver_settings_t {
        ffi::shoop_jack_audio_driver_settings_t {
            client_name_hint: std::ffi::CString::new(self.client_name_hint.clone()).unwrap().into_raw(),
            maybe_server_name: self.maybe_server_name.as_ref().map_or(std::ptr::null(), |s| std::ffi::CString::new(s.clone()).unwrap().into_raw()),
        }
    }
}

pub struct DummyAudioDriverSettings {
    pub client_name: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
}

impl DummyAudioDriverSettings {
    pub fn new(client_name: String, sample_rate: u32, buffer_size: u32) -> Self {
        DummyAudioDriverSettings {
            client_name,
            sample_rate,
            buffer_size,
        }
    }

    pub fn to_ffi(&self) -> ffi::shoop_dummy_audio_driver_settings_t {
        ffi::shoop_dummy_audio_driver_settings_t {
            client_name: std::ffi::CString::new(self.client_name.clone()).unwrap().into_raw(),
            sample_rate: self.sample_rate,
            buffer_size: self.buffer_size,
        }
    }
}

pub struct AudioDriver {
    obj : Mutex<*mut ffi::shoop_audio_driver_t>,
}

unsafe impl Send for AudioDriver {}
unsafe impl Sync for AudioDriver {}

impl AudioDriver {
    pub fn new(driver_type : AudioDriverType) -> Result<Self, anyhow::Error> {
        let obj = unsafe { ffi::create_audio_driver(driver_type as u32) };
        if obj.is_null() {
            Err(anyhow::anyhow!("create_audio_driver() failed"))
        } else {
            let wrapped = Mutex::new(obj);
            Ok(AudioDriver { obj: wrapped })
        }
    }

    pub fn start_dummy(&self, settings: &bindings::DummyAudioDriverSettings) -> Result<(), anyhow::Error> {
        let obj = self.lock();
        unsafe { ffi::start_dummy_driver(*obj, settings) };
        Ok(())
    }

    pub fn start_jack(&self, settings: &bindings::JackAudioDriverSettings) -> Result<(), anyhow::Error> {
        let obj = self.lock();
        unsafe { ffi::start_jack_driver(*obj, settings) };
        Ok(())
    }

    pub fn active(&self) -> bool {
        let obj = self.lock();
        let state = unsafe { ffi::get_audio_driver_state(*obj) };
        let active = unsafe { (*state).active != 0 };
        unsafe { ffi::destroy_audio_driver_state(state) };
        active
    }

    pub fn wait_process(&self) {
        let obj = self.lock();
        unsafe { ffi::wait_process(*obj) };
    }

    pub fn dummy_run_requested_frames(&self) {
        let obj = self.lock();
        unsafe { ffi::dummy_audio_run_requested_frames(*obj) };
    }

    pub fn find_external_ports(&self, maybe_name_regex: Option<&str>, port_direction: u32, data_type: u32) -> Vec<bindings::ExternalPortDescriptor> {
        let obj = self.lock();
        let regex_ptr = maybe_name_regex.map_or(std::ptr::null(), |s| s.as_ptr() as *const i8);
        let result = unsafe { ffi::find_external_ports(*obj, regex_ptr, port_direction, data_type) };
        let mut ports = Vec::new();
        for i in 0..unsafe { (*result).n_ports } {
            ports.push(unsafe { (*result).ports[i] });
        }
        unsafe { ffi::destroy_external_port_descriptors(result) };
        ports
    }

    pub fn driver_type_supported(driver_type : AudioDriverType) -> bool {
        unsafe { ffi::driver_type_supported(driver_type as u32) != 0 }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoop_audio_driver_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
    
    pub fn lock(&self) -> std::sync::MutexGuard<*mut ffi::shoop_audio_driver_t> {
        self.obj.lock().unwrap()
    }
}

impl Drop for AudioDriver {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_audio_driver(obj) };
    }
}
