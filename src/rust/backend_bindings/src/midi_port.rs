use anyhow;
use crate::ffi;
use std::sync::Mutex;

use crate::backend_session::BackendSession;
use crate::audio_driver::AudioDriver;
use crate::port::PortDirection;

pub struct MidiPort {
    obj : Mutex<*mut ffi::shoopdaloop_midi_port_t>,
}

unsafe impl Send for MidiPort {}
unsafe impl Sync for MidiPort {}

impl MidiPort {
    pub fn new_driver_port(backend_session : &BackendSession,
                           audio_driver : &AudioDriver,
                           name_hint : &str,
                           direction : &PortDirection,
                        min_n_ringbuffer_samples : u32) -> Result<Self, anyhow::Error>
    {
        let name_hint_ptr = name_hint.as_ptr() as *const i8;
        let obj = unsafe { ffi::open_driver_midi_port
                                (backend_session.unsafe_backend_ptr(),
                                audio_driver.unsafe_backend_ptr(),
                                name_hint_ptr,
                                *direction as u32,
                                min_n_ringbuffer_samples) };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create audio port"));
        }
        Ok(MidiPort {
            obj : Mutex::new(obj),
        })
    }

    pub fn unsafe_port_from_raw_ptr(ptr : usize) -> Self {
        MidiPort {
            obj : Mutex::new(ptr as *mut ffi::shoopdaloop_midi_port_t),
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_midi_port_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for MidiPort {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_midi_port(obj) };
    }
}