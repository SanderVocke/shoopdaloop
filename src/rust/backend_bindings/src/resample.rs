use anyhow;
use crate::ffi;

pub struct MultichannelAudio {
    obj: *mut ffi::shoop_multichannel_audio_t,
}

impl MultichannelAudio {
    pub fn new(n_channels : u32, n_frames : u32) -> Result<Self, anyhow::Error> {
        let obj = unsafe { ffi::alloc_multichannel_audio(n_channels, n_frames) };
        if obj.is_null() {
            Err(anyhow::anyhow!("alloc_multichannel_audio() failed"))
        } else {
            Ok(MultichannelAudio { obj })
        }
    }

    pub fn resample(&mut self, new_n_frames : u32) -> Result<Self, anyhow::Error> {
        let new_obj = unsafe { ffi::resample_audio(self.obj, new_n_frames) };
        if new_obj.is_null() {
            Err(anyhow::anyhow!("resample_audio() failed"))
        } else {
            Ok(MultichannelAudio { obj: new_obj })
        }
    }
}

impl Drop for MultichannelAudio {
    fn drop(&mut self) {
        unsafe { ffi::destroy_multichannel_audio(self.obj) };
    }
}