use crate::ffi;
use anyhow::anyhow;
use common::logging::macros::*;
use enum_iterator::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::sync::Mutex;

shoop_log_unit!("BackendBindings.FXChain");

use crate::audio_port::AudioPort;
use crate::midi_port::MidiPort;

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum FXChainType {
    CarlaRack = ffi::shoop_fx_chain_type_t_Carla_Rack as i32,
    CarlaPatchbay = ffi::shoop_fx_chain_type_t_Carla_Patchbay as i32,
    CarlaPatchbay16x = ffi::shoop_fx_chain_type_t_Carla_Patchbay_16x as i32,
    Test2x2x1 = ffi::shoop_fx_chain_type_t_Test2x2x1 as i32,
}

impl FXChainType {
    pub fn to_ffi(&self) -> ffi::shoop_fx_chain_type_t {
        *self as ffi::shoop_fx_chain_type_t
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
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
            Err(anyhow!("Cannot create FXChain from null pointer"))
        } else {
            let wrapped = Mutex::new(ptr);
            Ok(FXChain { obj: wrapped })
        }
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_fx_chain_t {
        let guard = match self.obj.lock() {
            Ok(g) => g,
            Err(e) => {
                error!("Mutex poisoned in unsafe_backend_ptr: {}", e);
                e.into_inner()
            }
        };
        *guard
    }

    pub fn available(&self) -> bool {
        match self.obj.lock() {
            Ok(g) => !g.is_null(),
            Err(e) => {
                error!("Mutex poisoned in available: {}", e);
                !e.into_inner().is_null()
            }
        }
    }

    pub fn set_visible(&self, visible: bool) {
        if self.available() {
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in set_visible: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                ffi::fx_chain_set_ui_visible(*guard, visible as u32);
            }
        }
    }

    pub fn set_active(&self, active: bool) {
        if self.available() {
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in set_active: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                ffi::set_fx_chain_active(*guard, active as u32);
            }
        }
    }

    pub fn get_state(&self) -> Option<FXChainState> {
        if self.available() {
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in get_state: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                let state_ptr = ffi::get_fx_chain_state(*guard);
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
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in get_state_str: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                let state_ptr = ffi::get_fx_chain_internal_state(*guard);
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
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in get_audio_input_port: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                let port = ffi::fx_chain_audio_input_port(*guard, idx);
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
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in get_audio_output_port: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                let port = ffi::fx_chain_audio_output_port(*guard, idx);
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
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in get_midi_input_port: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                let port = ffi::fx_chain_midi_input_port(*guard, idx);
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
            let c_state_str = match std::ffi::CString::new(state_str) {
                Ok(c) => c,
                Err(e) => {
                    error!("Invalid CString for state_str: {}", e);
                    return;
                }
            };
            let guard = match self.obj.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("Mutex poisoned in restore_state: {}", e);
                    e.into_inner()
                }
            };
            unsafe {
                ffi::restore_fx_chain_internal_state(*guard, c_state_str.as_ptr());
            }
        }
    }
}

impl Drop for FXChain {
    fn drop(&mut self) {
        let guard = match self.obj.lock() {
            Ok(g) => g,
            Err(e) => {
                error!("Mutex poisoned in drop: {}", e);
                e.into_inner()
            }
        };
        let obj = *guard;
        if obj.is_null() {
            return;
        }
        unsafe { ffi::destroy_fx_chain(obj) };
    }
}
