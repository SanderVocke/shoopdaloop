use anyhow;
use crate::integer_enum;
use crate::ffi;
use std::sync::Mutex;

use crate::port::ExternalPortDescriptor;

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
    pub fn new(obj : &ffi::shoop_jack_audio_driver_settings_t) -> Self {
        JackAudioDriverSettings {
            client_name_hint : unsafe { std::ffi::CStr::from_ptr(obj.client_name_hint).to_str().unwrap().to_string() },
            maybe_server_name : if obj.maybe_server_name.is_null() {
                None
            } else {
                Some(unsafe { std::ffi::CStr::from_ptr(obj.maybe_server_name).to_str().unwrap().to_string() })
            },
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
    pub fn new(obj : &ffi::shoop_dummy_audio_driver_settings_t) -> Self {
        DummyAudioDriverSettings {
            client_name : unsafe { std::ffi::CStr::from_ptr(obj.client_name).to_str().unwrap().to_string() },
            sample_rate : obj.sample_rate,
            buffer_size : obj.buffer_size,
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

    pub fn get_sample_rate(&self) -> u32 {
        let obj = self.lock();
        unsafe { ffi::get_sample_rate(*obj) }
    }

    pub fn get_buffer_size(&self) -> u32 {
        let obj = self.lock();
        unsafe { ffi::get_buffer_size(*obj) }
    }

    pub fn start_dummy(&self, settings: &DummyAudioDriverSettings) -> Result<(), anyhow::Error> {
        let obj = self.lock();
        unsafe { ffi::start_dummy_driver(*obj, settings.to_ffi()) };
        Ok(())
    }

    pub fn start_jack(&self, settings: &JackAudioDriverSettings) -> Result<(), anyhow::Error> {
        let obj = self.lock();
        unsafe { ffi::start_jack_driver(*obj, settings.to_ffi()) };
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

    pub fn dummy_enter_controlled_mode(&self) {
        let obj = self.lock();
        unsafe { ffi::dummy_audio_enter_controlled_mode(*obj) };
    }

    pub fn dummy_enter_automatic_mode(&self) {
        let obj = self.lock();
        unsafe { ffi::dummy_audio_enter_automatic_mode(*obj) };
    }

    pub fn dummy_is_controlled(&self) -> bool {
        let obj = self.lock();
        unsafe { ffi::dummy_audio_is_in_controlled_mode(*obj) != 0 }
    }

    pub fn dummy_request_controlled_frames(&self, n: u32) {
        let obj = self.lock();
        unsafe { ffi::dummy_audio_request_controlled_frames(*obj, n) };
    }

    pub fn dummy_n_requested_frames(&self) -> u32 {
        let obj = self.lock();
        unsafe { ffi::dummy_audio_n_requested_frames(*obj) }
    }

    pub fn dummy_add_external_mock_port(&self, name: &str, direction: u32, data_type: u32) {
        let obj = self.lock();
        unsafe {
            ffi::dummy_driver_add_external_mock_port(
                *obj,
                std::ffi::CString::new(name).unwrap().as_ptr(),
                direction,
                data_type,
            )
        };
    }

    pub fn dummy_remove_external_mock_port(&self, name: &str) {
        let obj = self.lock();
        unsafe {
            ffi::dummy_driver_remove_external_mock_port(
                *obj,
                std::ffi::CString::new(name).unwrap().as_ptr(),
            )
        };
    }

    pub fn dummy_remove_all_external_mock_ports(&self) {
        let obj = self.lock();
        unsafe { ffi::dummy_driver_remove_all_external_mock_ports(*obj) };
    }
        let obj = self.lock();
        unsafe { ffi::dummy_audio_run_requested_frames(*obj) };
    }

    pub fn find_external_ports(&self, maybe_name_regex: Option<&str>, port_direction: u32, data_type: u32) -> Vec<ExternalPortDescriptor> {
        let obj = self.lock();
        let regex_ptr = maybe_name_regex.map_or(std::ptr::null(), |s| s.as_ptr() as *const i8);
        let result = unsafe { ffi::find_external_ports(*obj, regex_ptr, port_direction, data_type) };
        let ports = unsafe { std::slice::from_raw_parts((*result).ports, (*result).n_ports as usize) };
        let mut port_descriptors = Vec::new();
        unsafe {
            for i in 0..(*result).n_ports {
                port_descriptors.push(ExternalPortDescriptor::new(&ports[i as usize]));
            }
        }
        unsafe { ffi::destroy_external_port_descriptors(result) };
        port_descriptors
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
