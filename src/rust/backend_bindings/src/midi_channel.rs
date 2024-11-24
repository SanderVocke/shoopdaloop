use anyhow;
use crate::ffi;
use std::sync::Mutex;

pub struct MidiChannel {
    obj : Mutex<*mut ffi::shoopdaloop_loop_midi_channel_t>,
}

unsafe impl Send for MidiChannel {}
unsafe impl Sync for MidiChannel {}

impl MidiChannel {
    pub fn new(raw : *mut ffi::shoopdaloop_loop_midi_channel_t) -> Result<Self, anyhow::Error> {
        if raw.is_null() {
            Err(anyhow::anyhow!("Cannot create MidiChannel from null pointer"))
        } else {
            let wrapped = Mutex::new(raw);
            Ok(MidiChannel { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_loop_midi_channel_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for MidiChannel {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_midi_channel(obj) };
    }
}