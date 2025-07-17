use cxx::UniquePtr;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-shoop/ShoopApplication.h");
        type ShoopApplication;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = ShoopApplication]
        type Application = super::ApplicationRust;

        #[qsignal]
        pub fn exit_handler_called(self: Pin<&mut Application>);

        #[qinvokable]
        pub fn do_quit(self: Pin<&mut Application>);

        #[qinvokable]
        pub fn wait(self: Pin<&mut Application>, delay_ms: u64);

        #[inherit]
        #[cxx_name = "setApplicationName"]
        unsafe fn set_application_name(self: Pin<&mut Application>, name: &QString);

        #[inherit]
        #[cxx_name = "setApplicationVersion"]
        unsafe fn set_application_version(self: Pin<&mut Application>, version: &QString);

        #[inherit]
        #[cxx_name = "setOrganizationName"]
        unsafe fn set_organization_name(self: Pin<&mut Application>, name: &QString);

        #[inherit]
        #[cxx_name = "processEvents"]
        unsafe fn process_events(self: Pin<&mut Application>);

        #[inherit]
        #[cxx_name = "sendPostedEvents"]
        unsafe fn send_posted_events(self: Pin<&mut Application>);

        #[inherit]
        unsafe fn exec(self: Pin<&mut Application>) -> i32;
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_unique.h");

        #[rust_name = "make_unique_application"]
        fn make_unique() -> UniquePtr<Application>;
    }
}

pub use ffi::Application;

pub struct ApplicationRust {
    pub config: config::config::ShoopConfig,
}

impl Default for ApplicationRust {
    fn default() -> ApplicationRust {
        ApplicationRust {
            config: config::config::ShoopConfig::default(),
        }
    }
}
