#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qproperty(bool, shift_pressed)]
        #[qproperty(bool, control_pressed)]
        #[qproperty(bool, alt_pressed)]
        type KeyModifiers = super::KeyModifiersRust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn process_event(self: &KeyModifiers, event: &QString);
    }
}

use std::env;
use cxx_qt_lib::QString;
use cxx_qt_lib::QEvent;

#[derive(Default)]
pub struct KeyModifiersRust {}

impl qobject::KeyModifiers {
    pub fn process_event(self: &qobject::KeyModifiers, event: &qobject::QEvent) {
        println!("KeyModifiers: processing event.");
        println!("KeyModifiers: shift_pressed: {}", self.shift_pressed());
        println!("KeyModifiers: control_pressed: {}", self.control_pressed());
        println!("KeyModifiers: alt_pressed: {}", self.alt_pressed());
    }
}
