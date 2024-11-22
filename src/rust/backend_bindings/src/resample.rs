use anyhow;
use crate::ffi;
use std::sync::{Arc, Mutex};
use static_assertions::assert_impl_all;

pub struct MultichannelAudio {
    obj : Mutex<*mut ffi::shoop_multichannel_audio_t>,
}

assert_impl_all!(MultichannelAudio : Send, Sync);

unsafe impl Send for MultichannelAudio {}
unsafe impl Sync for MultichannelAudio {}

impl MultichannelAudio {
    pub fn new(n_channels : u32, n_frames : u32) -> Result<Self, anyhow::Error> {
        let obj = unsafe { ffi::alloc_multichannel_audio(n_channels, n_frames) };
        if obj.is_null() {
            Err(anyhow::anyhow!("alloc_multichannel_audio() failed"))
        } else {
            let wrapped = Mutex::new(obj);
            Ok(MultichannelAudio { obj: wrapped })
        }
    }

    pub fn resample(&self, new_n_frames : u32) -> Result<Self, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let self_obj = *guard;
        let new_obj = unsafe { ffi::resample_audio(self_obj, new_n_frames) };
        if new_obj.is_null() {
            Err(anyhow::anyhow!("resample_audio() failed"))
        } else {
            let wrapped = Mutex::new(new_obj);
            Ok(MultichannelAudio { obj: wrapped })
        }
    }

    pub fn at(&self, frame : u32, channel: u32) -> Result<f32, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let self_obj = *guard;
        let value = unsafe { *(*self_obj).data.offset((frame * (*self_obj).n_channels + channel) as isize) };
        Ok(value)
    }
}

impl Drop for MultichannelAudio {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_multichannel_audio(obj) };
    }
}