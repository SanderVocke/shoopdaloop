#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type TestScreenGrabber = super::TestScreenGrabberRust;

        #[qinvokable]
        pub fn add_window(self: Pin<&mut TestScreenGrabber>, window: *mut QObject);

        #[qinvokable]
        pub fn remove_window(self: Pin<&mut TestScreenGrabber>, window: *mut QObject);

        #[qinvokable]
        pub fn grab_all(self: Pin<&mut TestScreenGrabber>, output_folder: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/make_unique.h");

        #[rust_name = "make_unique_test_screen_grabber"]
        fn make_unique() -> UniquePtr<TestScreenGrabber>;

        include!("cxx-qt-lib-shoop/register_qml_type.h");

        #[rust_name = "register_qml_singleton_test_screen_grabber"]
        unsafe fn register_qml_singleton(
            inference_example: *mut TestScreenGrabber,
            module_name: &mut String,
            version_major: i64,
            version_minor: i64,
            type_name: &mut String,
        );
    }
}

pub use ffi::TestScreenGrabber;
use std::collections::BTreeSet;

pub struct TestScreenGrabberRust {
    pub windows: BTreeSet<*mut ffi::QObject>,
}

impl Default for TestScreenGrabberRust {
    fn default() -> Self {
        Self {
            windows: BTreeSet::default(),
        }
    }
}
