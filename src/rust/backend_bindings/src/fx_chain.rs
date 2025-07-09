use crate::ffi;
use crate::integer_enum;
use anyhow;
use std::sync::Mutex;

use crate::audio_port::AudioPort;
use crate::midi_port::MidiPort;

integer_enum! {
    pub enum FXChainType {
        CarlaRack = ffi::shoop_fx_chain_type_t_Carla_Rack,
        CarlaPatchbay = ffi::shoop_fx_chain_type_t_Carla_Patchbay,
        CarlaPatchbay16x = ffi::shoop_fx_chain_type_t_Carla_Patchbay_16x,
        Test2x2x1 = ffi::shoop_fx_chain_type_t_Test2x2x1,
    }
}

#[derive(Debug)]
pub struct FXChainState {
    pub ready: u32,
    pub active: u32,
    pub visible: u32,
}

impl FXChainState {
    pub unsafe fn from_ffi(obj: &ffi::shoop_fx_chain_state_info_t) -> Self {
        FXChainState {
            ready: obj.ready,
            active: obj.active,
            visible: obj.visible,
        }
    }
}

pub struct FXChain {
    obj: Mutex<*mut ffi::shoopdaloop_fx_chain_t>,
}

unsafe impl Send for FXChain {}
unsafe impl Sync for FXChain {}

impl FXChain {
    pub fn new(ptr: *mut ffi::shoopdaloop_fx_chain_t) -> Result<Self, anyhow::Error> {
        if ptr.is_null() {
            Err(anyhow::anyhow!("Cannot create FXChain from null pointer"))
        } else {
            let wrapped = Mutex::new(ptr);
            Ok(FXChain { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_fx_chain_t {
        let guard = self.obj.lock().unwrap();
        *guard
    }

    pub fn available(&self) -> bool {
        !self.obj.lock().unwrap().is_null()
    }

    pub fn set_visible(&self, visible: bool) {
        if self.available() {
            unsafe {
                ffi::fx_chain_set_ui_visible(*self.obj.lock().unwrap(), visible as u32);
            }
        }
    }

    pub fn set_active(&self, active: bool) {
        if self.available() {
            unsafe {
                ffi::set_fx_chain_active(*self.obj.lock().unwrap(), active as u32);
            }
        }
    }

    pub fn get_state(&self) -> Option<FXChainState> {
        if self.available() {
            unsafe {
                let state_ptr = ffi::get_fx_chain_state(*self.obj.lock().unwrap());
                if !state_ptr.is_null() {
                    let state = FXChainState::from_ffi(&*state_ptr);
                    ffi::destroy_fx_chain_state(state_ptr);
                    Some(state)
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_state_str(&self) -> Option<String> {
        if self.available() {
            unsafe {
                let state_ptr = ffi::get_fx_chain_internal_state(*self.obj.lock().unwrap());
                if !state_ptr.is_null() {
                    let state_str = std::ffi::CStr::from_ptr(state_ptr)
                        .to_string_lossy()
                        .into_owned();
                    ffi::destroy_string(state_ptr);
                    Some(state_str)
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_audio_input_port(&self, idx: u32) -> Option<AudioPort> {
        if self.available() {
            unsafe {
                let port = ffi::fx_chain_audio_input_port(*self.obj.lock().unwrap(), idx);
                if !port.is_null() {
                    Some(AudioPort::new(port))
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_audio_output_port(&self, idx: u32) -> Option<AudioPort> {
        if self.available() {
            unsafe {
                let port = ffi::fx_chain_audio_output_port(*self.obj.lock().unwrap(), idx);
                if !port.is_null() {
                    Some(AudioPort::new(port))
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_midi_input_port(&self, idx: u32) -> Option<MidiPort> {
        if self.available() {
            unsafe {
                let port = ffi::fx_chain_midi_input_port(*self.obj.lock().unwrap(), idx);
                if !port.is_null() {
                    Some(MidiPort::new(port))
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn restore_state(&self, state_str: &str) {
        if self.available() {
            let c_state_str = std::ffi::CString::new(state_str).expect("Failed to create CString");
            unsafe {
                ffi::restore_fx_chain_internal_state(
                    *self.obj.lock().unwrap(),
                    c_state_str.as_ptr(),
                );
            }
        }
    }
}

impl Drop for FXChain {
    fn drop(&mut self) {
        let guard = self.obj.lock().unwrap();
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_fx_chain(obj) };
    }
}
