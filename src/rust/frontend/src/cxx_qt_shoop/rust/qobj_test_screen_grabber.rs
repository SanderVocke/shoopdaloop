use crate::cxx_qt_shoop::qobj_test_screen_grabber_bridge::{
    ffi::register_qml_singleton_test_screen_grabber, TestScreenGrabber,
};
use common::logging::macros::*;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use cxx_qt_lib_shoop::{
    qobject::QObject,
    qquickwindow_helpers::ffi::{
        qobject_as_qquickwindow_grab_and_save, qobject_as_qquickwindow_title,
    },
};
use std::{path::PathBuf, pin::Pin};
shoop_log_unit!("Frontend.TestScreenGrabber");

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
        let folder = PathBuf::from(output_folder.to_string());
        if !folder.exists() {
            if let Err(e) = std::fs::create_dir_all(folder) {
                error!("Could not grab screens: failed to create folder {output_folder:?}: {e}");
                return;
            }
        }

        for window in self.windows.iter() {
            if let Err(e) = || -> Result<(), anyhow::Error> {
                unsafe {
                    let mut title = qobject_as_qquickwindow_title(*window)?.to_string();
                    title = title.replace(" ", "_");
                    if title.is_empty() {
                        title = "unnamed".to_string();
                    }

                    let create_path = |t| PathBuf::from(format!("{output_folder:?}/{t}.png"));

                    if create_path(title.clone()).exists() {
                        let mut try_idx = 1;
                        while create_path(format!("{title}_{try_idx}")).exists() {
                            try_idx += 1;
                        }
                        title = format!("{title}_{try_idx}");
                    }

                    let path = QString::from(format!("{output_folder:?}/{title}.png"));

                    qobject_as_qquickwindow_grab_and_save(*window, path.clone())?;

                    info!("Grabbed window to {path:?}");
                }

                Ok(())
            }() {
                error!("Failed to grab and save window image: {e}");
            }
        }
    }
}
