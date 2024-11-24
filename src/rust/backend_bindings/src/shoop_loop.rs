use anyhow;
use crate::ffi;
use std::sync::Mutex;

use crate::backend_session::BackendSession;
use crate::audio_channel::AudioChannel;
use crate::midi_channel::MidiChannel;
use crate::common::ChannelMode;

pub struct Loop {
    obj : Mutex<*mut ffi::shoopdaloop_loop_t>,
}

unsafe impl Send for Loop {}
unsafe impl Sync for Loop {}

impl Loop {
    pub fn new(backend_session : &BackendSession) -> Result<Self, anyhow::Error> {
        let obj = unsafe { ffi::create_loop(backend_session.unsafe_backend_ptr()) };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create loop"));
        }
        Ok(Loop {
            obj : Mutex::new(obj),
        })
    }
    
    pub fn add_audio_channel(&self, mode : ChannelMode) -> Result<AudioChannel, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let channel = unsafe { ffi::add_audio_channel(obj, mode as u32) };
        AudioChannel::new(channel)
    }

    pub fn add_midi_channel(&self, mode : ChannelMode) -> Result<MidiChannel, anyhow::Error> {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        let channel = unsafe { ffi::add_midi_channel(obj, mode as u32) };
        MidiChannel::new(channel)
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_loop_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for Loop {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_loop(obj) };
    }
}