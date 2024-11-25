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