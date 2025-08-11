use crate::cxx_qt_shoop::qobj_fx_chain_gui_bridge::ffi::*;
use std::pin::Pin;

impl FXChainGui {
    pub fn initialize_impl(self: Pin<&mut Self>) {
        todo!();
    }

    pub fn set_backend(self: Pin<&mut Self>, _backend: *mut QObject) {
        todo!();
    }

    pub fn set_title(self: Pin<&mut Self>, _title: QString) {
        todo!();
    }

    pub fn set_chain_type(self: Pin<&mut Self>, _chain_type: i32) {
        todo!();
    }

    pub fn get_state_str(self: Pin<&mut Self>) -> QString {
        todo!();
    }

    pub fn restore_state(self: Pin<&mut Self>, _state_str: QString) {
        todo!();
    }
}
