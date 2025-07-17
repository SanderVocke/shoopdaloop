use std::pin::Pin;
use cxx::UniquePtr;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-shoop/ShoopApplication.h");
        type ShoopApplication;
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
    }

    unsafe extern "C++" {
        include!("cxx-qt-shoop/make_unique.h");

        #[rust_name = "make_unique_application"]
        fn make_unique() -> UniquePtr<Application>;
    }
}

use cxx_qt_lib::c_void;
pub use ffi::Application;

pub struct ApplicationRust {}

impl Default for ApplicationRust {
    fn default() -> ApplicationRust {
        ApplicationRust {}
    }
}
