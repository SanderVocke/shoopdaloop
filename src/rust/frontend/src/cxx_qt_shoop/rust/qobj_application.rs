use crate::cxx_qt_shoop::qobj_application_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_application_bridge::Application;
use cxx::UniquePtr;
use cxx_qt::CxxQtType;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, Instant};

use common::logging::macros::*;
shoop_log_unit!("Frontend.Application");

impl Application {
    pub fn make_unique() -> UniquePtr<Application> {
        make_unique_application()
    }

    pub fn do_quit(self: Pin<&mut Application>) {}

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

    pub fn reload_qml(mut self: Pin<&mut Application>, qml: &Path, quit_on_quit: bool) {
        self.as_mut().unload_qml();
        self.as_mut().load_qml(qml, quit_on_quit);
    }

    pub fn unload_qml(self: Pin<&mut Application>) {}

    pub fn load_qml(self: Pin<&mut Application>, qml: &Path, quit_on_quit: bool) {}

    pub fn execute(
        mut self: Pin<&mut Application>,
        config: config::config::ShoopConfig,
        main_qml: Option<&Path>,
    ) -> i32 {
        {
            let mut rust_mut = self.as_mut().rust_mut();
            rust_mut.config = config.clone();
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

        if main_qml.is_some() {
            let main_qml = main_qml.unwrap();
            self.as_mut().reload_qml(main_qml, true);
        }

        unsafe { self.as_mut().exec() }
    }
}
