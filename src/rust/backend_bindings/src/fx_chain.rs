use anyhow;
use crate::ffi;
use std::sync::Mutex;
use crate::integer_enum;

use crate::backend_session::BackendSession;

integer_enum! {
    pub enum FXChainType {
        CarlaRack = ffi::shoop_fx_chain_type_t_Carla_Rack,
        CarlaPatchbay = ffi::shoop_fx_chain_type_t_Carla_Patchbay,
        CarlaPatchbay16x = ffi::shoop_fx_chain_type_t_Carla_Patchbay_16x,
        Test2x2x1 = ffi::shoop_fx_chain_type_t_Test2x2x1,
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

    pub fn get_state(&self) -> Option<FXChainStateInfo> {
        if self.available() {
            unsafe {
                let state_ptr = ffi::get_fx_chain_state(*self.obj.lock().unwrap());
                if !state_ptr.is_null() {
                    let state = FXChainStateInfo::from_ffi(&*state_ptr);
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
                    let state_str = std::ffi::CStr::from_ptr(state_ptr).to_string_lossy().into_owned();
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

    pub fn restore_state(&self, state_str: &str) {
        if self.available() {
            let c_state_str = std::ffi::CString::new(state_str).unwrap();
            unsafe {
                ffi::restore_fx_chain_internal_state(*self.obj.lock().unwrap(), c_state_str.as_ptr());
            }
        }
    }
}

#[derive(Debug)]
pub struct FXChainStateInfo {
    pub ready: u32,
    pub active: u32,
    pub visible: u32,
}

impl FXChainStateInfo {
    pub unsafe fn from_ffi(obj: &ffi::shoop_fx_chain_state_info_t) -> Self {
        FXChainStateInfo {
            ready: obj.ready,
            active: obj.active,
            visible: obj.visible,
        }
    }
}

pub struct FXChain {
    obj : Mutex<*mut ffi::shoopdaloop_fx_chain_t>,
}

unsafe impl Send for FXChain {}
unsafe impl Sync for FXChain {}

impl FXChain {
    pub fn new(backend_session : &BackendSession,
               chain_type : &FXChainType,
               title : &str) -> Result<Self, anyhow::Error> {
        let title_ptr = title.as_ptr() as *const i8;
        let obj = unsafe { ffi::create_fx_chain
                             (backend_session.unsafe_backend_ptr(),
                              *chain_type as u32,
                              title_ptr) };
        if obj.is_null() {
            return Err(anyhow::anyhow!("Failed to create FX chain"));
        }
        Ok(FXChain {
            obj : Mutex::new(obj),
        })
    }

    pub unsafe fn unsafe_backend_ptr(&self) -> *mut ffi::shoopdaloop_fx_chain_t {
        let guard = self.obj.lock().unwrap();
        *guard
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
