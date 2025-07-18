use cxx::UniquePtr;
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-shoop/ShoopApplication.h");
        type ShoopApplication;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = crate::cxx_qt_lib_shoop::qobject::QObject;
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

        include!("cxx-qt-lib-shoop/qobject.h");
        #[rust_name = "application_qobject_from_ptr"]
        unsafe fn qobjectFromPtr(obj: *mut Application) -> *mut QObject;

        #[rust_name = "application_qobject_from_ref"]
        fn qobjectFromRef(obj: &Application) -> &QObject;
    }
}

use crate::cxx_qt_lib_shoop::qobject::AsQObject;
use crate::cxx_qt_shoop::qobj_qmlengine_bridge::ffi::QmlEngine;
pub use ffi::Application;

#[derive(Default)]
pub struct ApplicationSettings {
    pub refresh_backend_on_frontend_refresh: bool,
    pub backend_backup_refresh_interval_ms: u64,
    pub nsm: bool,
    pub qml_debug_port: Option<u32>,
    pub qml_debug_wait: Option<bool>,
    pub title: String,
}
pub struct ApplicationRust {
    pub config: config::config::ShoopConfig,
    pub qml_engine: *mut QmlEngine,
    pub settings: ApplicationSettings,
    pub setup_after_qml_engine_creation: fn(qml_engine: *mut ffi::QObject),
}

impl Default for ApplicationRust {
    fn default() -> ApplicationRust {
        ApplicationRust {
            config: config::config::ShoopConfig::default(),
            qml_engine: std::ptr::null_mut(),
            settings: ApplicationSettings::default(),
            setup_after_qml_engine_creation: |_| {},
        }
    }
}

impl AsQObject for Application {
    unsafe fn mut_qobject_ptr(&mut self) -> *mut ffi::QObject {
        ffi::application_qobject_from_ptr(self as *mut Self)
    }

    unsafe fn ref_qobject_ptr(&self) -> *const ffi::QObject {
        ffi::application_qobject_from_ref(self) as *const ffi::QObject
    }
}
