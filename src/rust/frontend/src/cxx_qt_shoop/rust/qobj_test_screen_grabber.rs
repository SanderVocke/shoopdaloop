use crate::cxx_qt_shoop::qobj_test_screen_grabber_bridge::{
    ffi::register_qml_singleton_test_screen_grabber, TestScreenGrabber,
};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use cxx_qt_lib_shoop::qobject::QObject;
use std::pin::Pin;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_test_screen_grabber(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

impl TestScreenGrabber {
    pub fn add_window(self: Pin<&mut TestScreenGrabber>, window: *mut QObject) {
        self.rust_mut().windows.insert(window);
    }

    pub fn remove_window(self: Pin<&mut TestScreenGrabber>, window: *mut QObject) {
        self.rust_mut().windows.remove(&window);
    }

    pub fn grab_all(self: Pin<&mut TestScreenGrabber>, output_folder: QString) {
        todo!();
    }
}
