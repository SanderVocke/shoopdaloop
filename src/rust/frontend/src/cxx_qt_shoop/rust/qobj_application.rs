use crate::cxx_qt_shoop::qobj_application_bridge::ffi::*;
pub use crate::cxx_qt_shoop::qobj_application_bridge::Application;
use crate::cxx_qt_shoop::qobj_application_bridge::ApplicationStartupSettings;
use crate::cxx_qt_shoop::qobj_qmlengine_bridge::QmlEngine;
use crate::engine_update_thread;
use anyhow;
use cxx::UniquePtr;
use cxx_qt::CxxQtType;
use cxx_qt_lib_shoop::qobject::ffi::qobject_set_property_int;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::qvariant_qobject::qobject_ptr_to_qvariant;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, Instant};

use common::logging::macros::*;
shoop_log_unit!("Frontend.Application");

impl Application {
    pub unsafe fn rust_exit(self: Pin<&mut Application>, code: i32) {
        self.exit(code);
    }

    pub fn make_unique() -> UniquePtr<Application> {
        make_unique_application()
    }

    pub fn do_quit(self: Pin<&mut Application>) {
        todo!();
    }

    pub fn wait(mut self: Pin<&mut Application>, delay_ms: u64) {
        let duration = Duration::from_millis(delay_ms);
        let start = Instant::now();
        while start.elapsed() < duration {
            unsafe {
                self.as_mut().process_events();
                self.as_mut().send_posted_events();
            }
        }
    }

    pub fn reload_qml(mut self: Pin<&mut Application>, qml: QString) {
        let qml: PathBuf = PathBuf::from(qml.to_string());
        self.as_mut().unload_qml();
        self.as_mut().wait(50);
        match self.as_mut().load_qml(&qml) {
            Ok(_) => (),
            Err(e) => {
                error!("Unable to load QML: {e}");
            }
        }
    }

    pub fn unload_qml(mut self: Pin<&mut Application>) {
        debug!("Unloading QML.");
        let self_mut = self.as_mut();
        let mut rust_mut = self_mut.rust_mut();

        if rust_mut.qml_engine.is_null() {
            debug!("Skip unload QML - no engine");
        } else {
            unsafe {
                debug!("Deleting engine");
                let mut engine = std::pin::Pin::new_unchecked(&mut *rust_mut.qml_engine);
                engine.as_mut().unload();
                engine.as_mut().delete_later();
            }
            rust_mut.qml_engine = std::ptr::null_mut();
        }
    }

    pub fn load_qml(mut self: Pin<&mut Application>, qml: &Path) -> Result<(), anyhow::Error> {
        debug!("Load qml: {qml:?}");
        let qml_engine: *mut QmlEngine;

        unsafe {
            let self_qobj = self.as_mut().pin_mut_qobject_ptr();
            let mut rust_mut = self.as_mut().rust_mut();
            if rust_mut.qml_engine.is_null() {
                rust_mut.qml_engine = QmlEngine::make_raw(self_qobj);
            }
            qml_engine = rust_mut.qml_engine;
        }

        unsafe {
            let self_qobj: *mut cxx_qt_lib_shoop::qobject::QObject =
                self.as_mut().pin_mut_qobject_ptr();
            let self_qvariant = qobject_ptr_to_qvariant(self_qobj);
            let mut qml_engine_pin = std::pin::Pin::new_unchecked(&mut *qml_engine);
            qml_engine_pin
                .as_mut()
                .set_root_context_property(&QString::from("shoop_application"), &self_qvariant);
        }

        unsafe {
            let mut qml_engine_mut = std::pin::Pin::new_unchecked(&mut *qml_engine);
            let mut rust_mut = self.as_mut().rust_mut();
            qml_engine_mut.as_mut().initialize()?;
            (rust_mut.setup_after_qml_engine_creation)(qml_engine_mut.as_mut());
            qml_engine_mut
                .as_mut()
                .load_and_init_qml(qml, rust_mut.settings.refresh_backend_on_frontend_refresh)?;
        }

        Ok(())
    }

    pub fn initialize<QmlEngineCreatedCallback>(
        mut self: Pin<&mut Application>,
        config: config::config::ShoopConfig,
        setup_after_qml_engine_creation: QmlEngineCreatedCallback,
        main_qml: Option<&Path>,
        settings: ApplicationStartupSettings,
    ) -> Result<(), anyhow::Error>
    where
        QmlEngineCreatedCallback: FnMut(Pin<&mut QmlEngine>) + 'static,
    {
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.config = config.clone();
            rust_mut.settings = settings;
            rust_mut.setup_after_qml_engine_creation = Box::new(setup_after_qml_engine_creation);
        }

        unsafe {
            self.as_mut()
                .set_application_name(&QString::from("ShoopDaLoop"));
            self.as_mut()
                .set_organization_name(&QString::from("ShoopDaLoop"));
            self.as_mut()
                .set_application_version(&QString::from(config._version));
        }

        // Initialize metatypes, oncecells, etc.
        crate::init::init();

        // Initialize shoop engine update thread
        unsafe {
            let self_ref = self.as_ref();
            let rust = self_ref.rust();
            let update_thread = engine_update_thread::get_engine_update_thread();
            let update_thread = update_thread.mut_qobject_ptr();
            qobject_set_property_int(
                update_thread,
                "backup_timer_interval_ms".to_string(),
                &(rust.settings.backend_backup_refresh_interval_ms as i32),
            )
            .map_err(|e| anyhow::anyhow!("Unable to set timer interval property: {e}"))?;
        }

        if main_qml.is_some() {
            let main_qml = main_qml.unwrap();
            self.as_mut()
                .reload_qml(QString::from(main_qml.to_str().unwrap()));
        }

        Ok(())
    }
}
