pub use crate::cxx_qt_shoop::qobj_qmlengine_bridge::QmlEngine;
use crate::cxx_qt_shoop::qobj_qmlengine_bridge::{self, ffi::*};
use crate::engine_update_thread;
use anyhow;
use common::util::PATH_LIST_SEPARATOR;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::{connect, connection_types};
use std::path::Path;
use std::pin::Pin;

use common::logging::macros::*;
shoop_log_unit!("Frontend.QmlEngine");

impl QmlEngine {
    pub unsafe fn make_raw(parent: *mut QObject) -> *mut Self {
        make_raw_qmlengine(parent)
    }

    fn initialize_qml_paths(mut self: Pin<&mut Self>) {
        match std::env::var("SHOOP_QML_PATHS") {
            Ok(paths) => {
                for path in paths.split(PATH_LIST_SEPARATOR) {
                    self.as_mut().add_import_path(&QString::from(path));
                }
            }
            Err(_) => {}
        }
    }

    fn initialize_backend_updates(mut self: Pin<&mut Self>) -> Result<(), anyhow::Error> {
        let window = self.as_mut().get_root_window()?;
        if !window.is_null() {
            unsafe {
                let update_thread = engine_update_thread::get_engine_update_thread();
                let update_thread = update_thread.ref_qobject_ptr();
                connect::connect_or_report(
                    window
                        .as_ref()
                        .ok_or(anyhow::anyhow!("Unable to get root window reference"))?,
                    "frameSwapped()",
                    update_thread
                        .as_ref()
                        .ok_or(anyhow::anyhow!("Unable to get update thread reference"))?,
                    "frontend_frame_swapped()",
                    connection_types::QUEUED_CONNECTION,
                );
            }
        } else {
            warn!(
                "Could not find top-level QQuickWindow to connect back-end refresh to GUI refresh"
            );
        }

        Ok(())
    }

    pub fn initialize(mut self: Pin<&mut Self>) -> Result<(), anyhow::Error> {
        let initialized: bool;
        {
            let self_ref = self.as_ref();
            let rust = self_ref.rust();
            initialized = rust.initialized;
        }

        if !initialized {
            {
                self.as_mut().initialize_qml_paths();
            }
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.initialized = true;
        }

        Ok(())
    }

    pub fn load_and_init_qml(
        mut self: Pin<&mut Self>,
        path: &Path,
        connect_frameswapped_to_update_thread: bool,
    ) -> Result<(), anyhow::Error> {
        // Ensure we are initialized
        self.as_mut().initialize()?;

        // Load QML file
        self.as_mut()
            .load(&QString::from(path.to_string_lossy().to_string()));

        // Set up back-end update connection
        if connect_frameswapped_to_update_thread {
            self.as_mut().initialize_backend_updates()?;
        }

        Ok(())
    }

    pub fn unload(mut self: Pin<&mut Self>) {
        self.as_mut().collect_garbage();
        self.as_mut().close_root();
    }
}

pub unsafe fn register_qml_engine(engine: *mut QmlEngine) {
    let qobj = std::pin::Pin::new_unchecked(&mut *engine).pin_mut_qobject_ptr();
    let shoop_eng = qobject_to_shoop_qml_engine_ptr(qobj);
    qobj_qmlengine_bridge::ffi::register_qml_engine(shoop_eng);
}

pub unsafe fn get_registered_qml_engine() -> Result<*mut QmlEngine, anyhow::Error> {
    let eng = qobj_qmlengine_bridge::ffi::get_registered_qml_engine()?;
    let qobj = shoop_qml_engine_to_qobject_ptr(eng);
    Ok(qobject_to_qml_engine_ptr(qobj))
}

pub unsafe fn get_qml_engine_stack(engine: Pin<&mut QmlEngine>) -> String {
    let qobj = engine.pin_mut_qobject_ptr();
    let shoop_eng = qobject_to_shoop_qml_engine_ptr(qobj);
    println!("shoop eng: {shoop_eng:?}");
    qobj_qmlengine_bridge::ffi::get_qml_engine_stack_trace(std::pin::Pin::new_unchecked(
        &mut *shoop_eng,
    ))
}
