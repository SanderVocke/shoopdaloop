use anyhow;
use crate::ffi;
use std::sync::Mutex;

use crate::audio_driver::AudioDriver;
use crate::shoop_loop::Loop;
use crate::common::BackendResult;
use crate::fx_chain::FXChain;
use std::ffi::CStr;
use std::slice;

pub struct ProfilingReportItem {
    pub key: String,
    pub n_samples: f32,
    pub average: f32,
    pub worst: f32,
    pub most_recent: f32,
}

pub struct ProfilingReport {
    pub items: Vec<ProfilingReportItem>,
}

impl ProfilingReport {
    pub fn new(obj: &ffi::shoop_profiling_report_t) -> Self {
        let items_slice = unsafe { slice::from_raw_parts(obj.items, obj.n_items as usize) };
        let items = items_slice.iter().map(|item| ProfilingReportItem::new(item)).collect();
        ProfilingReport { items }
    }
}

impl ProfilingReportItem {
    pub fn new(obj: &ffi::shoop_profiling_report_item_t) -> Self {
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
    pub fn new(obj: &ffi::shoop_backend_session_state_info_t) -> Self {
        BackendSessionState {
            audio_driver: obj.audio_driver,
        }
    }
}

pub struct BackendSession {
    obj : Mutex<*mut ffi::shoop_backend_session_t>,
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

    pub fn create() -> Result<Self, anyhow::Error> {
        let obj = unsafe { ffi::create_backend_session() };
        if obj.is_null() {
            Err(anyhow::anyhow!("create_backend_session() failed"))
        } else {
            let wrapped = Mutex::new(obj);
            Ok(BackendSession { obj: wrapped })
        }
    }

    pub fn get_state(&self) -> BackendSessionState {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let state = unsafe { ffi::get_backend_session_state(obj) };
        let rval = BackendSessionState::new(unsafe { &*state });
        unsafe { ffi::destroy_backend_state_info(state) };
        rval
    }

    pub fn create_loop(&self) -> Result<Loop, anyhow::Error> {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let loop_ptr = unsafe { ffi::create_loop(obj) };
        if loop_ptr.is_null() {
            Err(anyhow::anyhow!("create_loop() failed"))
        } else {
            let _loop = Loop::new(loop_ptr)?;
            Ok(_loop)
        }
    }

    pub fn create_fx_chain(&self, chain_type: ffi::shoop_fx_chain_type_t, title: &str) -> Result<FXChain, anyhow::Error> {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let c_title = std::ffi::CString::new(title).expect("Failed to create CString");
        let chain_ptr = unsafe { ffi::create_fx_chain(obj, chain_type, c_title.as_ptr()) };
        if chain_ptr.is_null() {
            Err(anyhow::anyhow!("create_fx_chain() failed"))
        } else {
            let chain = FXChain::new(chain_ptr)?;
            Ok(chain)
        }
    }

    pub fn get_profiling_report(&self) -> ProfilingReport {
        let obj = unsafe { self.unsafe_backend_ptr() };
        let report = unsafe { ffi::get_profiling_report(obj) };
        let rval = ProfilingReport::new(unsafe { &*report });
        unsafe { ffi::destroy_profiling_report(report) };
        rval
    }

    pub fn segfault_on_process_thread(&self) {
        let obj = unsafe { self.unsafe_backend_ptr() };
        unsafe { ffi::do_segfault_on_process_thread(obj) };
    }

    pub fn abort_on_process_thread(&self) {
        let obj = unsafe { self.unsafe_backend_ptr() };
        unsafe { ffi::do_abort_on_process_thread(obj) };
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
