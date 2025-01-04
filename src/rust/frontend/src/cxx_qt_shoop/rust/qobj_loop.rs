use cxx_qt::CxxQtType;
use crate::cxx_qt_shoop::qobj_loop_bridge::Loop;
use crate::cxx_qt_shoop::qobj_loop_bridge::constants::*;
use crate::cxx_qt_shoop::qobj_loop_bridge::ffi::*;
use std::pin::Pin;
use crate::cxx_qt_lib_shoop::qquickitem::{AsQQuickItem, IsQQuickItem, qquickitem_to_qobject_mut};
use crate::cxx_qt_lib_shoop::qobject::AsQObject;
use crate::cxx_qt_lib_shoop::{qobject, qtimer};

impl Loop {
    pub fn initialize_impl(self: Pin<&mut Loop>) {
        // Stub implementation
        println!("Initializing Loop");
    }

    pub fn update_on_non_gui_thread(self: Pin<&mut Loop>) {
        // Stub implementation
        println!("Updating Loop on Non-GUI Thread");
    }

    pub fn update_on_gui_thread(self: Pin<&mut Loop>) {
        // Stub implementation
        println!("Updating Loop on GUI Thread");
    }
}

pub fn register_qml_type(module_name: &str, type_name: &str) {
    let obj = make_unique_loop();
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    register_qml_type_loop(obj.as_ref().unwrap(), &mut mdl, 1, 0, &mut tp);
}
