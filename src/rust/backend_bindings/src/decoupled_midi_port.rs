use anyhow;
use crate::ffi;
use std::sync::Mutex;

use crate::audio_driver::AudioDriver;
use crate::port::PortDirection;

pub struct DecoupledMidiPort {
    obj : Mutex<*mut ffi::shoopdaloop_decoupled_midi_port_t>,
}

unsafe impl Send for DecoupledMidiPort {}
unsafe impl Sync for DecoupledMidiPort {}

impl DecoupledMidiPort {
    pub fn new_driver_port(audio_driver : &AudioDriver,
                           name_hint : &str,
                           direction : &PortDirection) -> Result<Self, anyhow::Error>
    {
        let name_hint_ptr = name_hint.as_ptr() as *const i8;
        let obj = unsafe { ffi::open_decoupled_midi_port
                                (audio_driver.unsafe_backend_ptr(),
                                 name_hint_ptr,
                                 *direction as u32) };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create audio port"));
        }
        Ok(DecoupledMidiPort {
            obj : Mutex::new(obj),
        })
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_decoupled_midi_port_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }
}

impl Drop for DecoupledMidiPort {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::close_decoupled_midi_port(obj) };
        unsafe { ffi::destroy_shoopdaloop_decoupled_midi_port(obj) };
    }
}
