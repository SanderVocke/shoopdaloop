use anyhow;
use crate::ffi;
use std::sync::Mutex;

use crate::audio_driver::AudioDriver;
use crate::ffi::shoop_backend_session_state_info_t;
use crate::common::BackendResult;
use crate::ffi::shoop_profiling_report_item_t;
use std::ffi::CStr;

pub struct BackendSession {
    obj : Mutex<*mut ffi::shoop_backend_session_t>,
}

pub struct ProfilingReportItem {
    pub key: String,
    pub n_samples: f32,
    pub average: f32,
    pub worst: f32,
    pub most_recent: f32,
}

impl ProfilingReportItem {
    pub fn new(obj: &shoop_profiling_report_item_t) -> Self {
        let key = unsafe { CStr::from_ptr(obj.key).to_string_lossy().into_owned() };
        ProfilingReportItem {
            key,
            n_samples: obj.n_samples,
            average: obj.average,
            worst: obj.worst,
            most_recent: obj.most_recent,
        }
    }
}

pub struct BackendSessionState {
    pub audio_driver: *mut ffi::shoop_audio_driver_t,
}

impl BackendSessionState {
    pub fn new(obj: &shoop_backend_session_state_info_t) -> Self {
        BackendSessionState {
            audio_driver: obj.audio_driver,
        }
    }
}

unsafe impl Send for BackendSession {}
unsafe impl Sync for BackendSession {}

impl BackendSession {
    pub fn new() -> Result<Self, anyhow::Error> {
        let obj = unsafe { ffi::create_backend_session() };
        if obj.is_null() {
            Err(anyhow::anyhow!("create_backend_session() failed"))
        } else {
            let wrapped = Mutex::new(obj);
            Ok(BackendSession { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoop_backend_session_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }

    pub fn set_audio_driver(&self, driver : &AudioDriver) -> Result<(), anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let driver_guard = driver.lock();
        let driver_obj = *driver_guard;
        let result = unsafe { ffi::set_audio_driver(obj, driver_obj) };
        if BackendResult::try_from(result)? == BackendResult::Success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("set_audio_driver() failed"))
        }
    }
}

impl Drop for BackendSession {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_backend_session(obj) };
    }
}
