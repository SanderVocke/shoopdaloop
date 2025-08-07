use backend_bindings::FXChain as BackendFXChain;

pub use crate::cxx_qt_shoop::qobj_fx_chain_backend_bridge::ffi::FXChainBackend;
use crate::cxx_qt_shoop::qobj_fx_chain_backend_bridge::ffi::*;
use std::pin::Pin;

impl FXChainBackend {
    pub fn initialize_impl(self: Pin<&mut Self>) {
        todo!();
    }

    pub fn set_backend(self: Pin<&mut Self>, backend: *mut QObject) {
        todo!();
    }

    pub fn set_title(self: Pin<&mut Self>, title: QString) {
        todo!();
    }

    pub fn set_chain_type(self: Pin<&mut Self>, chain_type: i32) {
        todo!();
    }

    pub fn get_state_str(self: Pin<&mut Self>) -> QString {
        todo!();
    }

    pub fn restore_state(self: Pin<&mut Self>, state_str: QString) {
        todo!();
    }

    pub fn get_backend_object<'a>(self: Pin<&mut Self>) -> &'a Option<BackendFXChain> {
        todo!();
    }
}
