#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib-shoop/qobject.h");
        type QObject = cxx_qt_lib_shoop::qobject::QObject;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-shoop/window_icons.h");
        #[rust_name = "set_window_icon_path_if_window"]
        pub unsafe fn setWindowIconPathIfWindow(window : *mut QObject, path : QString);
    }
}

pub use ffi::set_window_icon_path_if_window;