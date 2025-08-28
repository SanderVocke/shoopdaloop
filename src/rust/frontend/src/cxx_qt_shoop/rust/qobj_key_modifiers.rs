use std::pin::Pin;

use crate::cxx_qt_shoop::qobj_key_modifiers_bridge::ffi::*;

impl KeyModifiers {
    pub fn on_shift_pressed(self: Pin<&mut KeyModifiers>) {
        self.set_shift_pressed(true);
    }

    pub fn on_control_pressed(self: Pin<&mut KeyModifiers>) {
        println!("control pressed");
        self.set_control_pressed(true);
    }

    pub fn on_alt_pressed(self: Pin<&mut KeyModifiers>) {
        self.set_alt_pressed(true);
    }

    pub fn on_shift_released(self: Pin<&mut KeyModifiers>) {
        self.set_shift_pressed(false);
    }

    pub fn on_control_released(self: Pin<&mut KeyModifiers>) {
        println!("control released");
        self.set_control_pressed(false);
    }

    pub fn on_alt_released(self: Pin<&mut KeyModifiers>) {
        self.set_alt_pressed(false);
    }
}

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_keymodifiers(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}
