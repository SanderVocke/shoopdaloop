use crate::cxx_qt_shoop::qobj_application_bridge::ffi::*;
use crate::cxx_qt_shoop::qobj_application_bridge::Application;
use cxx::UniquePtr;
use std::pin::Pin;

impl Application {
    pub fn make_unique() -> UniquePtr<Application> {
        make_unique_application()
    }

    pub fn do_quit(self: Pin<&mut Application>) {}

    pub fn wait(self: Pin<&mut Application>, delay_ms: u64) {}
}
