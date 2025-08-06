#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-shoop/qml_debugging.h");
        #[rust_name = "enable_qml_debugging"]
        pub fn enableQmlDebugging(wait: bool, port: i32);
    }
}

pub use ffi::enable_qml_debugging;
