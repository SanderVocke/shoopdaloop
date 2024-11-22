use anyhow;
use crate::integer_enum;
use crate::ffi;
use std::sync::Mutex;

integer_enum! {
    pub enum AudioDriverType {
        Jack = ffi::shoop_audio_driver_type_t_Jack,
        JackTest = ffi::shoop_audio_driver_type_t_JackTest,
        Dummy = ffi::shoop_audio_driver_type_t_Dummy,
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