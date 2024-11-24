use anyhow;
use crate::ffi;
use std::sync::Mutex;

pub struct AudioChannel {
    obj : Mutex<*mut ffi::shoopdaloop_loop_audio_channel_t>,
}

unsafe impl Send for AudioChannel {}
unsafe impl Sync for AudioChannel {}

impl AudioChannel {
    pub fn new(raw : *mut ffi::shoopdaloop_loop_audio_channel_t) -> Result<Self, anyhow::Error> {
        if raw.is_null() {
            Err(anyhow::anyhow!("Cannot create AudioChannel from null pointer"))
        } else {
            let wrapped = Mutex::new(raw);
            Ok(AudioChannel { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_loop_audio_channel_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for AudioChannel {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_audio_channel(obj) };
    }
}